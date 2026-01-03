// TODO(v0.2): Distinguish clean EOF (client disconnect)
// from malformed frames to avoid noisy logs.
// read_exact returns UnexpectedEof when client closes connection normally.

use std::io::{Read};
use std::os::unix::net::UnixStream;

use nerve_protocol::constants::HEADER_SIZE;
use nerve_protocol::{ProtocolError};

use nerve_protocol::frame::{OwnedFrame};

use crate::dispatch::{dispatch_frame};
use crate::request_table::RequestTable;


/// Handle a single client connection.
///
/// This function blocks until:
/// - client disconnects
/// - protocol error occurs
/// - shutdown is requested
/// 
/// 
/// // TODO(v0.2): Suppress logging for clean connection shutdowns (EOF)
// once read_frame differentiates EOF vs protocol errors.
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
