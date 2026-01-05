use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

use nerve_protocol::codec::decode;
use nerve_protocol::constants::HEADER_SIZE;
use nerve_protocol::error::ProtocolError;
use nerve_protocol::frame::OwnedFrame;
use nerve_protocol::types::ProtocolErrorKind;

pub struct WorkerClient {
    stream: UnixStream,
}

impl WorkerClient {
    pub fn connect(path: &str) -> Result<Self, ProtocolError> {
        let stream = UnixStream::connect(path).map_err(|e| {
            eprintln!("Failed to connect to worker socket: {}", e);
            ProtocolError::new(ProtocolErrorKind::InternalError)
        })?;
        Ok(Self { stream })
    }

    pub fn send_raw(&mut self, buf: &[u8]) -> Result<(), ProtocolError> {
        self.stream.write_all(buf).map_err(|e| {
            eprintln!("Failed to write to worker socket: {}", e);
            ProtocolError::new(ProtocolErrorKind::InternalError)
        })
    }

    pub fn read_frame(&mut self) -> Result<OwnedFrame, ProtocolError> {
        let mut header = [0u8; HEADER_SIZE];
        self.stream.read_exact(&mut header).map_err(|e| {
            eprintln!("Failed to read frame header from worker socket: {}", e);
            ProtocolError::new(ProtocolErrorKind::InternalError)
        })?;

        let payload_len = u32::from_le_bytes(header[16..20].try_into().unwrap()) as usize;

        let mut payload = vec![0u8; payload_len];
        self.stream.read_exact(&mut payload).map_err(|e| {
            eprintln!("Failed to read frame payload from worker socket: {}", e);
            ProtocolError::new(ProtocolErrorKind::InternalError)
        })?;

        let mut full = Vec::with_capacity(HEADER_SIZE + payload_len);
        full.extend_from_slice(&header);
        full.extend_from_slice(&payload);

        let frame = decode(&full).map_err(|e| {
            eprintln!("Failed to decode frame: {}", e);
            ProtocolError::new(ProtocolErrorKind::InternalError)
        })?;

        Ok(OwnedFrame {
            header: frame.header,
            payload,
        })
    }
}
