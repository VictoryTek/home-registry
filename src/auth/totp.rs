//! TOTP (Time-based One-Time Password) module
//!
//! Provides TOTP secret generation, encryption/decryption at rest,
//! code verification, and key management following RFC 6238.

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use hkdf::Hkdf;
use rand::RngCore;
use sha2::Sha256;
use std::env;
use std::fmt::Write as _;
use std::sync::OnceLock;
use totp_rs::{Algorithm, Secret, TOTP};

/// TOTP configuration constants
const TOTP_DIGITS: usize = 6;
const TOTP_PERIOD: u64 = 30;
const TOTP_SKEW: u8 = 1; // ±1 time step
const TOTP_ISSUER: &str = "HomeRegistry";

/// AES-GCM nonce size (96 bits)
const NONCE_SIZE: usize = 12;

/// Global TOTP encryption key cache
static TOTP_KEY: OnceLock<[u8; 32]> = OnceLock::new();

/// TOTP-related errors
#[derive(Debug)]
pub enum TotpError {
    /// Encryption/decryption failure
    Crypto(String),
    /// TOTP generation or verification failure
    Totp(String),
    /// Missing configuration
    Config(String),
}

impl std::fmt::Display for TotpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TotpError::Crypto(msg) => write!(f, "Crypto error: {msg}"),
            TotpError::Totp(msg) => write!(f, "TOTP error: {msg}"),
            TotpError::Config(msg) => write!(f, "Config error: {msg}"),
        }
    }
}

impl std::error::Error for TotpError {}

// ==================== Key Management ====================

/// Initialize and get the TOTP encryption key (32 bytes for AES-256).
///
/// Tries multiple sources in order:
/// 1. Docker secret file (`/run/secrets/totp_encryption_key`)
/// 2. `TOTP_ENCRYPTION_KEY` environment variable
/// 3. Derive from the JWT secret (fallback)
pub fn get_or_init_totp_key() -> &'static [u8; 32] {
    TOTP_KEY.get_or_init(|| {
        // 1. Try Docker secret
        if let Ok(content) = std::fs::read_to_string("/run/secrets/totp_encryption_key") {
            let key_str = content.trim();
            if !key_str.is_empty() {
                log::info!("Using TOTP encryption key from Docker secrets");
                return derive_key(key_str.as_bytes());
            }
        }

        // 2. Try environment variable
        if let Ok(key_str) = env::var("TOTP_ENCRYPTION_KEY") {
            if !key_str.is_empty() {
                log::info!("Using TOTP encryption key from environment variable");
                return derive_key(key_str.as_bytes());
            }
        }

        // 3. Derive from JWT secret as fallback
        let jwt_secret = super::get_or_init_jwt_secret();
        log::warn!(
            "No TOTP_ENCRYPTION_KEY found. Deriving from JWT_SECRET. \
             Set TOTP_ENCRYPTION_KEY for independent key management."
        );
        derive_key(jwt_secret.as_bytes())
    })
}

/// A short, non-secret fingerprint of the TOTP encryption key, recorded in backups.
///
/// Lets a restore detect that the stored (encrypted) 2FA secrets were made with a different
/// key and could not be decrypted. It is an HKDF-SHA256 expansion of the key under its own
/// label (HMAC-SHA256 based, domain-separated from the encryption use), truncated to 8 bytes,
/// so it does not reveal the key.
#[must_use]
pub fn totp_key_fingerprint() -> String {
    let hk = Hkdf::<Sha256>::from_prk(get_or_init_totp_key()).expect("32-byte PRK is valid");
    let mut out = [0u8; 8];
    hk.expand(b"home-registry-backup-key-fingerprint-v1", &mut out)
        .expect("HKDF expand should never fail for 8-byte output");
    let mut hex = String::with_capacity(16);
    for byte in out {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// Derive a 256-bit key from arbitrary input material using HKDF-SHA256
fn derive_key(input_key_material: &[u8]) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(Some(b"home-registry-totp-v1"), input_key_material);
    let mut okm = [0u8; 32];
    hk.expand(b"totp-secret-encryption", &mut okm)
        .expect("HKDF expand should never fail for 32-byte output");
    okm
}

// ==================== Encryption / Decryption ====================

/// Encrypt a TOTP secret (base32 string) for storage in the database.
///
/// Uses AES-256-GCM with a random 12-byte nonce.
/// Returns: `base64(nonce || ciphertext || tag)`
pub fn encrypt_totp_secret(secret: &str) -> Result<String, TotpError> {
    use base64::Engine;

    let key = get_or_init_totp_key();
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|e| TotpError::Crypto(format!("Key init: {e}")))?;

    let mut nonce_bytes = [0u8; NONCE_SIZE];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, secret.as_bytes())
        .map_err(|e| TotpError::Crypto(format!("Encryption failed: {e}")))?;

    // Prepend nonce to ciphertext
    let mut combined = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);

    Ok(base64::engine::general_purpose::STANDARD.encode(combined))
}

/// Decrypt a TOTP secret from database storage.
///
/// Expects: `base64(nonce || ciphertext || tag)`
/// Returns: The original base32-encoded TOTP secret
pub fn decrypt_totp_secret(encrypted: &str) -> Result<String, TotpError> {
    use base64::Engine;

    let key = get_or_init_totp_key();
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|e| TotpError::Crypto(format!("Key init: {e}")))?;

    let combined = base64::engine::general_purpose::STANDARD
        .decode(encrypted)
        .map_err(|e| TotpError::Crypto(format!("Base64 decode: {e}")))?;

    if combined.len() < NONCE_SIZE {
        return Err(TotpError::Crypto("Encrypted data too short".to_string()));
    }

    let (nonce_bytes, ciphertext) = combined.split_at(NONCE_SIZE);
    let nonce = Nonce::from_slice(nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| TotpError::Crypto(format!("Decryption failed: {e}")))?;

    String::from_utf8(plaintext).map_err(|e| TotpError::Crypto(format!("UTF-8 decode: {e}")))
}

// ==================== TOTP Operations ====================

/// Result of generating a new TOTP setup for a user.
pub struct TotpSetupData {
    /// Base32-encoded secret (to display to user for manual entry)
    pub secret_base32: String,
    /// The `otpauth://totp/...` URI for QR code generation
    pub otpauth_uri: String,
    /// QR code as a `data:image/png;base64,...` data URI
    pub qr_code_data_uri: String,
    /// The encrypted secret for database storage
    pub encrypted_secret: String,
}

/// Generate a new TOTP secret and setup data for a user.
pub fn generate_totp_setup(username: &str) -> Result<TotpSetupData, TotpError> {
    let secret = Secret::generate_secret();
    let secret_bytes = secret
        .to_bytes()
        .map_err(|e| TotpError::Totp(format!("Secret generation: {e}")))?;

    let totp = TOTP::new(
        Algorithm::SHA1,
        TOTP_DIGITS,
        TOTP_SKEW,
        TOTP_PERIOD,
        secret_bytes,
        Some(TOTP_ISSUER.to_string()),
        username.to_string(),
    )
    .map_err(|e| TotpError::Totp(format!("TOTP init: {e}")))?;

    let otpauth_uri = totp.get_url();
    let secret_base32 = totp.get_secret_base32();

    let qr_code_data_uri = totp
        .get_qr_base64()
        .map(|b64| format!("data:image/png;base64,{b64}"))
        .map_err(|e| TotpError::Totp(format!("QR generation: {e}")))?;

    let encrypted_secret = encrypt_totp_secret(&secret_base32)?;

    Ok(TotpSetupData {
        secret_base32,
        otpauth_uri,
        qr_code_data_uri,
        encrypted_secret,
    })
}

/// Verify a TOTP code against an encrypted secret.
///
/// Allows ±1 time step skew (90-second window total).
/// Returns `true` if the code is valid.
pub fn verify_totp_code(encrypted_secret: &str, code: &str) -> Result<bool, TotpError> {
    let secret_base32 = decrypt_totp_secret(encrypted_secret)?;

    let secret = Secret::Encoded(secret_base32);
    let secret_bytes = secret
        .to_bytes()
        .map_err(|e| TotpError::Totp(format!("Secret decode: {e}")))?;

    let totp = TOTP::new(
        Algorithm::SHA1,
        TOTP_DIGITS,
        TOTP_SKEW,
        TOTP_PERIOD,
        secret_bytes,
        Some(TOTP_ISSUER.to_string()),
        String::new(), // account name not needed for verification
    )
    .map_err(|e| TotpError::Totp(format!("TOTP init: {e}")))?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| TotpError::Totp(format!("System time error: {e}")))?
        .as_secs();

    Ok(totp.check(code, now))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    #[test]
    fn test_key_derivation_deterministic() {
        let key1 = derive_key(b"test-key-material");
        let key2 = derive_key(b"test-key-material");
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_key_derivation_different_inputs() {
        let key1 = derive_key(b"key-a");
        let key2 = derive_key(b"key-b");
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        // Initialize key for test
        let _ = get_or_init_totp_key();

        let secret = "JBSWY3DPEHPK3PXP";
        let encrypted = encrypt_totp_secret(secret).expect("Encryption should succeed");
        let decrypted = decrypt_totp_secret(&encrypted).expect("Decryption should succeed");
        assert_eq!(secret, decrypted);
    }

    #[test]
    fn test_encrypt_produces_different_ciphertexts() {
        let _ = get_or_init_totp_key();

        let secret = "JBSWY3DPEHPK3PXP";
        let enc1 = encrypt_totp_secret(secret).expect("Encryption should succeed");
        let enc2 = encrypt_totp_secret(secret).expect("Encryption should succeed");
        // Different nonces should produce different ciphertexts
        assert_ne!(enc1, enc2);
    }

    #[test]
    fn test_decrypt_invalid_data() {
        let _ = get_or_init_totp_key();

        let result = decrypt_totp_secret("not-valid-base64!!!");
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_too_short() {
        let _ = get_or_init_totp_key();

        let short = base64::engine::general_purpose::STANDARD.encode([0u8; 5]);
        let result = decrypt_totp_secret(&short);
        assert!(result.is_err());
    }

    #[test]
    fn test_generate_totp_setup() {
        let _ = get_or_init_totp_key();

        let setup = generate_totp_setup("testuser").expect("Setup should succeed");
        assert!(!setup.secret_base32.is_empty());
        assert!(setup.otpauth_uri.starts_with("otpauth://totp/"));
        assert!(setup.otpauth_uri.contains("HomeRegistry"));
        assert!(setup.qr_code_data_uri.starts_with("data:image/png;base64,"));
        assert!(!setup.encrypted_secret.is_empty());
    }

    #[test]
    fn test_verify_totp_code_with_generated_secret() {
        let _ = get_or_init_totp_key();

        let setup = generate_totp_setup("testuser").expect("Setup should succeed");

        // Generate a valid code from the secret
        let secret = Secret::Encoded(setup.secret_base32.clone());
        let secret_bytes = secret.to_bytes().expect("Secret decode");
        let totp = TOTP::new(
            Algorithm::SHA1,
            TOTP_DIGITS,
            TOTP_SKEW,
            TOTP_PERIOD,
            secret_bytes,
            Some(TOTP_ISSUER.to_string()),
            "testuser".to_string(),
        )
        .expect("TOTP init");

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_secs();
        let valid_code = totp.generate(now);

        let result = verify_totp_code(&setup.encrypted_secret, &valid_code);
        assert!(result.expect("Verification should succeed"));
    }

    #[test]
    fn test_verify_totp_code_wrong_code() {
        let _ = get_or_init_totp_key();

        let setup = generate_totp_setup("testuser").expect("Setup should succeed");
        let result = verify_totp_code(&setup.encrypted_secret, "000000");
        // Likely false (extremely unlikely to match)
        assert!(result.is_ok());
    }
}
