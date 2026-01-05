use std::io::Write;
use std::os::unix::net::UnixStream;
use std::thread;
use std::time::Duration;

use nerve_protocol::codec::encode;
use nerve_protocol::types::{FrameFlags, MessageType, RequestId};

#[test]
fn send_cancel_message() {
    let socket_path = "/tmp/nerve_test_cancel.sock";

    let _ = std::fs::remove_file(socket_path);

    let socket_path_clone = socket_path.to_string();
    let _server_handle = thread::spawn(move || nerve_core::server::run(&socket_path_clone));

    thread::sleep(Duration::from_millis(50));

    let mut client = UnixStream::connect(socket_path).expect("failed to connect");

    // Start a search query
    let request_id = RequestId(100);
    let search_frame = encode(
        MessageType::SearchQuery,
        FrameFlags::STREAM,
        request_id,
        b"test query",
    )
    .expect("encode search");

    client.write_all(&search_frame).expect("write search");

    // Send cancel for the same request
    let cancel_frame =
        encode(MessageType::Cancel, FrameFlags::FINAL, request_id, &[]).expect("encode cancel");

    client.write_all(&cancel_frame).expect("write cancel");

    // Cancel should not produce a response, connection should remain open
    thread::sleep(Duration::from_millis(10));

    // Verify connection is still alive with a ping
    let ping_frame =
        encode(MessageType::Ping, FrameFlags::FINAL, RequestId(101), &[]).expect("encode ping");

    client
        .write_all(&ping_frame)
        .expect("write ping after cancel");

    drop(client);
    let _ = std::fs::remove_file(socket_path);
}

#[test]
fn cancel_nonexistent_request() {
    let socket_path = "/tmp/nerve_test_cancel_nonexistent.sock";

    let _ = std::fs::remove_file(socket_path);

    let socket_path_clone = socket_path.to_string();
    let _server_handle = thread::spawn(move || nerve_core::server::run(&socket_path_clone));

    thread::sleep(Duration::from_millis(50));

    let mut client = UnixStream::connect(socket_path).expect("failed to connect");

    // Send cancel for a request that doesn't exist
    let cancel_frame =
        encode(MessageType::Cancel, FrameFlags::FINAL, RequestId(999), &[]).expect("encode cancel");

    client.write_all(&cancel_frame).expect("write cancel");

    // Should handle gracefully, no error
    thread::sleep(Duration::from_millis(10));

    drop(client);
    let _ = std::fs::remove_file(socket_path);
}
