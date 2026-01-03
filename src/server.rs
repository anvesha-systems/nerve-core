use std::os::unix::net::UnixListener;
use std::path::Path;
use std::thread;

use crate::connection::handle_connection;

pub fn run_server(socket_path: &str) -> std::io::Result<()> {
    // Clean up old socket if exists
    if Path::new(socket_path).exists() {
        std::fs::remove_file(socket_path)?;
    }

    let listener = UnixListener::bind(socket_path)?;

    println!("NERVE core listening on {}", socket_path);

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                thread::spawn(|| {
                    handle_connection(stream);
                });
            }
            Err(e) => {
                eprintln!("Accept error: {}", e);
            }
        }
    }

    Ok(())
}