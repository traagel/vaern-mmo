//! Boot-time network config: netcode key and socket addresses.
//!
//! Both server and client read these on startup. Source of truth:
//!
//! - `VAERN_NETCODE_KEY` (env, hex-encoded 32 bytes) — required in release;
//!   debug builds fall back to an all-zero dev key with a `warn!`.
//! - `--bind <addr>` / `VAERN_BIND` — server listen socket. Default
//!   `0.0.0.0:27015` (host-friendly, NOT loopback).
//! - `--server <addr>` / `VAERN_SERVER` — client connect socket. Default
//!   `127.0.0.1:27015` (dev loop-friendly).
//!
//! All three resolvers return a `Result<_, String>` instead of carrying a
//! crate-level error type — this is boot-time config, callers panic with
//! the message and the process exits before any Bevy plugin loads.

use core::net::{IpAddr, Ipv4Addr, SocketAddr};

/// Default loopback address used by the client when neither `--server` nor
/// `VAERN_SERVER` is set, and by tests / scripts that bypass the resolvers.
pub const DEFAULT_DEV_SERVER_ADDR: SocketAddr =
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 27015);

/// Default unspecified-host bind used by the server when neither `--bind`
/// nor `VAERN_BIND` is set. Binding `0.0.0.0` so a Hetzner/OVH box accepts
/// connections from outside its loopback by default.
pub const DEFAULT_SERVER_BIND_ADDR: SocketAddr =
    SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 27015);

/// Dev fallback netcode key. Only used when `VAERN_NETCODE_KEY` is unset
/// AND the binary was built with `debug_assertions` on. Release builds
/// reject an unset env var; both ends reject an all-zero env value.
pub const DEV_NETCODE_KEY: [u8; 32] = [0; 32];

const NETCODE_KEY_ENV: &str = "VAERN_NETCODE_KEY";
const SERVER_BIND_ENV: &str = "VAERN_BIND";
const SERVER_CONNECT_ENV: &str = "VAERN_SERVER";

/// Where the resolved netcode key came from, for boot logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetcodeKeySource {
    Env,
    DevFallback,
}

impl NetcodeKeySource {
    pub fn label(self) -> &'static str {
        match self {
            NetcodeKeySource::Env => "env",
            NetcodeKeySource::DevFallback => "dev fallback (debug build)",
        }
    }
}

/// Parse a 64-char hex string into a 32-byte netcode key. Whitespace
/// around the value is trimmed; case is ignored.
pub fn netcode_key_from_hex(hex: &str) -> Result<[u8; 32], String> {
    let trimmed = hex.trim();
    if trimmed.len() != 64 {
        return Err(format!(
            "VAERN_NETCODE_KEY must be 64 hex chars (32 bytes), got {}",
            trimmed.len()
        ));
    }
    let mut out = [0u8; 32];
    let bytes = trimmed.as_bytes();
    for i in 0..32 {
        let hi = hex_nibble(bytes[i * 2]).ok_or_else(|| {
            format!(
                "VAERN_NETCODE_KEY contains non-hex character at byte {}",
                i * 2
            )
        })?;
        let lo = hex_nibble(bytes[i * 2 + 1]).ok_or_else(|| {
            format!(
                "VAERN_NETCODE_KEY contains non-hex character at byte {}",
                i * 2 + 1
            )
        })?;
        out[i] = (hi << 4) | lo;
    }
    Ok(out)
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Resolve the netcode key. Reads `VAERN_NETCODE_KEY` (hex). Release
/// builds reject missing or all-zero values. Debug builds fall back to
/// `DEV_NETCODE_KEY` with the `DevFallback` source so the caller can warn.
pub fn resolve_netcode_key() -> Result<([u8; 32], NetcodeKeySource), String> {
    match std::env::var(NETCODE_KEY_ENV) {
        Ok(hex) => {
            let key = netcode_key_from_hex(&hex)?;
            if key.iter().all(|b| *b == 0) {
                return Err(format!(
                    "{NETCODE_KEY_ENV} must not be all-zero (use `head -c 32 /dev/urandom | xxd -p -c 64`)"
                ));
            }
            Ok((key, NetcodeKeySource::Env))
        }
        Err(_) => {
            #[cfg(debug_assertions)]
            {
                Ok((DEV_NETCODE_KEY, NetcodeKeySource::DevFallback))
            }
            #[cfg(not(debug_assertions))]
            {
                Err(format!(
                    "{NETCODE_KEY_ENV} required in release builds (set to 64 hex chars)"
                ))
            }
        }
    }
}

/// Parse a SocketAddr with a friendly error message.
pub fn parse_socket_addr(s: &str) -> Result<SocketAddr, String> {
    s.parse::<SocketAddr>()
        .map_err(|e| format!("invalid socket address '{s}': {e}"))
}

/// Resolve the server's listen address. Order:
/// 1. `--bind <addr>` CLI arg
/// 2. `VAERN_BIND` env var
/// 3. `0.0.0.0:27015`
pub fn resolve_server_bind() -> Result<SocketAddr, String> {
    if let Some(addr) = parse_named_arg(std::env::args(), "--bind") {
        return parse_socket_addr(&addr);
    }
    if let Ok(s) = std::env::var(SERVER_BIND_ENV) {
        return parse_socket_addr(&s);
    }
    Ok(DEFAULT_SERVER_BIND_ADDR)
}

/// Resolve the address the client should connect to. Order:
/// 1. `--server <addr>` CLI arg
/// 2. `VAERN_SERVER` env var
/// 3. `127.0.0.1:27015`
pub fn resolve_server_connect() -> Result<SocketAddr, String> {
    if let Some(addr) = parse_named_arg(std::env::args(), "--server") {
        return parse_socket_addr(&addr);
    }
    if let Ok(s) = std::env::var(SERVER_CONNECT_ENV) {
        return parse_socket_addr(&s);
    }
    Ok(DEFAULT_DEV_SERVER_ADDR)
}

/// Scan an iterator of CLI args for `--name <value>` or `--name=<value>`.
/// Returns the first match. Public for tests.
pub fn parse_named_arg<I: IntoIterator<Item = String>>(args: I, name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        if arg == name {
            return iter.next();
        }
        if let Some(rest) = arg.strip_prefix(&prefix) {
            return Some(rest.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn netcode_key_from_hex_round_trips() {
        let raw: [u8; 32] = [
            0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54,
            0x32, 0x10, 0xde, 0xad, 0xbe, 0xef, 0xca, 0xfe, 0xba, 0xbe, 0x00, 0xff, 0x10, 0x20,
            0x30, 0x40, 0x50, 0x60,
        ];
        let hex: String = raw.iter().map(|b| format!("{b:02x}")).collect();
        let parsed = netcode_key_from_hex(&hex).unwrap();
        assert_eq!(parsed, raw);
    }

    #[test]
    fn netcode_key_uppercase_ok() {
        let hex = "AABBCCDDEEFF00112233445566778899AABBCCDDEEFF00112233445566778899";
        let parsed = netcode_key_from_hex(hex).unwrap();
        assert_eq!(parsed[0], 0xAA);
        assert_eq!(parsed[31], 0x99);
    }

    #[test]
    fn netcode_key_trims_whitespace() {
        let hex = "  AABBCCDDEEFF00112233445566778899AABBCCDDEEFF00112233445566778899\n";
        assert!(netcode_key_from_hex(hex).is_ok());
    }

    #[test]
    fn netcode_key_rejects_wrong_length() {
        assert!(netcode_key_from_hex("abcd").is_err());
        assert!(netcode_key_from_hex(&"a".repeat(63)).is_err());
        assert!(netcode_key_from_hex(&"a".repeat(65)).is_err());
    }

    #[test]
    fn netcode_key_rejects_non_hex() {
        let bad = "ZZBBCCDDEEFF00112233445566778899AABBCCDDEEFF00112233445566778899";
        assert!(netcode_key_from_hex(bad).is_err());
    }

    #[test]
    fn parse_socket_addr_ok() {
        assert_eq!(
            parse_socket_addr("0.0.0.0:27015").unwrap(),
            "0.0.0.0:27015".parse().unwrap()
        );
        assert_eq!(
            parse_socket_addr("127.0.0.1:1234").unwrap(),
            "127.0.0.1:1234".parse().unwrap()
        );
    }

    #[test]
    fn parse_socket_addr_rejects_garbage() {
        assert!(parse_socket_addr("not-an-addr").is_err());
        assert!(parse_socket_addr("127.0.0.1").is_err()); // missing port
    }

    #[test]
    fn parse_named_arg_finds_space_form() {
        let args = vec![
            "vaern-server".into(),
            "--bind".into(),
            "0.0.0.0:27015".into(),
        ];
        assert_eq!(
            parse_named_arg(args, "--bind"),
            Some("0.0.0.0:27015".into())
        );
    }

    #[test]
    fn parse_named_arg_finds_equals_form() {
        let args = vec!["vaern-server".into(), "--bind=0.0.0.0:27015".into()];
        assert_eq!(
            parse_named_arg(args, "--bind"),
            Some("0.0.0.0:27015".into())
        );
    }

    #[test]
    fn parse_named_arg_returns_none_when_absent() {
        let args = vec!["vaern-server".into(), "--other".into(), "x".into()];
        assert_eq!(parse_named_arg(args, "--bind"), None);
    }

    #[test]
    fn parse_named_arg_returns_first_match() {
        let args = vec![
            "vaern-server".into(),
            "--bind".into(),
            "first:1".into(),
            "--bind".into(),
            "second:2".into(),
        ];
        assert_eq!(parse_named_arg(args, "--bind"), Some("first:1".into()));
    }
}
