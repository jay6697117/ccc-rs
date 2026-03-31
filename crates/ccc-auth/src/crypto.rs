//! PKCE helpers.  Corresponds to TS `src/services/oauth/crypto.ts`.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::RngCore;
use sha2::{Digest, Sha256};

/// Generate a random 32-byte PKCE code verifier (base64url, no padding).
pub fn generate_code_verifier() -> String {
    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

/// Derive the S256 code challenge from a verifier.
pub fn generate_code_challenge(verifier: &str) -> String {
    let hash = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hash)
}

/// Generate a random CSRF state token.
pub fn generate_state() -> String {
    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifier_is_base64url() {
        let v = generate_code_verifier();
        assert!(!v.contains('+'));
        assert!(!v.contains('/'));
        assert!(!v.contains('='));
        assert!(v.len() > 20);
    }

    #[test]
    fn challenge_differs_from_verifier() {
        let v = generate_code_verifier();
        let c = generate_code_challenge(&v);
        assert_ne!(v, c);
        // Stable: same verifier → same challenge
        assert_eq!(c, generate_code_challenge(&v));
    }

    #[test]
    fn state_is_base64url() {
        let s = generate_state();
        assert!(!s.contains('+'));
        assert!(!s.contains('/'));
        assert!(!s.contains('='));
    }
}
