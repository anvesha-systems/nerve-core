use std::os::unix::net::UnixStream;
use std::io::{Read, Write};
use std::thread;
use std::time::Duration;

use nerve_protocol::codec::{encode, decode};
use nerve_protocol::types::{FrameFlags, MessageType, RequestId};
use nerve_protocol::constants::HEADER_SIZE;

#[test]
fn ping_roundtrip() {
    let socket_path = "/tmp/nerve_test_ping.sock";
    
    // Clean up any existing socket
    let _ = std::fs::remove_file(socket_path);
    
    // Start server in background thread
    let socket_path_clone = socket_path.to_string();
    let server_handle = thread::spawn(move || {
        nerve_core::server::run_server(&socket_path_clone)
    });
    
    // Give server time to start
    thread::sleep(Duration::from_millis(50));
    
    // Connect client
    let mut client = UnixStream::connect(socket_path).expect("failed to connect");
    
    // Send ping
    let request_id = RequestId(1);
    let ping_frame = encode(
        MessageType::Ping,
        FrameFlags::FINAL,
        request_id,
        &[]
    ).expect("encode ping");
    
    client.write_all(&ping_frame).expect("write ping");
    
    // Read response
    let mut response = vec![0u8; HEADER_SIZE];
    client.read_exact(&mut response).expect("read response");
    
    let decoded = decode(&response).expect("decode response");
    
    assert_eq!(decoded.header.msg_type, MessageType::Ping as u8);
    assert_eq!(decoded.header.request_id, request_id.0);
    assert_eq!(decoded.header.flags & FrameFlags::FINAL.bits(), FrameFlags::FINAL.bits());
    
    drop(client);
    let _ = std::fs::remove_file(socket_path);
    drop(server_handle);
}

#[test]
fn multiple_pings() {
    let socket_path = "/tmp/nerve_test_multi_ping.sock";
    
    let _ = std::fs::remove_file(socket_path);
    
    let socket_path_clone = socket_path.to_string();
    let _server_handle = thread::spawn(move || {
        nerve_core::server::run_server(&socket_path_clone)
    });
    
    thread::sleep(Duration::from_millis(50));
    
    let mut client = UnixStream::connect(socket_path).expect("failed to connect");
    
    // Send multiple pings
    for i in 1..=5 {
        let request_id = RequestId(i);
        let ping_frame = encode(
            MessageType::Ping,
            FrameFlags::FINAL,
            request_id,
            &[]
        ).expect("encode ping");
        
        client.write_all(&ping_frame).expect("write ping");
        
        let mut response = vec![0u8; HEADER_SIZE];
        client.read_exact(&mut response).expect("read response");
        
        let decoded = decode(&response).expect("decode response");
        assert_eq!(decoded.header.request_id, request_id.0);
    }
    
    drop(client);
    let _ = std::fs::remove_file(socket_path);
}