use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::thread;
use std::time::Duration;

use nerve_protocol::codec::{decode, encode};
use nerve_protocol::types::{FrameFlags, MessageType, RequestId};


#[test]

fn search_query_lifecycle() {
    let socket_path = "/tmp/nerve_test_search.sock";

    let _ = std::fs::remove_file(socket_path);

    let socket_path_clone = socket_path.to_string();
    let _server_handle = thread::spawn(move || nerve_core::server::run(&socket_path_clone));

    thread::sleep(Duration::from_millis(50));

    let mut client = UnixStream::connect(socket_path).expect("failed to connect");

    // Send search query
    let request_id = RequestId(42);
    let query = b"test search query";
    let search_frame = encode(
        MessageType::SearchQuery,
        FrameFlags::STREAM,
        request_id,
        query,
    )
    .expect("encode search");

    client.write_all(&search_frame).expect("write search");

    // Read response header first to determine payload size
    let mut header_buf = vec![0u8; 20];
    client
        .read_exact(&mut header_buf)
        .expect("read response header");

    // Parse just the header to get payload length
    let payload_len = u32::from_le_bytes([
        header_buf[16],
        header_buf[17],
        header_buf[18],
        header_buf[19],
    ]) as usize;

    // Read the complete frame (header + payload)
    let mut full_frame = header_buf.clone();
    if payload_len > 0 {
        let mut payload = vec![0u8; payload_len];
        client.read_exact(&mut payload).expect("read payload");
        full_frame.extend_from_slice(&payload);
    }

    let frame = decode(&full_frame).expect("decode frame");

    // Should receive SearchResult
    assert_eq!(frame.header.msg_type, MessageType::SearchResult as u8);
    assert_eq!(frame.header.request_id, request_id.0);
    assert_eq!(frame.payload, b"stub-result");

    drop(client);
    let _ = std::fs::remove_file(socket_path);
}

#[test]
fn multiple_requests_different_ids() {
    let socket_path = "/tmp/nerve_test_multi_requests.sock";

    let _ = std::fs::remove_file(socket_path);

    let socket_path_clone = socket_path.to_string();
    let _server_handle = thread::spawn(move || nerve_core::server::run(&socket_path_clone));

    thread::sleep(Duration::from_millis(50));

    let mut client = UnixStream::connect(socket_path).expect("failed to connect");

    // Send multiple different message types with different IDs
    let ping_id = RequestId(1);
    let search_id = RequestId(2);

    // Ping
    let ping_frame =
        encode(MessageType::Ping, FrameFlags::FINAL, ping_id, &[]).expect("encode ping");
    client.write_all(&ping_frame).expect("write ping");

    let mut response = vec![0u8; 20];
    client
        .read_exact(&mut response)
        .expect("read ping response");
    let decoded = decode(&response).expect("decode ping response");
    assert_eq!(decoded.header.request_id, ping_id.0);

    // Search query
    let search_frame = encode(
        MessageType::SearchQuery,
        FrameFlags::STREAM,
        search_id,
        b"query",
    )
    .expect("encode search");
    client.write_all(&search_frame).expect("write search");

    let mut header_buf = vec![0u8; 20];
    client
        .read_exact(&mut header_buf)
        .expect("read search response");

    // Parse payload length from header
    let payload_len = u32::from_le_bytes([
        header_buf[16],
        header_buf[17],
        header_buf[18],
        header_buf[19],
    ]) as usize;

    // Read complete frame
    let mut full_frame = header_buf.clone();
    if payload_len > 0 {
        let mut payload = vec![0u8; payload_len];
        client.read_exact(&mut payload).expect("read payload");
        full_frame.extend_from_slice(&payload);
    }

    let decoded = decode(&full_frame).expect("decode search response");
    assert_eq!(decoded.header.request_id, search_id.0);
    assert_eq!(decoded.header.msg_type, MessageType::SearchResult as u8);

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

    // Connect
    let mut client = UnixStream::connect(socket_path).expect("failed to connect");

    // Send ping to verify connection
    let ping_frame =
        encode(MessageType::Ping, FrameFlags::FINAL, RequestId(1), &[]).expect("encode ping");
    client.write_all(&ping_frame).expect("write ping");

    let mut response = vec![0u8; 20];
    client.read_exact(&mut response).expect("read response");

    // Close connection
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

    // write from "client"
    thread::spawn(move || {
        client.write_all(&encoded).unwrap();
    });

    // read on "server"
    let owned = nerve_core::server::read_frame(&mut server).expect("read_frame failed");

    let frame = owned.as_borrowed();

    assert_eq!(frame.header.request_id, 7);
    assert_eq!(frame.payload, payload);
}
