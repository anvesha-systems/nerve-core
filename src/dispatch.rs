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

pub fn dispatch_frame(
    stream: &mut UnixStream,
    frame: Frame<'_>,
) -> Result<(), ProtocolError>{
    match frame.header.msg_type {
        x if x == nerve_protocol::types::MessageType::Ping as u8 =>{
            // echo ping
            stream.write_all(
                &nerve_protocol::codec::encode(
                    nerve_protocol::types::MessageType::Ping,
                    nerve_protocol::types::FrameFlags::empty(),
                    nerve_protocol::types::RequestId(frame.header.request_id), 
                    frame.payload,
                ).unwrap()
            ).unwrap();
        }
        _ => {
            // ignore for now
        }
    }
    Ok(())
}