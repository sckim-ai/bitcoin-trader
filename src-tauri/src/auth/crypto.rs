use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, AeadCore,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

/// Encrypt plaintext using AES-256-GCM. Returns base64-encoded "nonce:ciphertext".
/// master_key must be exactly 32 bytes.
pub fn encrypt(plaintext: &str, master_key: &[u8]) -> Result<String, String> {
    if master_key.len() != 32 {
        return Err("Master key must be 32 bytes".to_string());
    }
    let cipher =
        Aes256Gcm::new_from_slice(master_key).map_err(|e| format!("Cipher init error: {e}"))?;
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| format!("Encryption error: {e}"))?;

    let mut combined = nonce.to_vec();
    combined.extend_from_slice(&ciphertext);
    Ok(BASE64.encode(combined))
}

/// Decrypt a base64-encoded "nonce+ciphertext" string using AES-256-GCM.
pub fn decrypt(encoded: &str, master_key: &[u8]) -> Result<String, String> {
    if master_key.len() != 32 {
        return Err("Master key must be 32 bytes".to_string());
    }
    let combined = BASE64
        .decode(encoded)
        .map_err(|e| format!("Base64 decode error: {e}"))?;

    if combined.len() < 12 {
        return Err("Ciphertext too short".to_string());
    }

    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce = aes_gcm::Nonce::from_slice(nonce_bytes);
    let cipher =
        Aes256Gcm::new_from_slice(master_key).map_err(|e| format!("Cipher init error: {e}"))?;
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("Decryption error: {e}"))?;

    String::from_utf8(plaintext).map_err(|e| format!("UTF-8 error: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = b"01234567890123456789012345678901"; // 32 bytes
        let plaintext = "my-secret-api-key-12345";

        let encrypted = encrypt(plaintext, key).unwrap();
        assert_ne!(encrypted, plaintext);

        let decrypted = decrypt(&encrypted, key).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_different_encryptions() {
        let key = b"01234567890123456789012345678901";
        let e1 = encrypt("same", key).unwrap();
        let e2 = encrypt("same", key).unwrap();
        assert_ne!(e1, e2); // different nonces
        assert_eq!(decrypt(&e1, key).unwrap(), "same");
        assert_eq!(decrypt(&e2, key).unwrap(), "same");
    }

    #[test]
    fn test_wrong_key_fails() {
        let key1 = b"01234567890123456789012345678901";
        let key2 = b"abcdefghijklmnopqrstuvwxyz012345";
        let encrypted = encrypt("secret", key1).unwrap();
        assert!(decrypt(&encrypted, key2).is_err());
    }

    #[test]
    fn test_invalid_key_length() {
        assert!(encrypt("x", b"short").is_err());
        assert!(decrypt("x", b"short").is_err());
    }
}
