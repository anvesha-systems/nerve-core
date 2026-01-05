use std::io::Write;
use std::os::unix::net::UnixStream;
use std::thread;

use nerve_protocol::codec::encode;
use nerve_protocol::types::{FrameFlags, MessageType, RequestId};

#[test]
fn socket_read_frame_roundtrip() {
    // Create a connected UnixStream pair
    let (mut client, mut server) = UnixStream::pair().expect("socket pair failed");

    // Prepare a real protocol frame
    let payload = b"hello nerve";
    let encoded = encode(
        MessageType::Ping,
        FrameFlags::empty(),
        RequestId(42),
        payload,
    )
    .expect("encode failed");

    // Write from client side in another thread
    let writer = thread::spawn(move || {
        client.write_all(&encoded).expect("write failed");
    });

    // Read on server side using core logic
    let owned = nerve_core::server::read_frame(&mut server).expect("read_frame failed");

    // Convert to borrowed frame
    let frame = owned.as_borrowed();

    // Assertions — this is the contract
    assert_eq!(frame.header.request_id, 42);
    assert_eq!(frame.header.payload_length as usize, payload.len());
    assert_eq!(frame.payload, payload);

    writer.join().unwrap();
}
