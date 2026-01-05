use std::os::unix::net::{UnixListener, UnixStream};

use nerve_protocol::error::ProtocolError;

use crate::connection::read_frame;
use crate::dispatch::route_to_worker;
use crate::dispatch::{DispatchAction, dispatch_frame};
use crate::request_table::RequestTable;
use crate::worker_client::WorkerClient;

pub fn handle_connection(mut client_stream: UnixStream) -> Result<(), ProtocolError> {
    let mut requests = RequestTable::new();

    // Lazy-initialized worker (avoids race)
    let mut search_worker: Option<WorkerClient> = None;

    loop {
        let owned = match read_frame(&mut client_stream) {
            Ok(f) => f,
            Err(e) => {
                log_protocol_error(e);
                return Ok(()); // clean EOF / protocol exit
            }
        };

        let borrowed = owned.as_borrowed();

        let action = match dispatch_frame(&mut client_stream, borrowed, &mut requests) {
            Ok(a) => a,
            Err(e) => {
                log_protocol_error(e);
                return Ok(()); // stop handling this connection
            }
        };

        match action {
            DispatchAction::Handled => {
                // already processed inline
            }

            DispatchAction::RouteToSearchWorker(frame) => {
                if search_worker.is_none() {
                    search_worker = Some(WorkerClient::connect("/tmp/nerve-search-worker.sock")?);
                }

                route_to_worker(
                    &mut client_stream,
                    frame,
                    &mut requests,
                    search_worker.as_mut().unwrap(),
                )?;
            }

            DispatchAction::Cancelled(_) => {
                // cancellation already recorded
            }
        }
    }
}

fn log_protocol_error(err: ProtocolError) {
    eprintln!("NERVE protocol error: {}", err);
}

pub fn run(path: &str) -> std::io::Result<()> {
    let _ = std::fs::remove_file(path); // avoid bind failure in tests
    let listener = UnixListener::bind(path)?;

    for stream in listener.incoming() {
        let stream = stream?;
        if let Err(e) = handle_connection(stream) {
            eprintln!("connection terminated: {}", e);
        }
    }

    Ok(())
}
