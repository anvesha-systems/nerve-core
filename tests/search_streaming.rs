/// V1 SearchQuery + Cancel streaming tests.
///
/// The AI daemon (not yet implemented) is responsible for producing streamed
/// SearchResult frames. These tests verify the cancellation path: a Cancel
/// frame received after a SearchQuery marks the request as cancelled, and the
/// connection remains alive for subsequent messages.
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
fn search_query_then_cancel_leaves_connection_usable() {
    let socket_path = "/tmp/nerve_v1_search_cancel_stream.sock";
    let _ = std::fs::remove_file(socket_path);

    thread::spawn(|| server::run(socket_path).unwrap());
    wait_for_socket(socket_path);

    let mut stream = UnixStream::connect(socket_path).unwrap();

    let req_id = RequestId(1);

    let query = encode(
        MessageType::SearchQuery,
        FrameFlags::empty(),
        req_id,
        b"test query",
    )
    .unwrap();
    stream.write_all(&query).unwrap();

    let cancel = encode(MessageType::Cancel, FrameFlags::empty(), req_id, &[]).unwrap();
    stream.write_all(&cancel).unwrap();

    // Connection must survive SearchQuery + Cancel; Ping must still work.
    let ping = encode(MessageType::Ping, FrameFlags::FINAL, RequestId(2), &[]).unwrap();
    stream.write_all(&ping).unwrap();

    let echoed = read_ping_response(&mut stream);
    assert_eq!(echoed, 2);
}

#[test]
fn interleaved_search_queries_and_pings() {
    let socket_path = "/tmp/nerve_v1_interleaved.sock";
    let _ = std::fs::remove_file(socket_path);

    thread::spawn(|| server::run(socket_path).unwrap());
    wait_for_socket(socket_path);

    let mut stream = UnixStream::connect(socket_path).unwrap();

    // Interleave: SearchQuery, Ping, SearchQuery, Ping
    let query_1 = encode(
        MessageType::SearchQuery,
        FrameFlags::empty(),
        RequestId(10),
        b"query 1",
    )
    .unwrap();
    let ping_1 = encode(MessageType::Ping, FrameFlags::FINAL, RequestId(11), &[]).unwrap();
    let query_2 = encode(
        MessageType::SearchQuery,
        FrameFlags::empty(),
        RequestId(12),
        b"query 2",
    )
    .unwrap();
    let ping_2 = encode(MessageType::Ping, FrameFlags::FINAL, RequestId(13), &[]).unwrap();

    stream.write_all(&query_1).unwrap();
    stream.write_all(&ping_1).unwrap();
    stream.write_all(&query_2).unwrap();
    stream.write_all(&ping_2).unwrap();

    // Both pings must be echoed back correctly.
    assert_eq!(read_ping_response(&mut stream), 11);
    assert_eq!(read_ping_response(&mut stream), 13);
}
