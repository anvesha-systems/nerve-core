use nerve_protocol::codec::encode;
use nerve_protocol::frame::OwnedFrame;
use nerve_protocol::request::RequestTable;
use nerve_protocol::types::{FrameFlags, MessageType, RequestId};

pub enum DispatchResult {
    Reply(Vec<u8>),
    NoReply,
}

pub fn dispatch_frame(
    requests: &mut RequestTable,
    frame: OwnedFrame,
) -> DispatchResult {
    let msg_type = match MessageType::try_from(frame.header.msg_type) {
        Ok(t) => t,
        Err(_) => return DispatchResult::NoReply,
    };

    match msg_type {
        MessageType::Ping => {
            let reply = encode(
                MessageType::Ping,
                FrameFlags::FINAL,
                RequestId(frame.header.request_id),
                &[],
            )
            .expect("encode ping");

            DispatchResult::Reply(reply)
        }

        MessageType::Cancel => {
            requests.cancel(RequestId(frame.header.request_id));
            DispatchResult::NoReply
        }

        MessageType::SearchQuery => {
            let id = RequestId(frame.header.request_id);
            requests.start(id);

            let reply = encode(
                MessageType::SearchResult,
                FrameFlags::FINAL,
                id,
                b"stub-result",
            )
            .expect("encode search result");

            requests.complete(id);
            DispatchResult::Reply(reply)
        }

        _ => DispatchResult::NoReply,
    }
}