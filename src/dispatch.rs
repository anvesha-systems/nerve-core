use std::io::Write;
use std::os::unix::net::UnixStream;

use nerve_protocol::codec::encode;
use nerve_protocol::types::{FrameFlags, MessageType, ProtocolErrorKind, RequestId};
use nerve_protocol::{Frame, ProtocolError};

use crate::request_table::RequestTable;

/// Outcome returned to the connection loop after dispatching a frame.
pub enum DispatchAction {
    /// Frame was handled inline; no further action needed.
    Handled,
    /// SearchQuery was received and registered. The future AI daemon should
    /// process this request. The request_id is already in the request table.
    ForwardToAiDaemon(RequestId),
}

/// Dispatch a single decoded frame and return the action to take.
///
/// This function must be fast, non-blocking, and deterministic.
/// Business logic that belongs to the AI daemon is NOT handled here;
/// those frames are returned as `ForwardToAiDaemon`.
pub fn dispatch_frame(
    stream: &mut UnixStream,
    frame: Frame<'_>,
    requests: &mut RequestTable,
) -> Result<DispatchAction, ProtocolError> {
    let msg_type = match MessageType::try_from(frame.header.msg_type) {
        Ok(t) => t,
        Err(_) => return Ok(DispatchAction::Handled),
    };

    match msg_type {
        MessageType::Ping => {
            handle_ping(stream, frame)?;
            Ok(DispatchAction::Handled)
        }

        MessageType::Cancel => {
            handle_cancel(frame, requests);
            Ok(DispatchAction::Handled)
        }

        // SearchQuery belongs to the AI daemon layer. Register the request
        // here so cancellation works, but do not process it inline.
        MessageType::SearchQuery => {
            let req_id = RequestId(frame.header.request_id);
            requests.insert(req_id);
            Ok(DispatchAction::ForwardToAiDaemon(req_id))
        }

        MessageType::AgentTaskStart => {
            handle_agent_task_start(frame, requests)?;
            Ok(DispatchAction::Handled)
        }

        MessageType::AgentTaskEvent => {
            handle_agent_task_event(frame, requests)?;
            Ok(DispatchAction::Handled)
        }

        MessageType::AgentTaskDone => {
            handle_agent_task_done(frame, requests)?;
            Ok(DispatchAction::Handled)
        }

        // AiToken and SearchResult flow from the AI daemon to the client, not
        // from the client to the core. Ignore if received in this direction.
        _ => Ok(DispatchAction::Handled),
    }
}

fn handle_ping(stream: &mut UnixStream, frame: Frame<'_>) -> Result<(), ProtocolError> {
    let response = encode(
        MessageType::Ping,
        FrameFlags::FINAL,
        RequestId(frame.header.request_id),
        frame.payload,
    )?;

    stream
        .write_all(&response)
        .map_err(|_| ProtocolError::new(ProtocolErrorKind::InternalError))?;
    Ok(())
}

fn handle_cancel(frame: Frame<'_>, requests: &mut RequestTable) {
    let req_id = RequestId(frame.header.request_id);
    requests.cancel(req_id);
}

fn handle_agent_task_start(
    frame: Frame<'_>,
    requests: &mut RequestTable,
) -> Result<(), ProtocolError> {
    let req_id = RequestId(frame.header.request_id);
    requests.insert(req_id);
    Ok(())
}

fn handle_agent_task_event(
    _frame: Frame<'_>,
    _requests: &RequestTable,
) -> Result<(), ProtocolError> {
    Ok(())
}

fn handle_agent_task_done(
    frame: Frame<'_>,
    requests: &mut RequestTable,
) -> Result<(), ProtocolError> {
    let req_id = RequestId(frame.header.request_id);
    requests.remove(req_id);
    Ok(())
}
