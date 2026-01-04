use std::io::{Read,Write};
use std::iter::FlatMap;
use std::os::unix::net::UnixStream;
use std::thread;
use std::time::Duration;
use std::path::Path;

use nerve_protocol::codec::{encode, decode};
use nerve_protocol::types::{MessageType, FrameFlags, RequestId};

use nerve_protocol::constants::HEADER_SIZE;
use nerve_protocol::frame::OwnedFrame;
use nerve_core::server;

const SOCKET_PATH : &str = "/tmp/nerve_search_test.sock";


fn read_frame(stream: &mut UnixStream) -> OwnedFrame {
    let mut header = [0u8; HEADER_SIZE];
    stream.read_exact(&mut header).expect("failed to read header");

    let payload_len =
        u32::from_le_bytes(header[16..20].try_into().unwrap()) as usize;

    let mut payload = vec![0u8; payload_len];
    stream.read_exact(&mut payload).expect("failed to read payload");

    let mut full = Vec::with_capacity(HEADER_SIZE + payload_len);
    full.extend_from_slice(&header);
    full.extend_from_slice(&payload);

    let frame = decode(&full).expect("decode failed");

    OwnedFrame {
        header: frame.header,
        payload: frame.payload.to_vec(),
    }
}


#[test]
fn search_query_returns_stub_result(){
    if Path::new(SOCKET_PATH).exists(){
        std::fs::remove_file(SOCKET_PATH).unwrap();
    }

    thread::spawn(||{
        server::run(SOCKET_PATH).unwrap();
    });

    // wait for socket
    for _ in 0..20 {
        if Path::new(SOCKET_PATH).exists(){
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    let mut stream = UnixStream::connect(SOCKET_PATH).unwrap();

        // time out for IPC is mandatory
    stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();

    let encode = encode(MessageType::SearchQuery, FrameFlags::empty(), RequestId(42), b"test-query").unwrap();

    stream.write_all(&encode).unwrap();

    let mut buf = vec![0u8; 1024];
    let mut got_result = false;

    let frame = read_frame(&mut stream);

    assert_eq!(frame.header.msg_type, MessageType::SearchResult as u8);
    assert_eq!(frame.header.request_id, 42);
    assert_eq!(frame.payload, b"stub-search-result");
    eprintln!("Client: SEARCH_QUERY sent, waiting for response...");
    
}