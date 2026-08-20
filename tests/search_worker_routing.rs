/// Connection isolation tests.
///
/// Proves that connections are fully independent: a misbehaving, disconnecting,
/// or malformed client on one connection cannot disrupt another client's
/// connection or request table.
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

/// Two simultaneous connections can exchange Pings independently.
#[test]
fn two_clients_are_served_concurrently() {
    let socket_path = "/tmp/nerve_v1_two_clients.sock";
    let _ = std::fs::remove_file(socket_path);

    thread::spawn(|| server::run(socket_path).unwrap());
    wait_for_socket(socket_path);

    let mut client_a = UnixStream::connect(socket_path).unwrap();
    let mut client_b = UnixStream::connect(socket_path).unwrap();

    let ping_a = encode(MessageType::Ping, FrameFlags::FINAL, RequestId(1), &[]).unwrap();
    let ping_b = encode(MessageType::Ping, FrameFlags::FINAL, RequestId(2), &[]).unwrap();

    client_a.write_all(&ping_a).unwrap();
    client_b.write_all(&ping_b).unwrap();

    read_ping_response(&mut client_a, 1);
    read_ping_response(&mut client_b, 2);
}

/// A malformed client (garbage bytes) is disconnected; the other client
/// continues without interruption.
#[test]
fn malformed_client_does_not_affect_good_client() {
    let socket_path = "/tmp/nerve_v1_isolation.sock";
    let _ = std::fs::remove_file(socket_path);

    thread::spawn(|| server::run(socket_path).unwrap());
    wait_for_socket(socket_path);

    let mut good_client = UnixStream::connect(socket_path).unwrap();
    let mut bad_client = UnixStream::connect(socket_path).unwrap();

    // Bad client: send 100 bytes of garbage (invalid NERVE magic).
    bad_client.write_all(&[0xFFu8; 100]).unwrap();
    drop(bad_client);

    // Give the server time to process and close the bad connection.
    thread::sleep(Duration::from_millis(50));

    // Good client must still be operational.
    let ping = encode(MessageType::Ping, FrameFlags::FINAL, RequestId(7), &[]).unwrap();
    good_client.write_all(&ping).unwrap();

    read_ping_response(&mut good_client, 7);
}

/// Cancelling a request on connection A does not affect requests on connection B.
///
/// Each connection has its own `RequestTable`; request IDs are scoped per
/// connection, not globally.
#[test]
fn cancel_on_connection_a_does_not_affect_connection_b() {
    let socket_path = "/tmp/nerve_v1_cross_cancel.sock";
    let _ = std::fs::remove_file(socket_path);

    thread::spawn(|| server::run(socket_path).unwrap());
    wait_for_socket(socket_path);

    let mut client_a = UnixStream::connect(socket_path).unwrap();
    let mut client_b = UnixStream::connect(socket_path).unwrap();

    // Connection A: register a SearchQuery with req_id=1, then cancel it.
    let query_a = encode(
        MessageType::SearchQuery,
        FrameFlags::empty(),
        RequestId(1),
        b"query from A",
    )
    .unwrap();
    client_a.write_all(&query_a).unwrap();

    let cancel_a = encode(MessageType::Cancel, FrameFlags::empty(), RequestId(1), &[]).unwrap();
    client_a.write_all(&cancel_a).unwrap();

    // Connection B: send a Ping. Must receive a response regardless of A's cancel.
    let ping_b = encode(MessageType::Ping, FrameFlags::FINAL, RequestId(2), &[]).unwrap();
    client_b.write_all(&ping_b).unwrap();

    read_ping_response(&mut client_b, 2);
}

/// A client that disconnects mid-session does not prevent new connections.
#[test]
fn disconnected_client_does_not_block_new_connections() {
    let socket_path = "/tmp/nerve_v1_reconnect.sock";
    let _ = std::fs::remove_file(socket_path);

    thread::spawn(|| server::run(socket_path).unwrap());
    wait_for_socket(socket_path);

    // First client connects, sends partial data, then disconnects.
    {
        let mut client = UnixStream::connect(socket_path).unwrap();
        let ping = encode(MessageType::Ping, FrameFlags::FINAL, RequestId(1), &[]).unwrap();
        client.write_all(&ping).unwrap();
        // Read response so the server finishes processing before we drop.
        let mut buf = [0u8; HEADER_SIZE];
        client.read_exact(&mut buf).unwrap();
    }

    // Brief pause to let server process the EOF.
    thread::sleep(Duration::from_millis(20));

    // A fresh second client can connect and communicate normally.
    let mut second_client = UnixStream::connect(socket_path).unwrap();
    let ping = encode(MessageType::Ping, FrameFlags::FINAL, RequestId(42), &[]).unwrap();
    second_client.write_all(&ping).unwrap();

    read_ping_response(&mut second_client, 42);
}
