use std::thread;
use tracing::info;

fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt::init();

    let config = nerve_core::config::Config::default();

    let token = nerve_core::auth::load_or_create_token(&config.token_path)?;
    info!("auth token loaded (path: {:?})", config.token_path);

    // Start the WebSocket server on a background thread.
    let ws_config = config.clone();
    let ws_token = token.clone();
    thread::spawn(move || {
        if let Err(e) = nerve_core::ws_server::run_ws(ws_config, ws_token) {
            eprintln!("WebSocket server error: {e}");
        }
    });

    info!("starting NERVE core (UDS: {})", config.uds_path);
    nerve_core::server::run(&config.uds_path)
}
