//! The accounts' tokens are kept sealed with ChaCha20-Poly1305, under a key derived from the proxy's
//! device key, so that a copy of the database alone gives nothing away.

use anyhow::Context;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use hkdf::Hkdf;
use sha2::Sha256;

const NONCE: usize = 12;
const PURPOSE: &[u8] = b"motile-proxy account tokens";

pub struct Seal(ChaCha20Poly1305);

impl Seal {
    pub fn new(device_key: &[u8; 32]) -> Self {
        let mut key = [0u8; 32];
        Hkdf::<Sha256>::new(None, device_key).expand(PURPOSE, &mut key).expect("32 bytes is a valid length");
        Self(ChaCha20Poly1305::new(Key::from_slice(&key)))
    }

    /// The nonce, then the ciphertext.
    pub fn seal(&self, plain: &[u8]) -> Vec<u8> {
        let mut nonce = [0u8; NONCE];
        getrandom::fill(&mut nonce).expect("the OS provides randomness");
        let sealed = self.0.encrypt(Nonce::from_slice(&nonce), plain).expect("encrypting in memory can't fail");
        [nonce.as_slice(), &sealed].concat()
    }

    pub fn open(&self, sealed: &[u8]) -> anyhow::Result<Vec<u8>> {
        anyhow::ensure!(sealed.len() > NONCE, "The sealed tokens are cut short.");
        let (nonce, sealed) = sealed.split_at(NONCE);
        self.0.decrypt(Nonce::from_slice(nonce), sealed).ok().context("The tokens were sealed with another key.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sealed_tokens_open_only_with_the_same_device_key() {
        let sealed = Seal::new(&[7; 32]).seal(b"refresh-token");
        assert_ne!(&sealed[NONCE..], b"refresh-token");
        assert_eq!(Seal::new(&[7; 32]).open(&sealed).unwrap(), b"refresh-token");
        assert!(Seal::new(&[8; 32]).open(&sealed).is_err());
    }
}
