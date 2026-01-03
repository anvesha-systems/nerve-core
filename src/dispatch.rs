use std::io::{Write, empty};
use std::os::unix::net::UnixStream;

use nerve_protocol::{Frame, ProtocolError};
use nerve_protocol::codec::encode;
use nerve_protocol::frame::OwnedFrame;
use nerve_protocol::request::RequestTable;
use nerve_protocol::types::{FrameFlags, MessageType, RequestId};

pub enum DispatchResult {
    Reply(Vec<u8>),
    NoReply,
}

// disapatch single decode frame

// this fn must be fast, non-blocking, and deterministic

pub fn dispatch_frame(
    stream: &mut UnixStream,
    frame: Frame<'_>,
) -> Result<(), ProtocolError>{
    match frame.header.msg_type {
        x if x == MessageType::Ping as u8 =>{
            handle_ping(stream, frame)?;
        }

        // unknown or unsupported msg types:
        // In v0.1.0 we simple ignore them
        _ => {
            // no op
        }
    }
    Ok(())
}

fn handle_ping(stream: &mut UnixStream, frame: Frame<'_>)->Result<(), ProtocolError>{
    let response = encode(
        MessageType::Ping, FrameFlags::empty(), RequestId(frame.header.request_id), frame.payload)?;

        stream.write_all(&response)
            .map_err(|_| ProtocolError::new(
                nerve_protocol::types::ProtocolErrorKind::InternalError
            ))?;
        Ok(())
}