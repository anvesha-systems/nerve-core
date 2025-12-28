use std::os::unix::net::UnixListener;
use std::path::{Path};

use tracing::info;

use crate::connection;

pub fn run(socket_path: &str)-> std::io::Result<()>{
    if Path::new(socket_path).exists(){
        std::fs::remove_file(socket_path)?;
    }

    let listener = UnixListener::bind(socket_path)?;
    info!("NERVE core listening on {}", socket_path);

    // v0.1 single client
    let (stream, _) = listener.accept()?;
    info!("client connected");

    connection::run(stream)
}