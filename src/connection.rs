use core::borrow;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

use nerve_protocol::constants::HEADER_SIZE;
use nerve_protocol::{Frame, ProtocolError};
use nerve_protocol::codec::decode;

use nerve_protocol::io::{FrameReader};
use nerve_protocol::frame::{self, OwnedFrame};
use nerve_protocol::request::RequestTable;

use crate::dispatch::{dispatch_frame, DispatchResult};


/// Handle a single client connection.
///
/// This function blocks until:
/// - client disconnects
/// - protocol error occurs
/// - shutdown is requested
/// 
pub fn handle_connection(mut stream: UnixStream){
    loop{
        match read_frame(&mut stream){
            Ok(frame) => {
                let borrowed = frame.as_borrowed();
                if let Err(e) = dispatch_frame((&mut stream), borrowed){
                    log_protocol_error(e);
                    break;
                }
            }
            Err(e) => {
                log_protocol_error(e);
                break;
            }
        }
    }
    // implicit close on drop
}
pub fn read_frame(stream: &mut UnixStream) -> Result<OwnedFrame, ProtocolError> {
    // 1. Read header
    let mut header_buf = [0u8; HEADER_SIZE];
    stream.read_exact(&mut header_buf)
        .map_err(|_| ProtocolError::new(
            nerve_protocol::types::ProtocolErrorKind::MalformedFrame
        ))?;

    // 2. Extract payload length
    let payload_len = u32::from_le_bytes(
        header_buf[16..20].try_into().unwrap()
    ) as usize;

    // 3. Read payload
    let mut payload = vec![0u8; payload_len];
    stream.read_exact(&mut payload)
        .map_err(|_| ProtocolError::new(
            nerve_protocol::types::ProtocolErrorKind::MalformedFrame
        ))?;

    // 4. Decode using a temporary buffer
    let mut full_buf = Vec::with_capacity(HEADER_SIZE + payload_len);
    full_buf.extend_from_slice(&header_buf);
    full_buf.extend_from_slice(&payload);

    let frame = nerve_protocol::codec::decode(&full_buf)?;

    Ok(OwnedFrame {
        header: frame.header,
        payload,
    })
}

fn log_protocol_error(err: ProtocolError){
    eprintln!("NERVE protocol error : {}",err)
}
