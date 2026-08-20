use std::fs;
use std::io;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// Number of random bytes in the token (256-bit entropy).
const TOKEN_BYTES: usize = 32;

/// Generate a fresh cryptographically random token as a 64-character hex string.
pub fn generate_token() -> String {
    let mut bytes = [0u8; TOKEN_BYTES];
    getrandom::getrandom(&mut bytes).expect("OS RNG unavailable");
    hex_encode(&bytes)
}

/// Load the token from `path`, or generate and persist a new one.
///
/// The file is created with permissions `0600` so only the owning user can read
/// it. The token is never logged.
pub fn load_or_create_token(path: &Path) -> io::Result<String> {
    if path.exists() {
        let raw = fs::read_to_string(path)?;
        let token = raw.trim().to_string();
        // Validate it looks like a proper hex token before trusting it.
        if token.len() == TOKEN_BYTES * 2 && token.chars().all(|c| c.is_ascii_hexdigit()) {
            return Ok(token);
        }
        // Corrupted or truncated — regenerate below.
        tracing::warn!("token file is invalid; regenerating");
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let token = generate_token();
    fs::write(path, &token)?;

    // Restrict permissions: owner read+write only.
    #[cfg(unix)]
    {
        let mut perms = fs::metadata(path)?.permissions();
        perms.set_mode(0o600);
        fs::set_permissions(path, perms)?;
    }

    Ok(token)
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_token_is_64_hex_chars() {
        let t = generate_token();
        assert_eq!(t.len(), 64);
        assert!(t.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn tokens_are_unique() {
        let a = generate_token();
        let b = generate_token();
        assert_ne!(a, b);
    }
}
