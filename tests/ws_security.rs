//! WebSocket authentication and input-validation security tests.
//!
//! Verifies that invalid credentials are rejected at the handshake and that
//! malformed NERVE frames close the connection without crashing the server.
#[path = "helpers/mod.rs"]
mod helpers;

use nerve_protocol::constants::MAX_PAYLOAD_SIZE;
use nerve_protocol::types::{FrameFlags, MessageType};
use tungstenite::Message;

const TOKEN: &str = "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899";
const EXT_ID: &str = "abcdefghijklmnopqrstuvwxyz012345";

fn origin() -> String {
    format!("chrome-extension://{EXT_ID}")
}

/// Wrong token → HTTP 403, connection refused at the handshake.
#[test]
fn reject_wrong_token() {
    let port = helpers::start_server(TOKEN, "");
    let result = helpers::try_connect_ws(
        port,
        "0000000000000000000000000000000000000000000000000000000000000000",
        None,
    );
    assert!(result.is_err(), "wrong token must be rejected");
}

/// Empty Sec-WebSocket-Protocol header → rejected (no token supplied).
#[test]
fn reject_missing_token() {
    let port = helpers::start_server(TOKEN, "");
    // Pass empty string so the helper omits the header entirely.
    let result = helpers::try_connect_ws(port, "", None);
    assert!(result.is_err(), "missing token must be rejected");
}

/// Correct token but wrong origin → rejected when origin check is active.
#[test]
fn reject_wrong_origin() {
    let port = helpers::start_server(TOKEN, EXT_ID);
    let result = helpers::try_connect_ws(
        port,
        TOKEN,
        Some("chrome-extension://wrong000000000000000000000000000000"),
    );
    assert!(result.is_err(), "wrong origin must be rejected");
}

/// Correct origin but wrong token → rejected.
#[test]
fn reject_correct_origin_wrong_token() {
    let port = helpers::start_server(TOKEN, EXT_ID);
    let result = helpers::try_connect_ws(
        port,
        "9999999999999999999999999999999999999999999999999999999999999999",
        Some(&origin()),
    );
    assert!(
        result.is_err(),
        "wrong token must be rejected even with correct origin"
    );
}

/// Missing Origin header → rejected when origin check is active.
#[test]
fn reject_missing_origin() {
    let port = helpers::start_server(TOKEN, EXT_ID);
    let result = helpers::try_connect_ws(port, TOKEN, None);
    assert!(
        result.is_err(),
        "missing Origin must be rejected when origin check is active"
    );
}

/// NERVE frame with payload_length > MAX_PAYLOAD_SIZE is rejected and the
/// server closes the connection (no allocation on an untrusted length field).
#[test]
fn reject_oversized_payload_header() {
    let port = helpers::start_server(TOKEN, "");
    let mut ws = helpers::connect_ws(port, TOKEN, None);

    // Build a 20-byte NERVE header claiming a payload of MAX_PAYLOAD_SIZE+1.
    // No actual payload bytes are appended; the server should reject it during
    // decode() before ever touching the payload.
    let oversized_len = (MAX_PAYLOAD_SIZE + 1) as u32;
    let bad_frame = helpers::raw_nerve_header(
        MessageType::Ping as u8,
        FrameFlags::FINAL.bits(),
        1,
        oversized_len,
    );
    ws.send(Message::Binary(bad_frame)).unwrap();

    // Server closes the connection on the malformed frame.
    let read_result = ws.read();
    assert!(
        read_result.is_err(),
        "server must close connection on oversized payload header"
    );
}

/// A NERVE frame with an invalid magic bytes is rejected and the server closes
/// the connection.
#[test]
fn reject_invalid_magic() {
    let port = helpers::start_server(TOKEN, "");
    let mut ws = helpers::connect_ws(port, TOKEN, None);

    // 20 zero bytes: magic 0x00000000 ≠ NERV
    let bad_frame = vec![0u8; 20];
    ws.send(Message::Binary(bad_frame)).unwrap();

    let read_result = ws.read();
    assert!(
        read_result.is_err(),
        "server must close connection on bad magic bytes"
    );
}

/// A frame that is too short to contain a NERVE header (< 20 bytes) is rejected.
#[test]
fn reject_truncated_frame() {
    let port = helpers::start_server(TOKEN, "");
    let mut ws = helpers::connect_ws(port, TOKEN, None);

    ws.send(Message::Binary(vec![0x4e, 0x45, 0x52, 0x56]))
        .unwrap(); // just "NERV", incomplete

    let read_result = ws.read();
    assert!(
        read_result.is_err(),
        "server must close connection on truncated NERVE header"
    );
}

/// A WebSocket binary message larger than MAX_PAYLOAD_SIZE + HEADER_SIZE is
/// rejected at the WebSocket frame layer — before nerve-core reads or allocates
/// any payload bytes.
///
/// This is a separate defence from the NERVE codec's payload_length check:
/// it fires on the WebSocket message size limit, so even a message that does
/// not contain a NERVE header at all is rejected if it exceeds the limit.
#[test]
fn reject_oversized_websocket_message() {
    use nerve_protocol::constants::{HEADER_SIZE, MAX_PAYLOAD_SIZE};

    let port = helpers::start_server(TOKEN, "");
    let mut ws = helpers::connect_ws(port, TOKEN, None);

    // Send MAX_PAYLOAD_SIZE + HEADER_SIZE + 1 bytes as a raw binary message.
    // This exceeds the WebSocket message-size limit configured in nerve_ws_config()
    // and must be rejected by the WebSocket layer before any NERVE parsing.
    let oversized = vec![0u8; MAX_PAYLOAD_SIZE + HEADER_SIZE + 1];
    // Client-side send has no size limit; the server will reject on read.
    let _ = ws.send(Message::Binary(oversized));

    // Server closes the connection; client gets an error or a Close frame.
    let result = ws.read();
    assert!(
        result.is_err() || matches!(result, Ok(Message::Close(_))),
        "server must close connection when WebSocket message exceeds MAX_PAYLOAD_SIZE + HEADER_SIZE"
    );
}

/// The Config default always binds to 127.0.0.1, never 0.0.0.0.
#[test]
fn default_config_binds_loopback_only() {
    use std::net::Ipv4Addr;
    let config = nerve_core::config::Config::default();
    assert_eq!(
        config.bind_addr,
        Ipv4Addr::LOCALHOST,
        "bind_addr must be 127.0.0.1"
    );
}
