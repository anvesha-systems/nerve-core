use std::io::Write;
use std::os::unix::net::UnixStream;
use std::thread;
use std::time::Duration;
use std::path::Path;

use nerve_protocol::codec::encode;
use nerve_protocol::types::{MessageType, FrameFlags, RequestId};
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
}