//! Invitation and password-reset tokens.
//!
//! A token is 32 random bytes from the operating system's generator, written as 43 URL-safe
//! characters. It is shown once, when an admin creates it; only its SHA-256 hash is stored, and
//! it is looked up by that hash. SHA-256 is enough here (no slow hash like the one for
//! passwords) because the input has 256 bits of entropy: there is nothing to guess and nothing to
//! brute-force.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::{Digest, Sha256};

/// Length of a token as text: 32 bytes in unpadded base64.
pub const TOKEN_LEN: usize = 43;

/// A new random token.
#[must_use]
pub fn generate_token() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Whether `token` has the shape of a token we issue. Anything else cannot match a stored hash,
/// so it is rejected without a database lookup.
#[must_use]
pub fn is_well_formed(token: &str) -> bool {
    token.len() == TOKEN_LEN
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// The value stored and looked up for a token: lower-case hex of its SHA-256.
#[must_use]
pub fn hash_token(token: &str) -> String {
    use std::fmt::Write;
    Sha256::digest(token.as_bytes())
        .iter()
        .fold(String::with_capacity(64), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_long_random_and_url_safe() {
        let a = generate_token();
        let b = generate_token();
        assert_ne!(a, b);
        assert_eq!(a.len(), TOKEN_LEN);
        assert!(is_well_formed(&a));
        assert!(!a.contains(['+', '/', '=']));
    }

    #[test]
    fn the_hash_is_sha256_in_hex() {
        // SHA-256("abc"), the standard test vector
        assert_eq!(
            hash_token("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let hash = hash_token(&generate_token());
        assert_eq!(hash.len(), 64);
        assert!(hash.bytes().all(|b| b.is_ascii_hexdigit()));
    }

    #[test]
    fn only_the_issued_shape_is_well_formed() {
        assert!(is_well_formed(&"A".repeat(43)));
        assert!(!is_well_formed(""));
        assert!(!is_well_formed(&"A".repeat(42)));
        assert!(!is_well_formed(&"A".repeat(44)));
        assert!(!is_well_formed(&format!("{}+", "A".repeat(42))));
        assert!(!is_well_formed(&format!("{} ", "A".repeat(42))));
        assert!(!is_well_formed(&format!("{}é", "A".repeat(41))));
    }
}
