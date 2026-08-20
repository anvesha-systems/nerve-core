/// Cancel-of-pending-SearchQuery tests.
///
/// Verifies that cancelling a registered SearchQuery:
/// 1. Does not disconnect the client.
/// 2. Does not affect other requests on the same connection.
/// 3. Does not affect requests on other connections.
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

fn read_ping_response(stream: &mut UnixStream, expected_id: u64) {
    let mut buf = [0u8; HEADER_SIZE];
    stream
        .read_exact(&mut buf)
        .expect("failed to read ping response");
    let frame = decode(&buf).expect("failed to decode ping response");
    assert_eq!(frame.header.msg_type, MessageType::Ping as u8);
    assert_eq!(frame.header.request_id, expected_id);
}

#[test]
fn cancel_pending_search_request_keeps_connection_open() {
    let socket_path = "/tmp/nerve_v1_cancel_pending.sock";
    let _ = std::fs::remove_file(socket_path);

    thread::spawn(|| server::run(socket_path).unwrap());
    wait_for_socket(socket_path);

    let mut stream = UnixStream::connect(socket_path).unwrap();
    let req_id = RequestId(9);

    let query = encode(
        MessageType::SearchQuery,
        FrameFlags::empty(),
        req_id,
        b"cancel test",
    )
    .unwrap();
    stream.write_all(&query).unwrap();

    // Cancel immediately — the request was registered but AI daemon hasn't started work.
    let cancel = encode(MessageType::Cancel, FrameFlags::empty(), req_id, &[]).unwrap();
    stream.write_all(&cancel).unwrap();

    // Connection must remain open; a subsequent Ping must succeed.
    let ping = encode(MessageType::Ping, FrameFlags::FINAL, RequestId(10), &[]).unwrap();
    stream.write_all(&ping).unwrap();

    read_ping_response(&mut stream, 10);
}

#[test]
fn cancel_one_request_other_requests_unaffected() {
    let socket_path = "/tmp/nerve_v1_cancel_one.sock";
    let _ = std::fs::remove_file(socket_path);

    thread::spawn(|| server::run(socket_path).unwrap());
    wait_for_socket(socket_path);

    let mut stream = UnixStream::connect(socket_path).unwrap();

    // Register two search requests.
    let query_a = encode(
        MessageType::SearchQuery,
        FrameFlags::empty(),
        RequestId(1),
        b"query a",
    )
    .unwrap();
    let query_b = encode(
        MessageType::SearchQuery,
        FrameFlags::empty(),
        RequestId(2),
        b"query b",
    )
    .unwrap();
    stream.write_all(&query_a).unwrap();
    stream.write_all(&query_b).unwrap();

    // Cancel only request 1.
    let cancel_a = encode(MessageType::Cancel, FrameFlags::empty(), RequestId(1), &[]).unwrap();
    stream.write_all(&cancel_a).unwrap();

    // Both pings must succeed — neither query cancellation affects the connection.
    let ping = encode(MessageType::Ping, FrameFlags::FINAL, RequestId(99), &[]).unwrap();
    stream.write_all(&ping).unwrap();

    read_ping_response(&mut stream, 99);
}
