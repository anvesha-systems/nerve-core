/// Concurrent connection and request isolation tests for nerve-core V1.
///
/// These tests verify the thread-per-connection model: multiple clients can
/// operate simultaneously, each with its own independent request table.
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
    for _ in 0..30 {
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
        .expect("failed to read ping response header");
    let frame = decode(&buf).expect("failed to decode ping response");
    assert_eq!(
        frame.header.msg_type,
        MessageType::Ping as u8,
        "expected Ping response"
    );
    assert_eq!(
        frame.header.request_id, expected_id,
        "ping response request_id mismatch"
    );
}

/// Two clients connect simultaneously and each receives correct responses
/// without interfering with each other.
#[test]
fn two_simultaneous_clients_both_receive_responses() {
    let socket_path = "/tmp/nerve_conc_two_clients.sock";
    let _ = std::fs::remove_file(socket_path);

    thread::spawn(|| server::run(socket_path).unwrap());
    wait_for_socket(socket_path);

    let mut a = UnixStream::connect(socket_path).unwrap();
    let mut b = UnixStream::connect(socket_path).unwrap();

    let ping_a = encode(MessageType::Ping, FrameFlags::FINAL, RequestId(10), &[]).unwrap();
    let ping_b = encode(MessageType::Ping, FrameFlags::FINAL, RequestId(20), &[]).unwrap();

    a.write_all(&ping_a).unwrap();
    b.write_all(&ping_b).unwrap();

    read_ping_response(&mut a, 10);
    read_ping_response(&mut b, 20);
}

/// Three clients connect and send pings; all three must receive correct echoes.
#[test]
fn three_simultaneous_clients() {
    let socket_path = "/tmp/nerve_conc_three_clients.sock";
    let _ = std::fs::remove_file(socket_path);

    thread::spawn(|| server::run(socket_path).unwrap());
    wait_for_socket(socket_path);

    let mut a = UnixStream::connect(socket_path).unwrap();
    let mut b = UnixStream::connect(socket_path).unwrap();
    let mut c = UnixStream::connect(socket_path).unwrap();

    let pings: Vec<_> = [1u64, 2, 3]
        .iter()
        .map(|&id| encode(MessageType::Ping, FrameFlags::FINAL, RequestId(id), &[]).unwrap())
        .collect();

    a.write_all(&pings[0]).unwrap();
    b.write_all(&pings[1]).unwrap();
    c.write_all(&pings[2]).unwrap();

    read_ping_response(&mut a, 1);
    read_ping_response(&mut b, 2);
    read_ping_response(&mut c, 3);
}

/// Client A sends a SearchQuery then Cancel; Client B sends a Ping.
/// Verify that A's cancel does not affect B, and B gets its response.
#[test]
fn cancel_on_one_connection_does_not_affect_another() {
    let socket_path = "/tmp/nerve_conc_cancel_isolation.sock";
    let _ = std::fs::remove_file(socket_path);

    thread::spawn(|| server::run(socket_path).unwrap());
    wait_for_socket(socket_path);

    let mut a = UnixStream::connect(socket_path).unwrap();
    let mut b = UnixStream::connect(socket_path).unwrap();

    // Connection A: register a SearchQuery and immediately cancel it.
    let query_a = encode(
        MessageType::SearchQuery,
        FrameFlags::empty(),
        RequestId(1),
        b"query",
    )
    .unwrap();
    let cancel_a = encode(MessageType::Cancel, FrameFlags::empty(), RequestId(1), &[]).unwrap();
    a.write_all(&query_a).unwrap();
    a.write_all(&cancel_a).unwrap();

    // Connection B: Ping must succeed regardless.
    let ping_b = encode(MessageType::Ping, FrameFlags::FINAL, RequestId(2), &[]).unwrap();
    b.write_all(&ping_b).unwrap();

    read_ping_response(&mut b, 2);
}

/// A client that sends malformed data (invalid NERVE magic) is disconnected.
/// A second client connected concurrently continues to work.
#[test]
fn malformed_client_does_not_crash_concurrent_good_client() {
    let socket_path = "/tmp/nerve_conc_malformed.sock";
    let _ = std::fs::remove_file(socket_path);

    thread::spawn(|| server::run(socket_path).unwrap());
    wait_for_socket(socket_path);

    let mut good = UnixStream::connect(socket_path).unwrap();
    let mut bad = UnixStream::connect(socket_path).unwrap();

    // Bad client: random bytes that are not a valid NERVE frame.
    bad.write_all(&[0xDEu8, 0xAD, 0xBE, 0xEF, 0xFF, 0xFF, 0x00, 0x00])
        .unwrap();
    drop(bad);

    // Pause to let the server close the bad connection.
    thread::sleep(Duration::from_millis(50));

    // Good client must still receive its Ping response.
    let ping = encode(MessageType::Ping, FrameFlags::FINAL, RequestId(99), &[]).unwrap();
    good.write_all(&ping).unwrap();

    read_ping_response(&mut good, 99);
}

/// An early-disconnecting client does not prevent the server from accepting
/// and serving subsequent connections.
#[test]
fn client_disconnect_does_not_block_new_connections() {
    let socket_path = "/tmp/nerve_conc_disconnect.sock";
    let _ = std::fs::remove_file(socket_path);

    thread::spawn(|| server::run(socket_path).unwrap());
    wait_for_socket(socket_path);

    // First client: connect, communicate, disconnect.
    {
        let mut first = UnixStream::connect(socket_path).unwrap();
        let ping = encode(MessageType::Ping, FrameFlags::FINAL, RequestId(1), &[]).unwrap();
        first.write_all(&ping).unwrap();
        let mut buf = [0u8; HEADER_SIZE];
        first.read_exact(&mut buf).unwrap();
        // `first` is dropped here
    }

    thread::sleep(Duration::from_millis(20));

    // Second client: must connect and communicate normally.
    let mut second = UnixStream::connect(socket_path).unwrap();
    let ping = encode(MessageType::Ping, FrameFlags::FINAL, RequestId(2), &[]).unwrap();
    second.write_all(&ping).unwrap();

    read_ping_response(&mut second, 2);
}

/// Multiple requests from the same connection use independent request IDs and
/// do not interfere with each other.
#[test]
fn concurrent_requests_same_connection() {
    let socket_path = "/tmp/nerve_conc_same_conn.sock";
    let _ = std::fs::remove_file(socket_path);

    thread::spawn(|| server::run(socket_path).unwrap());
    wait_for_socket(socket_path);

    let mut client = UnixStream::connect(socket_path).unwrap();

    // Issue two SearchQuery requests with different IDs, then cancel one.
    let query_a = encode(
        MessageType::SearchQuery,
        FrameFlags::empty(),
        RequestId(100),
        b"query A",
    )
    .unwrap();
    let query_b = encode(
        MessageType::SearchQuery,
        FrameFlags::empty(),
        RequestId(200),
        b"query B",
    )
    .unwrap();
    let cancel_a = encode(
        MessageType::Cancel,
        FrameFlags::empty(),
        RequestId(100),
        &[],
    )
    .unwrap();

    client.write_all(&query_a).unwrap();
    client.write_all(&query_b).unwrap();
    client.write_all(&cancel_a).unwrap();

    // Ping must succeed: cancelling A didn't break anything.
    let ping = encode(MessageType::Ping, FrameFlags::FINAL, RequestId(999), &[]).unwrap();
    client.write_all(&ping).unwrap();

    read_ping_response(&mut client, 999);
}
