//! NERVE-over-WebSocket protocol semantics tests.
//!
//! Verifies that the WebSocket path shares the same dispatch logic as the UDS
//! path: Ping echoes back with FINAL flag, SearchQuery/Cancel are handled
//! without a reply, and AgentTask lifecycle is accepted.
#[path = "helpers/mod.rs"]
mod helpers;

use nerve_protocol::types::{FrameFlags, MessageType};

const TOKEN: &str = "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";

/// The Ping reply carries the same request_id as the request.
#[test]
fn ping_echoes_request_id() {
    let port = helpers::start_server(TOKEN, "");
    let mut ws = helpers::connect_ws(port, TOKEN, None);

    helpers::send_nerve(
        &mut ws,
        MessageType::Ping,
        FrameFlags::FINAL,
        0xdeadbeef,
        &[],
    );
    let reply = helpers::recv_nerve(&mut ws);

    assert_eq!(reply.header.request_id, 0xdeadbeef);
}

/// The Ping reply has the FINAL flag set and the message type is Ping.
#[test]
fn ping_reply_flags_and_type() {
    let port = helpers::start_server(TOKEN, "");
    let mut ws = helpers::connect_ws(port, TOKEN, None);

    helpers::send_nerve(&mut ws, MessageType::Ping, FrameFlags::FINAL, 1, &[]);
    let reply = helpers::recv_nerve(&mut ws);

    assert_eq!(
        reply.header.msg_type,
        MessageType::Ping as u8,
        "reply message type must be Ping"
    );
    assert_eq!(
        reply.header.flags & FrameFlags::FINAL.bits(),
        FrameFlags::FINAL.bits(),
        "Ping reply must have the FINAL flag set"
    );
    assert_eq!(
        reply.header.flags & FrameFlags::STREAM.bits(),
        0,
        "Ping reply must not have the STREAM flag set"
    );
}

/// A SearchQuery is dispatched (no binary reply from the server for this
/// milestone) and the connection stays open for subsequent frames.
#[test]
fn search_query_handled_connection_stays_open() {
    let port = helpers::start_server(TOKEN, "");
    let mut ws = helpers::connect_ws(port, TOKEN, None);

    // SearchQuery — no reply expected; AI daemon is a future milestone.
    helpers::send_nerve(
        &mut ws,
        MessageType::SearchQuery,
        FrameFlags::empty(),
        42,
        b"{}",
    );

    // Confirm the connection is still alive via a subsequent Ping.
    helpers::send_nerve(&mut ws, MessageType::Ping, FrameFlags::FINAL, 43, &[]);
    let reply = helpers::recv_nerve(&mut ws);
    assert_eq!(
        reply.header.request_id, 43,
        "connection must remain open after SearchQuery"
    );
}

/// Cancel for a registered SearchQuery is accepted and the connection stays open.
#[test]
fn cancel_handled_connection_stays_open() {
    let port = helpers::start_server(TOKEN, "");
    let mut ws = helpers::connect_ws(port, TOKEN, None);

    helpers::send_nerve(
        &mut ws,
        MessageType::SearchQuery,
        FrameFlags::empty(),
        99,
        b"{}",
    );
    helpers::send_nerve(&mut ws, MessageType::Cancel, FrameFlags::FINAL, 99, &[]);

    // Connection must still be alive.
    helpers::send_nerve(&mut ws, MessageType::Ping, FrameFlags::FINAL, 100, &[]);
    let reply = helpers::recv_nerve(&mut ws);
    assert_eq!(
        reply.header.request_id, 100,
        "connection must remain open after Cancel"
    );
}

/// AgentTask lifecycle frames (Start → Event → Done) are all accepted without
/// closing the connection.
#[test]
fn agent_task_lifecycle() {
    let port = helpers::start_server(TOKEN, "");
    let mut ws = helpers::connect_ws(port, TOKEN, None);

    helpers::send_nerve(
        &mut ws,
        MessageType::AgentTaskStart,
        FrameFlags::empty(),
        200,
        b"{}",
    );
    helpers::send_nerve(
        &mut ws,
        MessageType::AgentTaskEvent,
        FrameFlags::STREAM,
        200,
        b"{}",
    );
    helpers::send_nerve(
        &mut ws,
        MessageType::AgentTaskDone,
        FrameFlags::FINAL,
        200,
        &[],
    );

    // Verify all three frames were accepted.
    helpers::send_nerve(&mut ws, MessageType::Ping, FrameFlags::FINAL, 201, &[]);
    let reply = helpers::recv_nerve(&mut ws);
    assert_eq!(reply.header.request_id, 201);
}

/// Multiple interleaved pings with distinct request_ids all get correct replies.
#[test]
fn multiple_pings_distinct_ids() {
    let port = helpers::start_server(TOKEN, "");
    let mut ws = helpers::connect_ws(port, TOKEN, None);

    let ids = [0u64, 1, u64::MAX, 42, 0xabcd];
    for &id in &ids {
        helpers::send_nerve(&mut ws, MessageType::Ping, FrameFlags::FINAL, id, &[]);
        let reply = helpers::recv_nerve(&mut ws);
        assert_eq!(
            reply.header.request_id, id,
            "request_id mismatch for id={id}"
        );
    }
}
