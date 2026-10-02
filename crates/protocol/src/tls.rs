//! The HTTP client is built without a TLS provider so that it shares ring with iroh, which keeps
//! the builds for Linux servers and for the apps free of a second crypto library.

/// Makes ring the process's TLS provider. Safe to call more than once.
pub fn install() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}
