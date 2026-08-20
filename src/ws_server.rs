/// WebSocket transport for nerve-core.
///
/// # Protocol layering
///
/// ```text
/// WebSocket binary message
///        │
///        ▼
///   NERVE frame (20-byte header + JSON payload)
///        │
///        ▼
///   dispatch_frame()  ← shared with UDS path
/// ```
///
/// One WebSocket binary message = one NERVE frame.
/// The NERVE protocol remains authoritative for message type, request ID,
/// flags, and payload.
///
/// # Security
///
/// Connections are authenticated at the WebSocket handshake via two checks:
///
/// 1. **Origin** — the `Origin` header must match `chrome-extension://<id>`
///    where `<id>` is the configured extension ID.  Origin checking is
///    disabled when `allowed_extension_id` is empty (test mode only).
///
/// 2. **Token** — the client passes the per-install secret via
///    `Sec-WebSocket-Protocol: anvesha-v1.<hex-token>`.  The server validates
///    the token and echoes back the accepted protocol string.
///
/// If either check fails the connection is rejected with HTTP 403 and no
/// application frames are processed.
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::thread;

use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::http::{HeaderValue, StatusCode};
use tungstenite::protocol::WebSocketConfig;
use tungstenite::{Message, WebSocket, accept_hdr_with_config};

use nerve_protocol::codec::decode;
use nerve_protocol::constants::{HEADER_SIZE, MAX_PAYLOAD_SIZE};
use nerve_protocol::frame::OwnedFrame;

use crate::config::Config;
use crate::dispatch::{DispatchAction, dispatch_frame};
use crate::request_table::RequestTable;

/// WebSocket configuration applied to every accepted connection.
///
/// Both `max_frame_size` and `max_message_size` are capped at the NERVE
/// payload limit plus the fixed 20-byte NERVE header.  The frame-size check
/// fires at the WebSocket frame-header level — before the payload bytes are
/// read or allocated — so an oversized binary message is rejected with
/// minimal work on the server side.
fn nerve_ws_config() -> WebSocketConfig {
    let limit = MAX_PAYLOAD_SIZE + HEADER_SIZE;
    WebSocketConfig {
        max_frame_size: Some(limit),
        max_message_size: Some(limit),
        ..WebSocketConfig::default()
    }
}

/// Bind to `config.bind_addr:config.ws_port` and accept WebSocket connections.
pub fn run_ws(config: Config, token: String) -> std::io::Result<()> {
    let listener = TcpListener::bind((config.bind_addr, config.ws_port))?;
    tracing::info!(addr = %listener.local_addr()?, "WebSocket server listening");
    run_ws_on_listener(listener, config, token)
}

/// Accept WebSocket connections from an already-bound listener.
///
/// Exposed so tests can bind on port 0, retrieve the assigned port, then
/// hand off the listener here.
pub fn run_ws_on_listener(
    listener: TcpListener,
    config: Config,
    token: String,
) -> std::io::Result<()> {
    // The bind address must always be 127.0.0.1.
    if let Ok(addr) = listener.local_addr() {
        debug_assert!(
            addr.ip() == std::net::IpAddr::V4(Ipv4Addr::LOCALHOST),
            "WebSocket server must bind only to 127.0.0.1, got {addr}"
        );
    }

    for stream in listener.incoming() {
        match stream {
            Ok(tcp) => {
                let cfg = config.clone();
                let tok = token.clone();
                thread::spawn(move || {
                    handle_ws_connection(tcp, &cfg, &tok);
                });
            }
            Err(e) => {
                tracing::warn!(error = %e, "WebSocket accept error");
            }
        }
    }
    Ok(())
}

/// Handle one authenticated WebSocket connection.
///
/// Authentication is enforced at the HTTP upgrade handshake.  If auth fails,
/// the TCP connection is closed and this function returns immediately.
// `ErrorResponse` (tungstenite's API type) is a large http::Response — we cannot shrink it.
#[allow(clippy::result_large_err)]
fn handle_ws_connection(stream: TcpStream, config: &Config, token: &str) {
    let config_c = config.clone();
    let token_c = token.to_string();

    let mut ws: WebSocket<TcpStream> = match accept_hdr_with_config(
        stream,
        move |req: &Request, resp: Response| validate_handshake(req, resp, &config_c, &token_c),
        Some(nerve_ws_config()),
    ) {
        Ok(ws) => ws,
        Err(e) => {
            // Normal for rejected connections (bad origin / token).
            tracing::debug!(error = %e, "WebSocket handshake rejected");
            return;
        }
    };

    tracing::debug!("WebSocket connection authenticated");

    let mut requests = RequestTable::new();

    loop {
        // Read one WebSocket message.  tungstenite reassembles fragmented
        // frames before returning, so we always receive a complete message.
        let msg = match ws.read() {
            Ok(m) => m,
            Err(e) => {
                tracing::debug!(error = %e, "WebSocket read error");
                break;
            }
        };

        let data: Vec<u8> = match msg {
            Message::Binary(b) => b,
            Message::Close(_) => break,
            // WebSocket Ping frames: tungstenite queues a Pong reply
            // automatically on the next send/flush; nothing else to do here.
            Message::Ping(_) | Message::Pong(_) => continue,
            // Text frames violate the protocol — NERVE is binary-only.
            Message::Text(_) | Message::Frame(_) => {
                tracing::warn!("received non-binary WebSocket frame; closing connection");
                break;
            }
        };

        // Decode the NERVE frame.  `decode` validates magic, version, message
        // type, and payload size before touching payload bytes — no pre-allocation
        // on an untrusted `payload_length` field.
        let owned: OwnedFrame = match decode_nerve_frame(&data) {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!(error = e, "invalid NERVE frame; closing connection");
                break;
            }
        };

        let action = match dispatch_frame(owned.as_borrowed(), &mut requests) {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!(error = %e, "dispatch error");
                break;
            }
        };

        // Write any reply back as a binary WebSocket message.  The read borrow
        // on `ws` has already been released so this sequential write is safe.
        match action {
            DispatchAction::Handled => {}

            DispatchAction::Reply(bytes) => {
                if let Err(e) = ws.send(Message::Binary(bytes)) {
                    tracing::warn!(error = %e, "WebSocket write error");
                    break;
                }
            }

            DispatchAction::ForwardToAiDaemon(req_id) => {
                tracing::debug!(
                    request_id = req_id.0,
                    "SearchQuery pending AI daemon (WebSocket)"
                );
            }
        }
    }

    tracing::debug!("WebSocket connection closed");
}

/// Validate the WebSocket upgrade request.
///
/// Returns HTTP 403 if either the Origin or the token is invalid.
/// Never reveals which check failed in the error response.
// `ErrorResponse` (tungstenite's API type) is a large http::Response — we cannot shrink it.
#[allow(clippy::result_large_err)]
fn validate_handshake(
    request: &Request,
    mut response: Response,
    config: &Config,
    stored_token: &str,
) -> Result<Response, ErrorResponse> {
    // ── Origin check ──────────────────────────────────────────────────────────
    if let Some(allowed_origin) = config.allowed_origin() {
        let origin = request
            .headers()
            .get("Origin")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if origin != allowed_origin {
            tracing::warn!("WebSocket rejected: invalid Origin");
            return Err(forbidden());
        }
    }

    // ── Token check via Sec-WebSocket-Protocol ────────────────────────────────
    let protocol_header = request
        .headers()
        .get("Sec-WebSocket-Protocol")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let matched = match find_auth_protocol(protocol_header, stored_token) {
        Some(p) => p,
        None => {
            tracing::warn!("WebSocket rejected: invalid or missing token");
            return Err(forbidden());
        }
    };

    // Echo back the selected protocol so the browser client can confirm it.
    match HeaderValue::from_str(matched) {
        Ok(hv) => {
            response.headers_mut().insert("Sec-WebSocket-Protocol", hv);
        }
        Err(_) => {
            // Token contains characters not valid in an HTTP header value.
            return Err(forbidden());
        }
    }

    Ok(response)
}

/// Find a `anvesha-v1.<token>` entry in the comma-separated protocol list.
///
/// Returns the full matched entry (e.g. `"anvesha-v1.abc123"`) so it can be
/// echoed back as the selected subprotocol.
fn find_auth_protocol<'a>(protocol_header: &'a str, stored_token: &str) -> Option<&'a str> {
    for entry in protocol_header.split(',') {
        let p = entry.trim();
        if let Some(provided_token) = p.strip_prefix("anvesha-v1.")
            && provided_token == stored_token
        {
            return Some(p);
        }
    }
    None
}

fn forbidden() -> ErrorResponse {
    Response::builder()
        .status(StatusCode::FORBIDDEN)
        .body(None)
        .unwrap()
}

/// Decode a NERVE frame from a complete WebSocket binary message payload.
fn decode_nerve_frame(data: &[u8]) -> Result<OwnedFrame, &'static str> {
    let frame = decode(data).map_err(|_| "NERVE frame decode failed")?;
    Ok(OwnedFrame {
        header: frame.header,
        payload: frame.payload.to_vec(),
    })
}
