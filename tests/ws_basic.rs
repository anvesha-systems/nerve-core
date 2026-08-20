//! Basic WebSocket transport tests: server startup, authentication, and ping.
#[path = "helpers/mod.rs"]
mod helpers;

use nerve_protocol::types::{FrameFlags, MessageType};

const TOKEN: &str = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";

/// Verify the server starts and accepts a TCP connection on 127.0.0.1.
#[test]
fn server_binds_and_accepts() {
    let port = helpers::start_server(TOKEN, "");
    assert!(port > 0, "expected a non-zero port");

    // A plain TCP connect should succeed (the actual WS handshake can fail,
    // but the server must be listening).
    std::net::TcpStream::connect(format!("127.0.0.1:{port}"))
        .expect("server should accept TCP connections on 127.0.0.1");
}

/// A valid token with Origin checking disabled (empty extension_id) is accepted.
#[test]
fn auth_valid_no_origin_check() {
    let port = helpers::start_server(TOKEN, "");
    let ws = helpers::try_connect_ws(port, TOKEN, None);
    assert!(
        ws.is_ok(),
        "valid token should be accepted when origin check is disabled"
    );
}

/// A valid token with a matching Origin is accepted.
#[test]
fn auth_valid_with_origin() {
    let extension_id = "abcdefghijklmnopqrstuvwxyz012345";
    let port = helpers::start_server(TOKEN, extension_id);
    let origin = format!("chrome-extension://{extension_id}");

    let ws = helpers::try_connect_ws(port, TOKEN, Some(&origin));
    assert!(
        ws.is_ok(),
        "valid token + matching origin should be accepted"
    );
}

/// A NERVE Ping roundtrip over WebSocket: the reply echoes the request_id.
#[test]
fn ping_roundtrip() {
    let port = helpers::start_server(TOKEN, "");
    let mut ws = helpers::connect_ws(port, TOKEN, None);

    helpers::send_nerve(&mut ws, MessageType::Ping, FrameFlags::FINAL, 42, &[]);
    let reply = helpers::recv_nerve(&mut ws);

    assert_eq!(reply.header.msg_type, MessageType::Ping as u8);
    assert_eq!(reply.header.request_id, 42);
    assert_eq!(
        reply.header.flags & FrameFlags::FINAL.bits(),
        FrameFlags::FINAL.bits(),
        "Ping reply must have the FINAL flag set"
    );
}

/// Multiple sequential pings all come back with the correct request_ids.
#[test]
fn multiple_sequential_pings() {
    let port = helpers::start_server(TOKEN, "");
    let mut ws = helpers::connect_ws(port, TOKEN, None);

    for id in [1u64, 100, 0xffffffff, 999] {
        helpers::send_nerve(&mut ws, MessageType::Ping, FrameFlags::FINAL, id, &[]);
        let reply = helpers::recv_nerve(&mut ws);
        assert_eq!(
            reply.header.request_id, id,
            "request_id mismatch for id={id}"
        );
    }
}
