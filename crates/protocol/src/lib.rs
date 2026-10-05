//! What the clients, the server and the auth server agree on: the messages on an iroh stream,
//! the auth server's JSON, and how a device proves who it is.

pub mod auth_api;
#[cfg(feature = "auth-client")]
pub mod auth_client;
pub mod frame;
pub mod identity;
pub mod media;
#[cfg(feature = "auth-client")]
pub mod tls;
pub mod wire;

pub const ALPN: &[u8] = b"motile/1";
pub const PROTOCOL_VERSION: u32 = 10;
pub const DEFAULT_AUTH_URL: &str = "https://auth.motile.app";
/// Where the auth server sends the browser once a sign-in is done; the client owns this scheme.
pub const APP_REDIRECT: &str = "motile://auth";

pub fn now() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

/// An error and its causes as sentences a client can show, the first one saying what went wrong.
pub fn error_text(error: &anyhow::Error) -> String {
    error.chain().map(ToString::to_string).collect::<Vec<_>>().join(" ")
}
