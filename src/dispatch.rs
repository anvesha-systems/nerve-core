use std::io::{Write, empty};
use std::os::unix::net::UnixStream;

use nerve_protocol::{Frame, ProtocolError};
use nerve_protocol::codec::encode;
use nerve_protocol::frame::OwnedFrame;
use nerve_protocol::types::{FrameFlags, MessageType, RequestId};

use crate::request_table::RequestTable;
pub enum DispatchResult {
    Reply(Vec<u8>),
    NoReply,
}

// disapatch single decode frame

// this fn must be fast, non-blocking, and deterministic

pub fn dispatch_frame(
    stream: &mut UnixStream,
    frame: Frame<'_>,
    requests: &mut RequestTable
) -> Result<(), ProtocolError>{
    match frame.header.msg_type {
        x if x == MessageType::Ping as u8 =>{
            handle_ping(stream, frame)?;
        }

        x if x == MessageType::Cancel as u8 =>{
            handle_cancel(frame, requests);
        }

        x if x == MessageType::SearchQuery as u8 => {
            handle_searchquery(stream, frame, requests)?;
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

fn handle_cancel(frame: Frame<'_>, requests: &mut RequestTable){
    let req_id = RequestId(frame.header.request_id);
    requests.cancel(req_id);
}

fn handle_searchquery(stream: &mut UnixStream, frame: Frame<'_>, requests: &mut RequestTable) -> Result<(), ProtocolError>{
    let req_id = RequestId(frame.header.request_id);

    eprintln!(
        "SEARCH_QUERY received, req_id={}",
        frame.header.request_id
    );

    // register request
    if !requests.insert(req_id) {
        // duplicate request_id =-> ignore for v0.1
        return Ok(());
    }

    let stub_result = b"stub-search-result";

    // fuard cancellation before emitting
    if requests.is_cancelled(req_id) {
        requests.remove(req_id);
        return  Ok(());
    }

    // emit a search result frame with the stub payload
    let response = encode(MessageType::SearchResult, FrameFlags::FINAL, req_id, stub_result)?;

    eprintln!(
        "Emitting SEARCH_RESULT, req_id={}",
        frame.header.request_id
    );

    stream.write_all(&response)
        .map_err(|_| ProtocolError::new(
            nerve_protocol::types::ProtocolErrorKind::InternalError
        ))?;

    eprintln!("SEARCH_RESULT written to socket");

    
    // request complete
    requests.remove(req_id);

    Ok(())
}