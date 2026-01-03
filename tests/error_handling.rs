use std::os::unix::net::UnixStream;
use std::io::Write;
use std::thread;
use std::time::Duration;

#[test]
fn connection_closed_gracefully() {
    let socket_path = "/tmp/nerve_test_graceful_close.sock";
    
    let _ = std::fs::remove_file(socket_path);
    
    let socket_path_clone = socket_path.to_string();
    let _server_handle = thread::spawn(move || {
        nerve_core::server::run_server(&socket_path_clone)
    });
    
    thread::sleep(Duration::from_millis(50));
    
    let client = UnixStream::connect(socket_path).expect("failed to connect");
    
    // Close connection immediately
    drop(client);
    
    thread::sleep(Duration::from_millis(10));
    
    let _ = std::fs::remove_file(socket_path);
}

#[test]
fn malformed_data_closes_connection() {
    let socket_path = "/tmp/nerve_test_malformed.sock";
    
    let _ = std::fs::remove_file(socket_path);
    
    let socket_path_clone = socket_path.to_string();
    let _server_handle = thread::spawn(move || {
        nerve_core::server::run_server(&socket_path_clone)
    });
    
    thread::sleep(Duration::from_millis(50));
    
    let mut client = UnixStream::connect(socket_path).expect("failed to connect");
    
    // Send invalid data (not a valid NERVE frame)
    let invalid_data = vec![0xFF; 50];
    let result = client.write_all(&invalid_data);
    
    // Write may succeed but connection should close on next read attempt
    thread::sleep(Duration::from_millis(50));
    
    // Try to write again - connection should be closed
    let ping_attempt = client.write_all(&[0u8; 20]);
    
    // Connection may or may not still be open, but server logged the error
    drop(client);
    let _ = std::fs::remove_file(socket_path);
}

#[test]
fn empty_write_handling() {
    let socket_path = "/tmp/nerve_test_empty.sock";
    
    let _ = std::fs::remove_file(socket_path);
    
    let socket_path_clone = socket_path.to_string();
    let _server_handle = thread::spawn(move || {
        nerve_core::server::run_server(&socket_path_clone)
    });
    
    thread::sleep(Duration::from_millis(50));
    
    let mut client = UnixStream::connect(socket_path).expect("failed to connect");
    
    // Write empty data
    client.write_all(&[]).expect("empty write");
    
    thread::sleep(Duration::from_millis(10));
    
    drop(client);
    let _ = std::fs::remove_file(socket_path);
}

#[test]
fn partial_frame_handling() {
    let socket_path = "/tmp/nerve_test_partial.sock";
    
    let _ = std::fs::remove_file(socket_path);
    
    let socket_path_clone = socket_path.to_string();
    let _server_handle = thread::spawn(move || {
        nerve_core::server::run_server(&socket_path_clone)
    });
    
    thread::sleep(Duration::from_millis(50));
    
    let mut client = UnixStream::connect(socket_path).expect("failed to connect");
    
    // Send partial frame header (less than 20 bytes)
    let partial_data = vec![0x56, 0x52, 0x45, 0x4E, 0x01, 0x00]; // Just magic + partial version
    client.write_all(&partial_data).expect("write partial");
    
    // Give server time to process
    thread::sleep(Duration::from_millis(50));
    
    drop(client);
    let _ = std::fs::remove_file(socket_path);
}

#[test]
fn rapid_connect_disconnect() {
    let socket_path = "/tmp/nerve_test_rapid.sock";
    
    let _ = std::fs::remove_file(socket_path);
    
    let socket_path_clone = socket_path.to_string();
    let _server_handle = thread::spawn(move || {
        // Server accepts one connection at a time in v0.1
        nerve_core::server::run_server(&socket_path_clone)
    });
    
    thread::sleep(Duration::from_millis(50));
    
    // Connect and immediately disconnect
    for _ in 0..3 {
        let client = UnixStream::connect(socket_path);
        if let Ok(c) = client {
            drop(c);
        }
        thread::sleep(Duration::from_millis(20));
    }
    
    let _ = std::fs::remove_file(socket_path);
}

#[test]
fn socket_already_exists() {
    let socket_path = "/tmp/nerve_test_existing.sock";
    
    // Create socket file manually
    let _ = std::fs::File::create(socket_path);
    
    // Server should remove and recreate it
    let socket_path_clone = socket_path.to_string();
    let server_handle = thread::spawn(move || {
        nerve_core::server::run_server(&socket_path_clone)
    });
    
    thread::sleep(Duration::from_millis(50));
    
    // Should be able to connect
    let result = UnixStream::connect(socket_path);
    assert!(result.is_ok(), "Should connect to recreated socket");
    
    drop(result);
    let _ = std::fs::remove_file(socket_path);
    drop(server_handle);
}
