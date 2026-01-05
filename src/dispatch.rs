use std::io::{Write};
use std::os::unix::net::UnixStream;

use nerve_protocol::{Frame, ProtocolError};
use nerve_protocol::codec::encode;
use nerve_protocol::types::{FrameFlags, MessageType, RequestId};

use crate::worker_client::WorkerClient;
use crate::request_table::RequestTable;
pub enum DispatchResult {
    Reply(Vec<u8>),
    NoReply,
}

pub enum DispatchAction<'a> {
    Handled,
    RouteToSearchWorker(Frame<'a>),
    Cancelled(RequestId)
}
// disapatch single decode frame

// this fn must be fast, non-blocking, and deterministic

pub fn dispatch_frame<'a>(
    stream: &mut UnixStream,
    frame: Frame<'a>,
    requests: &mut RequestTable,
) -> Result<DispatchAction<'a>, ProtocolError> {
    match frame.header.msg_type {
        x if x == MessageType::Ping as u8 => {
            handle_ping(stream, frame)?;
            Ok(DispatchAction::Handled)
        }

        x if x == MessageType::Cancel as u8 => {
            handle_cancel(frame, requests);
            Ok(DispatchAction::Cancelled(req_id))
        }

        // ONLY CHANGE: SearchQuery is no longer handled inline
        x if x == MessageType::SearchQuery as u8 => {
            Ok(DispatchAction::RouteToSearchWorker(frame))
        }

        x if x == MessageType::AgentTaskStart as u8 => {
            handle_agent_task_start(frame, requests)?;
            Ok(DispatchAction::Handled)
        }

        x if x == MessageType::AgentTaskEvent as u8 => {
            handle_agent_task_event(frame, requests)?;
            Ok(DispatchAction::Handled)
        }

        x if x == MessageType::AgentTaskDone as u8 => {
            handle_agent_task_done(frame, requests)?;
            Ok(DispatchAction::Handled)
        }

        // unknown / unsupported → ignore
        _ => Ok(DispatchAction::Handled),
    }
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
    let _ = requests.cancel(req_id);
}

fn handle_searchquery(stream: &mut UnixStream, frame: Frame<'_>, requests: &mut RequestTable) -> Result<(), ProtocolError>{
    let req_id = RequestId(frame.header.request_id);

    // register request

    let _ = requests.insert(req_id);

    // stub search chunk
    let chunks = [
        b"result chunk 1",
        b"result chunk 2",
        b"result chunk 3",
    ];

    for (i, chunk) in chunks.iter().enumerate() {
        // cancellation guard before emit
        if requests.is_cancelled(req_id) {
            requests.remove(req_id);
            return Ok(());
        }

        let is_final = i == chunks.len() -1 ;

        let flags = if is_final{
            FrameFlags::FINAL
        }else {
            FrameFlags::STREAM
        };
        let frame = encode(
            MessageType::SearchResult,
            flags,
            req_id,
            *chunk,
        )?;

        stream.write_all(&frame)
            .map_err(|_| ProtocolError::new(
                nerve_protocol::types::ProtocolErrorKind::InternalError
            ))?;
    }

    requests.remove(req_id);

    Ok(())
}

// stub handlers
fn handle_agent_task_start(
    frame: Frame<'_>, requests: &mut RequestTable) -> Result<(), ProtocolError> {
    let req_id = RequestId(frame.header.request_id);

    // register task lifecycle
    let _ = requests.insert(req_id);

    // No execution in v0.1
    Ok(())
}

fn handle_agent_task_event (_frame: Frame<'_>, _requests: &RequestTable) -> Result<(), ProtocolError> {
    // stub : events will be forwarded later
    Ok(())
}

fn handle_agent_task_done(frame: Frame<'_>, requests: &mut RequestTable) -> Result<(), ProtocolError> {
    let req_id = RequestId(frame.header.request_id);

    requests.remove(req_id);
    Ok(())
}

pub fn route_to_worker(
    client_stream: &mut UnixStream,
    frame: Frame<'_>,
    requests: &mut RequestTable,
    worker: &mut WorkerClient,
) -> Result<(), ProtocolError> {
    let req_id = RequestId(frame.header.request_id);
    let _ = requests.insert(req_id);

    // forward frame as-is
    let msg_type = MessageType::try_from(frame.header.msg_type)
        .map_err(|_| ProtocolError::new(
            nerve_protocol::types::ProtocolErrorKind::InternalError,
        ))?;

    let raw = encode(
        msg_type,
        FrameFlags::from_bits_truncate(frame.header.flags),
        req_id,
        frame.payload,
    )?;
    worker.send_raw(&raw)?;

    let mut seen_final = false;

    loop {
        if requests.is_cancelled(req_id) {
            let cancel = encode(
                MessageType::Cancel,
                FrameFlags::empty(),
                req_id,
                &[],
            )?;
            let _ = worker.send_raw(&cancel);
            requests.remove(req_id);
            return Ok(());
        }

        let wf = match worker.read_frame() {
            Ok(f) => f,

            Err(e) => {
                // ✅ EOF is OK only AFTER FINAL
                if seen_final {
                    return Ok(());
                }
                // ❌ otherwise this is a real error
                return Err(e);
            }
        };

        let flags = FrameFlags::from_bits_truncate(wf.header.flags);

        let msg_type = MessageType::try_from(wf.header.msg_type)
            .map_err(|_| ProtocolError::new(
                nerve_protocol::types::ProtocolErrorKind::InternalError,
            ))?;

        let out = encode(
            msg_type,
            flags,
            req_id,
            &wf.payload,
        )?;

        client_stream.write_all(&out)
            .map_err(|_| ProtocolError::new(
                nerve_protocol::types::ProtocolErrorKind::InternalError
            ))?;

        if flags.contains(FrameFlags::FINAL) {
            seen_final = true;
            requests.remove(req_id);
            // IMPORTANT: do NOT return here
            // let EOF end the loop cleanly
        }
    }
}