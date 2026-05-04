use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    AesGcm,
};
use aes_gcm::aes::Aes256;
use rand::Rng;
use std::env;
use std::sync::OnceLock;

// Node.js crypto uses a 16-byte IV for aes-256-gcm by default.
// Rust's default Aes256Gcm expects a 12-byte IV.
// We must declare a custom AesGcm type that expects a 16-byte nonce.
type Aes256Gcm16 = AesGcm<Aes256, typenum::U16>;

static CACHED_KEY: OnceLock<[u8; 32]> = OnceLock::new();

pub fn init_encryption_key() {
    let env_key = env::var("ENCRYPTION_KEY").unwrap_or_default();
    if env_key.is_empty() || env_key == "your-64-char-hex-key-here" {
        panic!("ENCRYPTION_KEY must be set to a 64-char hex string in .env");
    }

    if env_key.len() != 64 {
        panic!("ENCRYPTION_KEY must be exactly 64 characters long.");
    }

    let mut key_bytes = [0u8; 32];
    hex::decode_to_slice(env_key, &mut key_bytes).expect("ENCRYPTION_KEY must be valid hex");

    // Only set if not already set (e.g. from tests)
    let _ = CACHED_KEY.set(key_bytes);
}

fn get_encryption_key() -> &'static [u8; 32] {
    CACHED_KEY.get().expect("Encryption key not initialized. Call init_encryption_key() first.")
}

pub struct EncryptedData {
    pub encrypted: String,
    pub iv: String,
    pub auth_tag: String,
}

pub fn encrypt(text: &str) -> EncryptedData {
    let key = get_encryption_key();
    let cipher = Aes256Gcm16::new(key.into());

    let mut iv_bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut iv_bytes);
    
    // In node.js, the final ciphertext and auth tag are separate.
    // aes_gcm appends the 16-byte auth tag to the end of the ciphertext.
    let encrypted = cipher.encrypt(&iv_bytes.into(), text.as_bytes())
        .expect("encryption failure");
    
    // Split the result into ciphertext and tag
    let tag_start = encrypted.len() - 16;
    let ciphertext = &encrypted[..tag_start];
    let tag = &encrypted[tag_start..];

    EncryptedData {
        encrypted: hex::encode(ciphertext),
        iv: hex::encode(iv_bytes),
        auth_tag: hex::encode(tag),
    }
}

pub fn decrypt(encrypted_hex: &str, iv_hex: &str, auth_tag_hex: &str) -> String {
    let key = get_encryption_key();
    let cipher = Aes256Gcm16::new(key.into());

    let ciphertext = hex::decode(encrypted_hex).expect("invalid hex in ciphertext");
    let iv_bytes = hex::decode(iv_hex).expect("invalid hex in iv");
    let tag_bytes = hex::decode(auth_tag_hex).expect("invalid hex in auth_tag");

    // Reconstruct the combined payload (ciphertext + tag) for the aes_gcm crate
    let mut payload = Vec::with_capacity(ciphertext.len() + tag_bytes.len());
    payload.extend_from_slice(&ciphertext);
    payload.extend_from_slice(&tag_bytes);

    let decrypted = cipher.decrypt(iv_bytes.as_slice().into(), payload.as_ref())
        .expect("decryption failure");
    
    String::from_utf8(decrypted).expect("invalid utf8 in decrypted string")
}

pub fn mask_key(key: &str) -> String {
    if key.len() <= 8 {
        let suffix = if key.len() > 4 { &key[key.len() - 4..] } else { key };
        format!("****{}", suffix)
    } else {
        format!("{}...{}", &key[0..4], &key[key.len() - 4..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_roundtrip() {
        unsafe { env::set_var("ENCRYPTION_KEY", "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"); }
        init_encryption_key();

        let original = "my-secret-api-key-123";
        let encrypted_data = encrypt(original);
        
        let decrypted = decrypt(&encrypted_data.encrypted, &encrypted_data.iv, &encrypted_data.auth_tag);
        assert_eq!(original, decrypted);
    }
}
