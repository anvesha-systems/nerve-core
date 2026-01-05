use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::thread;
use std::time::Duration;

use nerve_core::server;
use nerve_protocol::codec::decode;
use nerve_protocol::constants::HEADER_SIZE;
use nerve_protocol::types::{FrameFlags, MessageType, RequestId};

const SOCKET_PATH: &str = "/tmp/nerve_search_stream.sock";

fn read_frame(stream: &mut UnixStream) -> (u8, FrameFlags, RequestId, Vec<u8>) {
    let mut header = [0u8; HEADER_SIZE];
    stream.read_exact(&mut header).unwrap();

    let payload_len = u32::from_le_bytes(header[16..20].try_into().unwrap()) as usize;

    let mut payload = vec![0u8; payload_len];
    stream.read_exact(&mut payload).unwrap();

    let mut buf = Vec::with_capacity(header.len() + payload.len());
    buf.extend_from_slice(&header);
    buf.extend_from_slice(&payload);

    let frame = decode(&buf).unwrap();

    (
        frame.header.msg_type,
        FrameFlags::from_bits_truncate(frame.header.flags),
        RequestId(frame.header.request_id),
        payload,
    )
}

#[test]
fn search_results_are_streamed_until_final() {
    if Path::new(SOCKET_PATH).exists() {
        std::fs::remove_file(SOCKET_PATH).unwrap();
    }

    thread::spawn(|| {
        server::run(SOCKET_PATH).unwrap();
    });

    for _ in 0..20 {
        if Path::new(SOCKET_PATH).exists() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    let mut stream = UnixStream::connect(SOCKET_PATH).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();

    // Send SEARCH_QUERY
    let query = nerve_protocol::codec::encode(
        MessageType::SearchQuery,
        FrameFlags::empty(),
        RequestId(1),
        b"test query",
    )
    .unwrap();

    stream.write_all(&query).unwrap();

    let mut received = Vec::new();

    loop {
        let (msg_type, flags, req_id, payload) = read_frame(&mut stream);

        assert_eq!(msg_type, MessageType::SearchResult as u8);
        assert_eq!(req_id, RequestId(1));

        received.push(payload);

        if flags.contains(FrameFlags::FINAL) {
            break;
        }
    }

    assert_eq!(received.len(), 3);
}
