//! Concurrent WebSocket connection tests.
//!
//! Verifies thread-per-connection isolation: multiple clients running
//! simultaneously do not share state, and a misbehaving client cannot
//! affect other connections.
#[path = "helpers/mod.rs"]
mod helpers;

use nerve_protocol::types::{FrameFlags, MessageType};

const TOKEN: &str = "cafecafecafecafecafecafecafecafecafecafecafecafecafecafecafecafe";

/// Two simultaneous clients each receive correct ping replies.
#[test]
fn two_concurrent_pings() {
    let port = helpers::start_server(TOKEN, "");

    let mut ws_a = helpers::connect_ws(port, TOKEN, None);
    let mut ws_b = helpers::connect_ws(port, TOKEN, None);

    helpers::send_nerve(&mut ws_a, MessageType::Ping, FrameFlags::FINAL, 1, &[]);
    helpers::send_nerve(&mut ws_b, MessageType::Ping, FrameFlags::FINAL, 2, &[]);

    let reply_a = helpers::recv_nerve(&mut ws_a);
    let reply_b = helpers::recv_nerve(&mut ws_b);

    assert_eq!(reply_a.header.request_id, 1);
    assert_eq!(reply_b.header.request_id, 2);
}

/// Three simultaneous clients all get correct ping replies.
#[test]
fn three_concurrent_clients() {
    let port = helpers::start_server(TOKEN, "");

    let mut ws_a = helpers::connect_ws(port, TOKEN, None);
    let mut ws_b = helpers::connect_ws(port, TOKEN, None);
    let mut ws_c = helpers::connect_ws(port, TOKEN, None);

    helpers::send_nerve(&mut ws_a, MessageType::Ping, FrameFlags::FINAL, 10, &[]);
    helpers::send_nerve(&mut ws_b, MessageType::Ping, FrameFlags::FINAL, 20, &[]);
    helpers::send_nerve(&mut ws_c, MessageType::Ping, FrameFlags::FINAL, 30, &[]);

    let ra = helpers::recv_nerve(&mut ws_a);
    let rb = helpers::recv_nerve(&mut ws_b);
    let rc = helpers::recv_nerve(&mut ws_c);

    assert_eq!(ra.header.request_id, 10);
    assert_eq!(rb.header.request_id, 20);
    assert_eq!(rc.header.request_id, 30);
}

/// A bad frame on one connection closes only that connection; other connections
/// continue to work normally.
#[test]
fn bad_frame_closes_one_connection_only() {
    use tungstenite::Message;

    let port = helpers::start_server(TOKEN, "");

    let mut good = helpers::connect_ws(port, TOKEN, None);
    let mut bad = helpers::connect_ws(port, TOKEN, None);

    // Send a malformed frame on the `bad` connection.
    bad.send(Message::Binary(vec![0u8; 20])).unwrap(); // magic 0 ≠ NERV

    // The server closes the `bad` connection.
    assert!(
        bad.read().is_err(),
        "bad connection must be closed by server"
    );

    // The `good` connection still works.
    helpers::send_nerve(&mut good, MessageType::Ping, FrameFlags::FINAL, 99, &[]);
    let reply = helpers::recv_nerve(&mut good);
    assert_eq!(
        reply.header.request_id, 99,
        "good connection must remain functional"
    );
}

/// Cancel on one connection cannot affect a SearchQuery registered on a
/// different connection (per-connection RequestTable isolation).
#[test]
fn cancel_isolation_between_connections() {
    let port = helpers::start_server(TOKEN, "");

    let mut ws_a = helpers::connect_ws(port, TOKEN, None);
    let mut ws_b = helpers::connect_ws(port, TOKEN, None);

    // Register a SearchQuery on connection A with request_id 777.
    helpers::send_nerve(
        &mut ws_a,
        MessageType::SearchQuery,
        FrameFlags::empty(),
        777,
        b"{}",
    );

    // Send a Cancel for request_id 777 on connection B (different RequestTable).
    helpers::send_nerve(&mut ws_b, MessageType::Cancel, FrameFlags::FINAL, 777, &[]);

    // Both connections must still be alive — verified by a successful ping.
    helpers::send_nerve(&mut ws_a, MessageType::Ping, FrameFlags::FINAL, 1, &[]);
    helpers::send_nerve(&mut ws_b, MessageType::Ping, FrameFlags::FINAL, 2, &[]);

    let ra = helpers::recv_nerve(&mut ws_a);
    let rb = helpers::recv_nerve(&mut ws_b);

    assert_eq!(ra.header.request_id, 1);
    assert_eq!(rb.header.request_id, 2);
}

/// Disconnect and reconnect: a new connection after a previous one closes
/// authenticates and responds to pings correctly.
#[test]
fn reconnect_after_disconnect() {
    let port = helpers::start_server(TOKEN, "");

    {
        let mut ws = helpers::connect_ws(port, TOKEN, None);
        helpers::send_nerve(&mut ws, MessageType::Ping, FrameFlags::FINAL, 1, &[]);
        let reply = helpers::recv_nerve(&mut ws);
        assert_eq!(reply.header.request_id, 1);
        // `ws` is dropped here, closing the connection.
    }

    // Second connection on the same server.
    let mut ws2 = helpers::connect_ws(port, TOKEN, None);
    helpers::send_nerve(&mut ws2, MessageType::Ping, FrameFlags::FINAL, 2, &[]);
    let reply2 = helpers::recv_nerve(&mut ws2);
    assert_eq!(reply2.header.request_id, 2);
}
