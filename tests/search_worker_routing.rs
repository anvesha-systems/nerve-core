use std::io::{Read, Write};
use std::os::unix::net::UnixListener;

use nerve_protocol::FrameFlags;
use nerve_protocol::codec::{decode, encode};
use nerve_protocol::constants::HEADER_SIZE;
use nerve_protocol::types::{MessageType, RequestId};

fn start_fake_search_worker(path: &str) {
    let path = path.to_string();

    std::thread::spawn(move || {
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).unwrap();

        let (mut stream, _) = listener.accept().unwrap();

        // ---- read incoming frame ----
        let frame = read_frame_from_stream(&mut stream).unwrap();
        let req_id = RequestId(frame.header.request_id);

        // reconstruct full frame to extract request_id
        // ---- emit STREAM ----
        let stream_frame = encode(
            MessageType::SearchResult,
            FrameFlags::STREAM,
            req_id,
            b"result-1",
        )
        .unwrap();

        stream.write_all(&stream_frame).unwrap();

        // ---- emit FINAL ----
        let final_frame = encode(
            MessageType::SearchResult,
            FrameFlags::FINAL,
            req_id,
            b"result-final",
        )
        .unwrap();

        stream.write_all(&final_frame).unwrap();
        stream.flush().unwrap();

        // IMPORTANT: keep worker alive briefly
        std::thread::sleep(std::time::Duration::from_millis(100));
    });
}

fn read_frame_from_stream(
    stream: &mut std::os::unix::net::UnixStream,
) -> Option<nerve_protocol::frame::OwnedFrame> {
    use nerve_protocol::codec::decode;
    use nerve_protocol::constants::HEADER_SIZE;

    let mut header = [0u8; HEADER_SIZE];
    if stream.read_exact(&mut header).is_err() {
        return None;
    }

    let payload_len = u32::from_le_bytes(header[16..20].try_into().unwrap()) as usize;

    let mut payload = vec![0u8; payload_len];
    if stream.read_exact(&mut payload).is_err() {
        return None;
    }

    let mut full = Vec::with_capacity(HEADER_SIZE + payload_len);
    full.extend_from_slice(&header);
    full.extend_from_slice(&payload);

    decode(&full)
        .ok()
        .map(|f| nerve_protocol::frame::OwnedFrame {
            header: f.header,
            payload,
        })
}

#[test]
fn search_query_is_routed_to_worker_and_streamed_back() {
    let core_sock = "/tmp/nerve_core_test.sock";
    let worker_sock = "/tmp/nerve-search-worker.sock";

    let _ = std::fs::remove_file(core_sock);
    let _ = std::fs::remove_file(worker_sock);

    // Start fake worker
    start_fake_search_worker(worker_sock);

    // Start nerve-core server
    std::thread::spawn(move || {
        nerve_core::server::run(core_sock).unwrap();
    });

    std::thread::sleep(std::time::Duration::from_millis(100));

    // Client connects to core
    let mut client = std::os::unix::net::UnixStream::connect(core_sock).unwrap();

    // Send SEARCH_QUERY
    let query = encode(
        MessageType::SearchQuery,
        FrameFlags::empty(),
        RequestId(42),
        b"test query",
    )
    .unwrap();

    client.write_all(&query).unwrap();

    // ---- read frames until FINAL ----
    let mut seen_stream = false;
    let mut seen_final = false;

    while let Some(frame) = read_frame_from_stream(&mut client) {
        match frame.header.flags {
            f if f & nerve_protocol::FrameFlags::STREAM.bits() != 0 => {
                assert_eq!(&frame.payload, b"result-1");
                seen_stream = true;
            }

            f if f & nerve_protocol::FrameFlags::FINAL.bits() != 0 => {
                assert_eq!(&frame.payload, b"result-final");
                seen_final = true;
                break;
            }

            _ => {}
        }
    }

    assert!(seen_stream, "did not receive STREAM frame");
    assert!(seen_final, "did not receive FINAL frame");
}
