use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

use nerve_protocol::codec::{encode, decode};
use nerve_protocol::types::{MessageType, FrameFlags, RequestId};

fn main() {
    let socket_path = "/tmp/nerve.sock";

    let mut stream =
        UnixStream::connect(socket_path).expect("failed to connect to nerve-core");

    let payload = b"ping-from-client";

    let encoded = encode(
        MessageType::Ping,
        FrameFlags::empty(),
        RequestId(1),
        payload,
    )
    .expect("encode failed");

    // Send ping
    stream.write_all(&encoded).expect("write failed");

    // Read response (blocking)
    let mut buf = vec![0u8; 1024];
    let n = stream.read(&mut buf).expect("read failed");

    let frame = decode(&buf[..n]).expect("decode failed");

    println!("Received response:");
    println!("request_id = {}", frame.header.request_id);
    println!("payload = {:?}", std::str::from_utf8(frame.payload).unwrap());
}