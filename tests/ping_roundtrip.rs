use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::thread;
use std::time::Duration;

use nerve_protocol::codec::{decode, encode};
use nerve_protocol::types::{FrameFlags, MessageType, RequestId};

use nerve_core::connection;
use nerve_core::server;

const SOCKET_PATH: &str = "/tmp/nerve_test.sock";

#[test]
fn ping_roundtrip_over_unix_socket() {
    if Path::new(SOCKET_PATH).exists() {
        std::fs::remove_file(SOCKET_PATH).unwrap();
    }
    // start server in background thread
    thread::spawn(|| {
        server::run(SOCKET_PATH).expect("Server failed");
    });

    // give server time to bind socket
    for _ in 0..20 {
        if Path::new(SOCKET_PATH).exists() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    assert!(
        Path::new(SOCKET_PATH).exists(),
        "Server did not create socket"
    );

    //  connect client
    let mut stream = UnixStream::connect(SOCKET_PATH).expect("Client connect failed");

    // prepare ping
    let payload = b"ping-ci-test";

    let encoded = encode(
        MessageType::Ping,
        FrameFlags::empty(),
        RequestId(99),
        payload,
    )
    .expect("encode failed");

    // send ping
    stream.write_all(&encoded).expect("write failed");

    // read response
    let mut buf = vec![0u8; 1024];

    let n = stream.read(&mut buf).expect("read failed");
    let frame = decode(&buf[..n]).expect("decode failed");

    // Assertions
    assert_eq!(frame.header.request_id, 99);
    assert_eq!(frame.payload, payload);
}
