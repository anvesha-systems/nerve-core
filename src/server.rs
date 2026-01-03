use std::os::unix::net::UnixListener;
use std::path::Path;
use std::thread;
use std::io;

use crate::connection::handle_connection;

pub fn run(socket_path: &str) -> io::Result<()> {
    // Clean up old socket if exists
    if Path::new(socket_path).exists() {
        std::fs::remove_file(socket_path)?;
    }

    let listener = UnixListener::bind(socket_path)?;

    // optional but recomended : restrict permission
    // only current user can connect
    #[cfg(unix)]
    {
        use std::fs::Permissions;
        use std::os::unix::fs::PermissionsExt;

        std::fs::set_permissions(socket_path, Permissions::from_mode(0o600))?;
    }
    
    println!("NERVE core listening on {}", socket_path);
    // accept loop
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                thread::spawn(|| {
                    handle_connection(stream);
                });
            }
            Err(e) => {
                eprintln!("Accept error: {}", e);
                // continue accepting connections
            }
        }
    }

    Ok(())
}