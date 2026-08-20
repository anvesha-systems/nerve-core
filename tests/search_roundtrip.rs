/// V1 SearchQuery dispatch tests.
///
/// In V1, nerve-core registers a SearchQuery in the request table and returns
/// a `ForwardToAiDaemon` action. The AI daemon is not yet implemented, so no
/// response is sent back to the client. These tests verify the dispatch boundary
/// is clean: the connection stays open and accepts further messages.
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::thread;
use std::time::Duration;

use nerve_protocol::codec::{decode, encode};
use nerve_protocol::constants::HEADER_SIZE;
use nerve_protocol::types::{FrameFlags, MessageType, RequestId};

use nerve_core::server;

fn wait_for_socket(path: &str) {
    for _ in 0..20 {
        if Path::new(path).exists() {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("server socket did not appear: {}", path);
}

fn read_ping_response(stream: &mut UnixStream) -> u64 {
    let mut buf = [0u8; HEADER_SIZE];
    stream.read_exact(&mut buf).expect("read ping response");
    let frame = decode(&buf).expect("decode ping response");
    assert_eq!(frame.header.msg_type, MessageType::Ping as u8);
    frame.header.request_id
}

#[test]
fn search_query_accepted_without_disconnecting_client() {
    let socket_path = "/tmp/nerve_v1_search_dispatch.sock";
    let _ = std::fs::remove_file(socket_path);

    thread::spawn(|| server::run(socket_path).unwrap());
    wait_for_socket(socket_path);

    let mut stream = UnixStream::connect(socket_path).unwrap();

    // Send SearchQuery — no response expected; AI daemon not yet implemented.
    let query = encode(
        MessageType::SearchQuery,
        FrameFlags::empty(),
        RequestId(42),
        b"test-query",
    )
    .unwrap();
    stream.write_all(&query).unwrap();

    // A Ping after the SearchQuery must still work, proving the connection
    // was not dropped by the SearchQuery dispatch.
    let ping = encode(MessageType::Ping, FrameFlags::FINAL, RequestId(99), &[]).unwrap();
    stream.write_all(&ping).unwrap();

    let req_id = read_ping_response(&mut stream);
    assert_eq!(
        req_id, 99,
        "Ping after SearchQuery must echo the correct request_id"
    );
}

#[test]
fn multiple_search_queries_do_not_disconnect_client() {
    let socket_path = "/tmp/nerve_v1_search_multi.sock";
    let _ = std::fs::remove_file(socket_path);

    thread::spawn(|| server::run(socket_path).unwrap());
    wait_for_socket(socket_path);

    let mut stream = UnixStream::connect(socket_path).unwrap();

    for i in 1u64..=3 {
        let query = encode(
            MessageType::SearchQuery,
            FrameFlags::empty(),
            RequestId(i),
            b"another-query",
        )
        .unwrap();
        stream.write_all(&query).unwrap();
    }

    // All three SearchQueries accepted without crashing the connection.
    let ping = encode(MessageType::Ping, FrameFlags::FINAL, RequestId(100), &[]).unwrap();
    stream.write_all(&ping).unwrap();

    let req_id = read_ping_response(&mut stream);
    assert_eq!(req_id, 100);
}
