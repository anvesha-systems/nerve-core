#![allow(dead_code)]
/// Shared helpers for WebSocket integration tests.
///
/// Each test starts its own server on a randomly assigned port (bind to 0)
/// so tests run fully in parallel with no port conflicts.
use std::net::{Ipv4Addr, TcpStream};
use std::time::Duration;

use nerve_core::config::Config;
use nerve_protocol::codec::{decode, encode};
use nerve_protocol::frame::OwnedFrame;
use nerve_protocol::types::{FrameFlags, MessageType, RequestId};
use tungstenite::{ClientRequestBuilder, Message, WebSocket};

/// Start a WebSocket server on a free port and return the port number.
///
/// `extension_id` — non-empty string enables Origin checking;
/// empty string disables it (test mode).
pub fn start_server(token: &str, extension_id: &str) -> u16 {
    use std::net::TcpListener;

    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0u16)).unwrap();
    let port = listener.local_addr().unwrap().port();

    let config = Config {
        bind_addr: Ipv4Addr::LOCALHOST,
        ws_port: port,
        allowed_extension_id: extension_id.to_string(),
        token_path: format!("/tmp/.anvesha-test-{port}").into(),
        uds_path: format!("/tmp/nerve-test-{port}.sock"),
    };
    let tok = token.to_string();

    std::thread::spawn(move || {
        let _ = nerve_core::ws_server::run_ws_on_listener(listener, config, tok);
    });

    // Poll until the server accepts TCP connections.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match TcpStream::connect(format!("127.0.0.1:{port}")) {
            Ok(_) => break,
            Err(_) => {
                assert!(
                    std::time::Instant::now() < deadline,
                    "test WebSocket server did not start within 5s"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
    // Brief pause so the server finishes the spurious probe connection before
    // the real test connects.
    std::thread::sleep(Duration::from_millis(20));

    port
}

/// Build a `ClientRequestBuilder` for the test server.
///
/// - If `token` is empty, the `Sec-WebSocket-Protocol` header is omitted.
/// - If `origin` is `None`, the `Origin` header is omitted.
fn make_request_builder(port: u16, token: &str, origin: Option<&str>) -> ClientRequestBuilder {
    let uri: tungstenite::http::Uri = format!("ws://127.0.0.1:{port}/").parse().unwrap();

    let mut b = ClientRequestBuilder::new(uri);

    if !token.is_empty() {
        b = b.with_sub_protocol(format!("anvesha-v1.{token}"));
    }
    if let Some(o) = origin {
        b = b.with_header("Origin", o);
    }
    b
}

/// Connect to the test server with auth headers; panics on failure.
pub fn connect_ws(port: u16, token: &str, origin: Option<&str>) -> WebSocket<TcpStream> {
    try_connect_ws(port, token, origin).expect("WebSocket connect failed")
}

/// Attempt a WebSocket connection; returns `Err` on handshake failure (e.g. 403).
#[allow(clippy::result_large_err)] // tungstenite::Error is large; we cannot change it
pub fn try_connect_ws(
    port: u16,
    token: &str,
    origin: Option<&str>,
) -> Result<WebSocket<TcpStream>, tungstenite::Error> {
    let tcp =
        TcpStream::connect(format!("127.0.0.1:{port}")).expect("TCP connect to test server failed");

    tungstenite::client(make_request_builder(port, token, origin), tcp)
        .map(|(ws, _)| ws)
        .map_err(|e| match e {
            tungstenite::HandshakeError::Failure(e) => e,
            tungstenite::HandshakeError::Interrupted(_) => {
                panic!("unexpected non-blocking handshake state")
            }
        })
}

/// Encode and send one NERVE frame as a binary WebSocket message.
pub fn send_nerve(
    ws: &mut WebSocket<TcpStream>,
    msg_type: MessageType,
    flags: FrameFlags,
    request_id: u64,
    payload: &[u8],
) {
    let bytes = encode(msg_type, flags, RequestId(request_id), payload).unwrap();
    ws.send(Message::Binary(bytes))
        .expect("WebSocket send failed");
}

/// Read the next binary WebSocket message and decode it as a NERVE frame.
///
/// Skips WebSocket-level Ping/Pong control frames.
/// Panics if the message is not binary or the NERVE decode fails.
pub fn recv_nerve(ws: &mut WebSocket<TcpStream>) -> OwnedFrame {
    loop {
        let msg = ws.read().expect("WebSocket read failed");
        match msg {
            Message::Binary(data) => {
                let frame = decode(&data).expect("NERVE decode failed");
                return OwnedFrame {
                    header: frame.header,
                    payload: frame.payload.to_vec(),
                };
            }
            Message::Ping(_) | Message::Pong(_) => continue,
            other => panic!("expected binary NERVE frame, got: {other:?}"),
        }
    }
}

/// Build a raw 20-byte NERVE header with the given payload_length field.
///
/// The caller controls whether this produces a valid or deliberately malformed frame.
pub fn raw_nerve_header(msg_type: u8, flags: u8, request_id: u64, payload_length: u32) -> Vec<u8> {
    use nerve_protocol::constants::{MAGIC, VERSION};
    let mut buf = Vec::with_capacity(20);
    buf.extend_from_slice(&MAGIC.to_le_bytes());
    buf.extend_from_slice(&VERSION.to_le_bytes());
    buf.push(msg_type);
    buf.push(flags);
    buf.extend_from_slice(&request_id.to_le_bytes());
    buf.extend_from_slice(&payload_length.to_le_bytes());
    buf
}
