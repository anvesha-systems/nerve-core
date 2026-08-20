use nerve_protocol::codec::encode;
use nerve_protocol::types::{FrameFlags, MessageType, RequestId};
use nerve_protocol::{Frame, ProtocolError};

use crate::request_table::RequestTable;

/// Outcome returned to the connection loop after dispatching a frame.
pub enum DispatchAction {
    /// Frame was handled and requires no response.
    Handled,
    /// Frame generated a response; the transport must send these bytes.
    Reply(Vec<u8>),
    /// SearchQuery was received and registered. The future AI daemon should
    /// process this request. The `request_id` is already in the request table.
    ForwardToAiDaemon(RequestId),
}

/// Dispatch a single decoded frame and return the action to take.
///
/// This function is transport-agnostic: it does not perform any I/O.
/// The caller is responsible for sending any `Reply` bytes back to the client,
/// whether over a Unix socket or a WebSocket.
///
/// Must be fast, non-blocking, and deterministic.
pub fn dispatch_frame(
    frame: Frame<'_>,
    requests: &mut RequestTable,
) -> Result<DispatchAction, ProtocolError> {
    let msg_type = match MessageType::try_from(frame.header.msg_type) {
        Ok(t) => t,
        Err(_) => return Ok(DispatchAction::Handled),
    };

    match msg_type {
        MessageType::Ping => {
            let bytes = encode_ping_reply(frame)?;
            Ok(DispatchAction::Reply(bytes))
        }

        MessageType::Cancel => {
            handle_cancel(frame, requests);
            Ok(DispatchAction::Handled)
        }

        // SearchQuery belongs to the AI daemon layer. Register the request
        // here so cancellation works, then let the caller forward it.
        MessageType::SearchQuery => {
            let req_id = RequestId(frame.header.request_id);
            requests.insert(req_id);
            Ok(DispatchAction::ForwardToAiDaemon(req_id))
        }

        MessageType::AgentTaskStart => {
            let req_id = RequestId(frame.header.request_id);
            requests.insert(req_id);
            Ok(DispatchAction::Handled)
        }

        MessageType::AgentTaskEvent => Ok(DispatchAction::Handled),

        MessageType::AgentTaskDone => {
            let req_id = RequestId(frame.header.request_id);
            requests.remove(req_id);
            Ok(DispatchAction::Handled)
        }

        // AiToken / SearchResult flow from the AI daemon to the client.
        // Ignore if received in this direction.
        _ => Ok(DispatchAction::Handled),
    }
}

fn encode_ping_reply(frame: Frame<'_>) -> Result<Vec<u8>, ProtocolError> {
    encode(
        MessageType::Ping,
        FrameFlags::FINAL,
        RequestId(frame.header.request_id),
        frame.payload,
    )
}

fn handle_cancel(frame: Frame<'_>, requests: &mut RequestTable) {
    let req_id = RequestId(frame.header.request_id);
    requests.cancel(req_id);
}
