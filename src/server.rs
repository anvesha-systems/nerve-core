use std::io::Read;
use std::os::unix::net::{UnixListener, UnixStream};
use std::thread;

use nerve_protocol::codec::decode;
use nerve_protocol::constants::HEADER_SIZE;
use nerve_protocol::error::ProtocolError;
use nerve_protocol::frame::OwnedFrame;
use nerve_protocol::types::ProtocolErrorKind;

use crate::dispatch::{DispatchAction, dispatch_frame};
use crate::request_table::RequestTable;

/// Handle a single client connection until EOF, a protocol error, or an I/O error.
///
/// Each connection has its own `RequestTable`; cancellation on one connection
/// cannot affect another connection's requests.
pub fn handle_connection(mut client_stream: UnixStream) -> Result<(), ProtocolError> {
    let mut requests = RequestTable::new();

    loop {
        let owned = match read_frame(&mut client_stream) {
            Ok(f) => f,
            Err(e) => {
                log_protocol_error(e);
                return Ok(());
            }
        };

        let borrowed = owned.as_borrowed();

        let action = match dispatch_frame(&mut client_stream, borrowed, &mut requests) {
            Ok(a) => a,
            Err(e) => {
                log_protocol_error(e);
                return Ok(());
            }
        };

        match action {
            DispatchAction::Handled => {}

            // AI daemon boundary: the request is registered; the daemon will
            // consume it in a future milestone. Nothing to do here yet.
            DispatchAction::ForwardToAiDaemon(req_id) => {
                tracing::debug!("request {:?} pending AI daemon dispatch", req_id);
            }
        }
    }
}

fn log_protocol_error(err: ProtocolError) {
    eprintln!("NERVE protocol error: {}", err);
}

/// Read one complete NERVE frame from `stream`.
pub fn read_frame(stream: &mut UnixStream) -> Result<OwnedFrame, ProtocolError> {
    let mut header_buf = [0u8; HEADER_SIZE];
    stream
        .read_exact(&mut header_buf)
        .map_err(|_| ProtocolError::new(ProtocolErrorKind::MalformedFrame))?;

    let payload_len = u32::from_le_bytes(header_buf[16..20].try_into().unwrap()) as usize;

    let mut payload = vec![0u8; payload_len];
    stream
        .read_exact(&mut payload)
        .map_err(|_| ProtocolError::new(ProtocolErrorKind::MalformedFrame))?;

    let mut full_buf = Vec::with_capacity(HEADER_SIZE + payload_len);
    full_buf.extend_from_slice(&header_buf);
    full_buf.extend_from_slice(&payload);

    let frame = decode(&full_buf)?;

    Ok(OwnedFrame {
        header: frame.header,
        payload,
    })
}

/// Bind to `path` and accept connections, spawning a thread per connection.
///
/// Each accepted connection is fully isolated: a misbehaving or disconnecting
/// client cannot affect other in-flight connections.
pub fn run(path: &str) -> std::io::Result<()> {
    let _ = std::fs::remove_file(path);
    let listener = UnixListener::bind(path)?;

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                thread::spawn(move || {
                    if let Err(e) = handle_connection(stream) {
                        eprintln!("connection terminated: {}", e);
                    }
                });
            }
            Err(e) => {
                eprintln!("accept error: {}", e);
            }
        }
    }

    Ok(())
}
