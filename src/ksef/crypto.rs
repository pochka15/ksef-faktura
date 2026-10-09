//! The three primitives KSeF needs: RSA-OAEP(SHA-256) with the Ministry's public key, AES-256-CBC with
//! PKCS#7 for the invoice, and base64 SHA-256 hashes.

use aes::cipher::{BlockEncryptMut, KeyIvInit, block_padding::Pkcs7};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use rsa::pkcs8::DecodePublicKey;
use rsa::{Oaep, RsaPublicKey};
use sha2::{Digest, Sha256};
use x509_cert::Certificate;
use x509_cert::der::{Decode, Encode};

pub fn b64(data: &[u8]) -> String {
    STANDARD.encode(data)
}

pub fn unb64(text: &str) -> Result<Vec<u8>, String> {
    STANDARD
        .decode(text.trim())
        .map_err(|e| format!("bad base64: {e}"))
}

pub fn sha256_b64(data: &[u8]) -> String {
    b64(&Sha256::digest(data))
}

/// RSA public key out of a base64 DER X.509 certificate (the form `/security/public-key-certificates` uses).
pub fn public_key_from_certificate(der_b64: &str) -> Result<RsaPublicKey, String> {
    let der = unb64(der_b64)?;
    let cert = Certificate::from_der(&der).map_err(|e| format!("bad certificate: {e}"))?;
    let spki = cert
        .tbs_certificate
        .subject_public_key_info
        .to_der()
        .map_err(|e| format!("bad public key: {e}"))?;
    RsaPublicKey::from_public_key_der(&spki).map_err(|e| format!("not an RSA key: {e}"))
}

pub fn rsa_encrypt(key: &RsaPublicKey, data: &[u8]) -> Result<Vec<u8>, String> {
    key.encrypt(&mut rand::thread_rng(), Oaep::new::<Sha256>(), data)
        .map_err(|e| format!("RSA encryption failed: {e}"))
}

pub fn aes_encrypt(key: &[u8; 32], iv: &[u8; 16], data: &[u8]) -> Vec<u8> {
    cbc::Encryptor::<aes::Aes256>::new(key.into(), iv.into()).encrypt_padded_vec_mut::<Pkcs7>(data)
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut bytes);
    bytes
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use aes::cipher::BlockDecryptMut;
    use rsa::RsaPrivateKey;
    use rsa::pkcs8::DecodePrivateKey;

    /// A throwaway key pair made for these tests (`tests/fixtures`), standing in for the Ministry's.
    pub const TEST_CERT_B64: &str = include_str!("../../tests/fixtures/test-cert.der.b64");
    const TEST_KEY: &str = include_str!("../../tests/fixtures/test-key.pem");

    pub fn rsa_decrypt(data: &[u8]) -> Vec<u8> {
        let key = RsaPrivateKey::from_pkcs8_pem(TEST_KEY).unwrap();
        key.decrypt(Oaep::new::<Sha256>(), data).unwrap()
    }

    pub fn aes_decrypt(key: &[u8], iv: &[u8], data: &[u8]) -> Vec<u8> {
        cbc::Decryptor::<aes::Aes256>::new(key.into(), iv.into())
            .decrypt_padded_vec_mut::<Pkcs7>(data)
            .unwrap()
    }

    #[test]
    fn rsa_round_trip_through_the_certificate() {
        let key = public_key_from_certificate(TEST_CERT_B64).unwrap();
        let secret = b"token|1760000000000";
        assert_eq!(rsa_decrypt(&rsa_encrypt(&key, secret).unwrap()), secret);
    }

    #[test]
    fn aes_round_trip_with_padding() {
        let (key, iv) = (random_bytes::<32>(), random_bytes::<16>());
        let encrypted = aes_encrypt(&key, &iv, b"<Faktura/>");
        assert_eq!(encrypted.len(), 16, "padded to one block");
        assert_eq!(aes_decrypt(&key, &iv, &encrypted), b"<Faktura/>");
    }

    #[test]
    fn hashes_are_base64_sha256() {
        assert_eq!(sha256_b64(b"abc"), "ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0=");
    }
}
