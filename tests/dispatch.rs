use nerve_protocol::frame::OwnedFrame;
use nerve_protocol::request::RequestTable;
use nerve_protocol::types::{FrameFlags, MessageType, RequestId};
use nerve_protocol::constants::*;
use nerve_protocol::codec::decode;

use nerve_core::dispatch::{dispatch_frame, DispatchResult};

fn create_test_frame(msg_type: MessageType, request_id: u64, payload: &[u8]) -> OwnedFrame {
    OwnedFrame {
        header: nerve_protocol::frame::FrameHeader {
            magic: MAGIC,
            version: VERSION,
            msg_type: msg_type as u8,
            flags: FrameFlags::FINAL.bits(),
            request_id,
            payload_length: payload.len() as u32,
        },
        payload: payload.to_vec(),
    }
}

#[test]
fn dispatch_ping_returns_reply() {
    let mut requests = RequestTable::new();
    let frame = create_test_frame(MessageType::Ping, 1, &[]);
    
    let result = dispatch_frame(&mut requests, frame);
    
    match result {
        DispatchResult::Reply(bytes) => {
            let decoded = decode(&bytes).expect("decode reply");
            assert_eq!(decoded.header.msg_type, MessageType::Ping as u8);
            assert_eq!(decoded.header.request_id, 1);
        }
        DispatchResult::NoReply => panic!("Expected reply for ping"),
    }
}

#[test]
fn dispatch_cancel_returns_no_reply() {
    let mut requests = RequestTable::new();
    let req_id = RequestId(10);
    
    // Start a request first
    requests.start(req_id);
    
    let frame = create_test_frame(MessageType::Cancel, req_id.0, &[]);
    let result = dispatch_frame(&mut requests, frame);
    
    match result {
        DispatchResult::NoReply => {
            assert!(requests.is_cancelled(req_id));
        }
        DispatchResult::Reply(_) => panic!("Cancel should not produce reply"),
    }
}

#[test]
fn dispatch_search_query_returns_result() {
    let mut requests = RequestTable::new();
    let frame = create_test_frame(MessageType::SearchQuery, 42, b"test query");
    
    let result = dispatch_frame(&mut requests, frame);
    
    match result {
        DispatchResult::Reply(bytes) => {
            let decoded = decode(&bytes).expect("decode reply");
            assert_eq!(decoded.header.msg_type, MessageType::SearchResult as u8);
            assert_eq!(decoded.header.request_id, 42);
            
            // Read payload if present
            if decoded.header.payload_length > 0 {
                let payload_start = HEADER_SIZE;
                let payload_end = payload_start + decoded.header.payload_length as usize;
                let payload = &bytes[payload_start..payload_end];
                assert_eq!(payload, b"stub-result");
            }
        }
        DispatchResult::NoReply => panic!("Expected reply for search query"),
    }
}

#[test]
fn dispatch_search_tracks_request_lifecycle() {
    let mut requests = RequestTable::new();
    let req_id = RequestId(100);
    
    let frame = create_test_frame(MessageType::SearchQuery, req_id.0, b"query");
    
    // Request should not exist before dispatch
    assert!(!requests.is_cancelled(req_id));
    
    let result = dispatch_frame(&mut requests, frame);
    
    // After dispatch, request should be completed (not in cancelled state)
    match result {
        DispatchResult::Reply(_) => {
            // Request completed successfully
            assert!(!requests.is_cancelled(req_id));
        }
        DispatchResult::NoReply => panic!("Expected reply"),
    }
}

#[test]
fn dispatch_unknown_message_type() {
    let mut requests = RequestTable::new();
    
    // Create frame with invalid message type
    let mut frame = create_test_frame(MessageType::Ping, 1, &[]);
    frame.header.msg_type = 0xFF; // Invalid type
    
    let result = dispatch_frame(&mut requests, frame);
    
    match result {
        DispatchResult::NoReply => {
            // Unknown types should be ignored
        }
        DispatchResult::Reply(_) => panic!("Unknown type should not produce reply"),
    }
}

#[test]
fn dispatch_multiple_sequential_requests() {
    let mut requests = RequestTable::new();
    
    // Dispatch multiple pings
    for i in 1..=5 {
        let frame = create_test_frame(MessageType::Ping, i, &[]);
        let result = dispatch_frame(&mut requests, frame);
        
        match result {
            DispatchResult::Reply(bytes) => {
                let decoded = decode(&bytes).expect("decode reply");
                assert_eq!(decoded.header.request_id, i);
            }
            DispatchResult::NoReply => panic!("Expected reply"),
        }
    }
}

#[test]
fn dispatch_interleaved_request_types() {
    let mut requests = RequestTable::new();
    
    // Ping
    let ping_frame = create_test_frame(MessageType::Ping, 1, &[]);
    let result = dispatch_frame(&mut requests, ping_frame);
    assert!(matches!(result, DispatchResult::Reply(_)));
    
    // Search
    let search_frame = create_test_frame(MessageType::SearchQuery, 2, b"query");
    let result = dispatch_frame(&mut requests, search_frame);
    assert!(matches!(result, DispatchResult::Reply(_)));
    
    // Cancel
    let cancel_frame = create_test_frame(MessageType::Cancel, 3, &[]);
    let result = dispatch_frame(&mut requests, cancel_frame);
    assert!(matches!(result, DispatchResult::NoReply));
    
    // Another ping
    let ping_frame2 = create_test_frame(MessageType::Ping, 4, &[]);
    let result = dispatch_frame(&mut requests, ping_frame2);
    assert!(matches!(result, DispatchResult::Reply(_)));
}
