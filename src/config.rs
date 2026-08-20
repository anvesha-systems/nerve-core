use std::net::Ipv4Addr;
use std::path::PathBuf;

/// Runtime configuration for nerve-core.
///
/// The bind address is always `127.0.0.1` — exposing the daemon on `0.0.0.0`
/// would make it reachable from the LAN, which is a security violation for a
/// local-first system.
#[derive(Clone)]
pub struct Config {
    /// WebSocket bind address. MUST be 127.0.0.1. Never 0.0.0.0.
    pub bind_addr: Ipv4Addr,
    /// WebSocket port.
    pub ws_port: u16,
    /// Chrome extension ID allowed to connect.
    /// Derives the expected `Origin: chrome-extension://<id>` header.
    /// Empty string disables origin checking (test mode only).
    pub allowed_extension_id: String,
    /// Path where the per-install auth token is stored.
    pub token_path: PathBuf,
    /// Unix Domain Socket path for the UDS server.
    pub uds_path: String,
}

impl Config {
    /// Return the expected `Origin` header value for the allowed extension,
    /// or `None` when origin checking is disabled (empty extension ID).
    pub fn allowed_origin(&self) -> Option<String> {
        if self.allowed_extension_id.is_empty() {
            None
        } else {
            Some(format!("chrome-extension://{}", self.allowed_extension_id))
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        Self {
            bind_addr: Ipv4Addr::LOCALHOST,
            ws_port: 9001,
            allowed_extension_id: String::new(),
            token_path: PathBuf::from(home).join(".anvesha").join("token"),
            uds_path: "/tmp/nerve.sock".to_string(),
        }
    }
}
