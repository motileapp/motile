//! A device is an ed25519 key. Its public half is the device's iroh address and its name at the
//! auth server; requests to the auth server are signed with it instead of carrying a token.

use std::fmt;
use std::io;
use std::path::Path;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

pub const AUTH_SCHEME: &str = "Motile";
const MAX_CLOCK_SKEW: f64 = 120.0;

pub struct DeviceKey(SigningKey);

impl DeviceKey {
    pub fn generate() -> Self {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).expect("the OS provides randomness");
        Self(SigningKey::from_bytes(&bytes))
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(SigningKey::from_bytes(&bytes))
    }

    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_bytes()
    }

    pub fn public(&self) -> String {
        hex::encode(self.0.verifying_key().to_bytes())
    }

    pub fn sign(&self, message: &[u8]) -> String {
        hex::encode(self.0.sign(message).to_bytes())
    }

    /// Reads the key file, creating it (owner-only) on first use.
    pub fn load_or_create(path: &Path) -> io::Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                let bytes = hex::decode(text.trim()).ok().and_then(|bytes| <[u8; 32]>::try_from(bytes).ok());
                bytes
                    .map(Self::from_bytes)
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "malformed device key"))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let key = Self::generate();
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                write_private(path, &hex::encode(key.to_bytes()))?;
                Ok(key)
            }
            Err(error) => Err(error),
        }
    }
}

fn write_private(path: &Path, contents: &str) -> io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(contents.as_bytes())
}

pub fn is_public_key(public_key: &str) -> bool {
    verifying_key(public_key).is_some()
}

fn verifying_key(public_key: &str) -> Option<VerifyingKey> {
    let bytes = <[u8; 32]>::try_from(hex::decode(public_key).ok()?).ok()?;
    VerifyingKey::from_bytes(&bytes).ok()
}

pub fn verify(public_key: &str, message: &[u8], signature: &str) -> bool {
    let Some(key) = verifying_key(public_key) else { return false };
    let Some(bytes) = hex::decode(signature).ok().and_then(|bytes| <[u8; 64]>::try_from(bytes).ok()) else {
        return false;
    };
    key.verify(message, &Signature::from_bytes(&bytes)).is_ok()
}

/// A random URL-safe secret: 32 bytes as hex.
pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the OS provides randomness");
    hex::encode(bytes)
}

const INSTALL_CODE_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
pub const INSTALL_CODE_LENGTH: usize = 8;

/// The short code in an install command: 8 characters of A-Z and 0-9, each drawn evenly.
pub fn random_install_code() -> String {
    let fair_limit = u8::MAX - u8::MAX % INSTALL_CODE_ALPHABET.len() as u8;
    let mut code = String::with_capacity(INSTALL_CODE_LENGTH);
    while code.len() < INSTALL_CODE_LENGTH {
        let mut bytes = [0u8; INSTALL_CODE_LENGTH];
        getrandom::fill(&mut bytes).expect("the OS provides randomness");
        for byte in bytes.into_iter().filter(|byte| *byte < fair_limit) {
            if code.len() == INSTALL_CODE_LENGTH {
                break;
            }
            code.push(INSTALL_CODE_ALPHABET[byte as usize % INSTALL_CODE_ALPHABET.len()] as char);
        }
    }
    code
}

pub fn sha256_hex(input: &[u8]) -> String {
    hex::encode(Sha256::digest(input))
}

fn request_message(method: &str, path: &str, body: &[u8], timestamp: u64) -> Vec<u8> {
    format!("{method}\n{path}\n{timestamp}\n{}", hex::encode(Sha256::digest(body))).into_bytes()
}

/// The `Authorization` header value for a request to the auth server.
pub fn sign_request(key: &DeviceKey, method: &str, path: &str, body: &[u8], now: f64) -> String {
    let timestamp = now as u64;
    let signature = key.sign(&request_message(method, path, body, timestamp));
    format!("{AUTH_SCHEME} {}.{timestamp}.{signature}", key.public())
}

#[derive(Debug, PartialEq, Eq)]
pub enum RequestAuthError {
    Malformed,
    Expired,
    BadSignature,
}

impl fmt::Display for RequestAuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Malformed => "malformed device authorization",
            Self::Expired => "device authorization expired; check the clock",
            Self::BadSignature => "device signature doesn't match",
        })
    }
}

/// Returns the public key that signed the request.
pub fn verify_request(
    header: &str,
    method: &str,
    path: &str,
    body: &[u8],
    now: f64,
) -> Result<String, RequestAuthError> {
    let value = header.strip_prefix(AUTH_SCHEME).map(str::trim).ok_or(RequestAuthError::Malformed)?;
    let mut parts = value.splitn(3, '.');
    let (Some(public_key), Some(timestamp), Some(signature)) = (parts.next(), parts.next(), parts.next()) else {
        return Err(RequestAuthError::Malformed);
    };
    let timestamp: u64 = timestamp.parse().map_err(|_| RequestAuthError::Malformed)?;
    if (now - timestamp as f64).abs() > MAX_CLOCK_SKEW {
        return Err(RequestAuthError::Expired);
    }
    if !verify(public_key, &request_message(method, path, body, timestamp), signature) {
        return Err(RequestAuthError::BadSignature);
    }
    Ok(public_key.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_install_code_is_eight_capitals_or_digits() {
        for _ in 0..100 {
            let code = random_install_code();
            assert_eq!(code.len(), INSTALL_CODE_LENGTH);
            assert!(code.bytes().all(|byte| INSTALL_CODE_ALPHABET.contains(&byte)), "{code}");
        }
        assert_ne!(random_install_code(), random_install_code());
    }

    #[test]
    fn signed_request_is_accepted_only_as_sent() {
        let key = DeviceKey::generate();
        let header = sign_request(&key, "GET", "/api/me", b"", 1000.0);

        assert_eq!(verify_request(&header, "GET", "/api/me", b"", 1010.0), Ok(key.public()));
        assert_eq!(verify_request(&header, "DELETE", "/api/me", b"", 1010.0), Err(RequestAuthError::BadSignature));
        assert_eq!(verify_request(&header, "GET", "/api/me", b"x", 1010.0), Err(RequestAuthError::BadSignature));
        assert_eq!(verify_request(&header, "GET", "/api/me", b"", 2000.0), Err(RequestAuthError::Expired));
    }

    #[test]
    fn signature_from_another_key_is_rejected() {
        let key = DeviceKey::generate();
        let other = DeviceKey::generate();
        let forged = sign_request(&other, "GET", "/api/me", b"", 1000.0).replace(&other.public(), &key.public());

        assert_eq!(verify_request(&forged, "GET", "/api/me", b"", 1000.0), Err(RequestAuthError::BadSignature));
    }

    #[test]
    fn key_file_round_trips() {
        let path = std::env::temp_dir().join(format!("motile-key-{}", DeviceKey::generate().public()));
        let created = DeviceKey::load_or_create(&path).unwrap();
        let loaded = DeviceKey::load_or_create(&path).unwrap();
        std::fs::remove_file(&path).unwrap();

        assert_eq!(created.public(), loaded.public());
    }
}
