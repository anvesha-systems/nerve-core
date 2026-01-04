use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::thread;
use std::time::Duration;
use std::path::Path;

use nerve_protocol::codec::{encode, decode};
use nerve_protocol::types::{MessageType, FrameFlags, RequestId};
use nerve_protocol::constants::HEADER_SIZE;
use nerve_core::server;

const SOCKET_PATH: &str = "/tmp/nerve_agent_task.sock";

#[test]
fn agent_task_lifecycle_is_accepted() {
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

    let req_id = RequestId(77);

    let start = encode(
        MessageType::AgentTaskStart,
        FrameFlags::empty(),
        req_id,
        b"task payload",
    ).unwrap();

    stream.write_all(&start).unwrap();

    let done = encode(
        MessageType::AgentTaskDone,
        FrameFlags::empty(),
        req_id,
        &[],
    ).unwrap();

    stream.write_all(&done).unwrap();

    // Verify the connection remains open and server is still functional
    // by sending a ping and checking the response
    let ping_req_id = RequestId(78);
    let ping = encode(
        MessageType::Ping,
        FrameFlags::FINAL,
        ping_req_id,
        &[],
    ).unwrap();

    stream.write_all(&ping).unwrap();

    // Read ping response
    let mut response = vec![0u8; HEADER_SIZE];
    stream.read_exact(&mut response).unwrap();

    let decoded = decode(&response).unwrap();

    // Assert the server responded correctly to the ping
    assert_eq!(decoded.header.msg_type, MessageType::Ping as u8);
    assert_eq!(decoded.header.request_id, ping_req_id.0);
    assert_eq!(decoded.header.flags & FrameFlags::FINAL.bits(), FrameFlags::FINAL.bits());
}