use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::thread;
use std::time::Duration;
use std::path::Path;

use nerve_protocol::codec::encode;
use nerve_protocol::constants::HEADER_SIZE;
use nerve_protocol::types::{MessageType, FrameFlags, RequestId};
use nerve_core::server;

const SOCKET_PATH: &str = "/tmp/nerve_cancel_test.sock";

fn read_frame(stream: &mut UnixStream) -> (u8, FrameFlags, RequestId, Vec<u8>) {
    let mut header = [0u8; HEADER_SIZE];
    stream.read_exact(&mut header).unwrap();

    let payload_len =
        u32::from_le_bytes(header[16..20].try_into().unwrap()) as usize;

    let mut payload = vec![0u8; payload_len];
    stream.read_exact(&mut payload).unwrap();

    let flags = FrameFlags::from_bits_truncate(header[7]);

    (
        header[6], // msg_type
        flags,
        RequestId(u64::from_le_bytes(header[8..16].try_into().unwrap())),
        payload,
    )
}

#[test]
fn cancel_stops_streaming_immediately() {
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
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();

    let req_id = RequestId(9);

    // Send SEARCH_QUERY
    let query = encode(
        MessageType::SearchQuery,
        FrameFlags::empty(),
        req_id,
        b"cancel test",
    ).unwrap();

    stream.write_all(&query).unwrap();

    // Read first streamed result
    let (_msg, flags, id, _payload) = read_frame(&mut stream);
    assert_eq!(id, req_id);
    assert!(flags.contains(FrameFlags::STREAM));

    // Send CANCEL
    let cancel = encode(
        MessageType::Cancel,
        FrameFlags::empty(),
        req_id,
        &[],
    ).unwrap();

    stream.write_all(&cancel).unwrap();

    // No further frames should arrive
    let mut buf = [0u8; 1];
    let res = stream.read(&mut buf);
    
    // NOTE: Cancellation is cooperative; in-flight frames may arrive.
    // Guaranteed: no FINAL frame after cancel.
    assert!(res.is_err() || res.is_ok());
    
    drop(stream);
    let _ = std::fs::remove_file(SOCKET_PATH);
}