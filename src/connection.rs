use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

use tracing::warn;

use nerve_protocol::io::{FrameReader};
use nerve_protocol::frame::OwnedFrame;
use nerve_protocol::request::RequestTable;

use crate::dispatch::{dispatch_frame, DispatchResult};

pub fn run(mut stream: UnixStream) -> std::io::Result<()> {
    let mut reader = FrameReader::new();
    let mut requests = RequestTable::new();

    loop {
        let frames = match reader.read_from(&mut stream) {
            Ok(frames) => frames,
            Err(e) => {
                warn!(error = %e, "protocol error, closing connection");
                break;
            }
        };

        for frame in frames {
            match dispatch_frame(&mut requests, frame) {
                DispatchResult::Reply(bytes) => {
                    stream.write_all(&bytes)?;
                }
                DispatchResult::NoReply => {}
            }
        }
    }

    Ok(())
}