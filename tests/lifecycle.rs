use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::thread;
use std::time::Duration;

use nerve_protocol::codec::{decode, encode};
use nerve_protocol::constants::HEADER_SIZE;
use nerve_protocol::types::{FrameFlags, MessageType, RequestId};

fn read_ping_response(stream: &mut UnixStream, expected_id: u64) {
    let mut header = [0u8; HEADER_SIZE];
    stream.read_exact(&mut header).expect("read ping header");
    let frame = decode(&header).expect("decode ping frame");
    assert_eq!(frame.header.msg_type, MessageType::Ping as u8);
    assert_eq!(frame.header.request_id, expected_id);
}

/// In V1, SearchQuery is dispatched to the AI daemon boundary. No response is
/// sent inline. Verify the connection stays open after a SearchQuery.
#[test]
fn search_query_dispatched_to_ai_boundary() {
    let socket_path = "/tmp/nerve_test_search.sock";
    let _ = std::fs::remove_file(socket_path);

    let socket_path_clone = socket_path.to_string();
    let _server_handle = thread::spawn(move || nerve_core::server::run(&socket_path_clone));
    thread::sleep(Duration::from_millis(50));

    let mut client = UnixStream::connect(socket_path).expect("failed to connect");

    let search_frame = encode(
        MessageType::SearchQuery,
        FrameFlags::STREAM,
        RequestId(42),
        b"test search query",
    )
    .expect("encode search");
    client.write_all(&search_frame).expect("write search");

    // No response is expected from the AI daemon boundary.
    // Connection must remain open; a Ping must echo back.
    let ping = encode(MessageType::Ping, FrameFlags::FINAL, RequestId(43), &[]).unwrap();
    client.write_all(&ping).unwrap();

    read_ping_response(&mut client, 43);

    drop(client);
    let _ = std::fs::remove_file(socket_path);
}

/// Multiple request IDs on the same connection are handled independently.
#[test]
fn multiple_requests_different_ids() {
    let socket_path = "/tmp/nerve_test_multi_requests.sock";
    let _ = std::fs::remove_file(socket_path);

    let socket_path_clone = socket_path.to_string();
    let _server_handle = thread::spawn(move || nerve_core::server::run(&socket_path_clone));
    thread::sleep(Duration::from_millis(50));

    let mut client = UnixStream::connect(socket_path).expect("failed to connect");

    // Ping req_id=1: expect an echo.
    let ping_frame =
        encode(MessageType::Ping, FrameFlags::FINAL, RequestId(1), &[]).expect("encode ping");
    client.write_all(&ping_frame).expect("write ping");
    read_ping_response(&mut client, 1);

    // SearchQuery req_id=2: dispatched to AI boundary, no inline response.
    let search_frame = encode(
        MessageType::SearchQuery,
        FrameFlags::STREAM,
        RequestId(2),
        b"query",
    )
    .expect("encode search");
    client.write_all(&search_frame).expect("write search");

    // Ping req_id=3: must still work, confirming neither request interfered.
    let ping2 = encode(MessageType::Ping, FrameFlags::FINAL, RequestId(3), &[]).unwrap();
    client.write_all(&ping2).unwrap();
    read_ping_response(&mut client, 3);

    drop(client);
    let _ = std::fs::remove_file(socket_path);
}

#[test]
fn connection_lifecycle() {
    let socket_path = "/tmp/nerve_test_connection.sock";
    let _ = std::fs::remove_file(socket_path);

    let socket_path_clone = socket_path.to_string();
    let _server_handle = thread::spawn(move || nerve_core::server::run(&socket_path_clone));
    thread::sleep(Duration::from_millis(50));

    let mut client = UnixStream::connect(socket_path).expect("failed to connect");

    let ping_frame =
        encode(MessageType::Ping, FrameFlags::FINAL, RequestId(1), &[]).expect("encode ping");
    client.write_all(&ping_frame).expect("write ping");

    let mut response = [0u8; HEADER_SIZE];
    client.read_exact(&mut response).expect("read response");

    drop(client);
    thread::sleep(Duration::from_millis(10));
    let _ = std::fs::remove_file(socket_path);
}

#[test]
fn socket_read_frame_roundtrip() {
    let (mut client, mut server) = UnixStream::pair().unwrap();

    let payload = b"hello nerve";
    let encoded = encode(
        MessageType::Ping,
        FrameFlags::empty(),
        RequestId(7),
        payload,
    )
    .unwrap();

    thread::spawn(move || {
        client.write_all(&encoded).unwrap();
    });

    let owned = nerve_core::server::read_frame(&mut server).expect("read_frame failed");
    let frame = owned.as_borrowed();

    assert_eq!(frame.header.request_id, 7);
    assert_eq!(frame.payload, payload);
}
