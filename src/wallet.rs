use ed25519_dalek::{Signer, SigningKey, VerifyingKey, Signature, Verifier};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

/// KeyPair — ed25519, more secure than BTC's secp256k1 ECDSA:
/// - Deterministic, not malleable (BTC had txn malleability until SegWit)
/// - Faster batch verification
/// - 32-byte pubkeys (vs 33 compressed secp256k1)
/// - No fragile k-nonce reuse bug (ECDSA fails catastrophically if k repeats)
///
/// Private key is 32 bytes, stored hex. In production, encrypt at rest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyPair {
    pub secret_hex: String, // 32 bytes
    pub public_hex: String, // 32 bytes
}

impl KeyPair {
    pub fn generate() -> Self {
        let mut csprng = OsRng;
        let signing = SigningKey::generate(&mut csprng);
        let verifying = signing.verifying_key();
        Self {
            secret_hex: hex::encode(signing.to_bytes()),
            public_hex: hex::encode(verifying.to_bytes()),
        }
    }

    pub fn from_secret_hex(hex_str: &str) -> Result<Self, String> {
        let bytes = hex::decode(hex_str).map_err(|e| e.to_string())?;
        if bytes.len() != 32 {
            return Err("secret must be 32 bytes".to_string());
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        let signing = SigningKey::from_bytes(&arr);
        let verifying = signing.verifying_key();
        Ok(Self {
            secret_hex: hex_str.to_string(),
            public_hex: hex::encode(verifying.to_bytes()),
        })
    }

    pub fn sign(&self, msg: &[u8]) -> String {
        let bytes = hex::decode(&self.secret_hex).unwrap();
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        let signing = SigningKey::from_bytes(&arr);
        let sig: Signature = signing.sign(msg);
        hex::encode(sig.to_bytes())
    }

    pub fn public(&self) -> String {
        self.public_hex.clone()
    }

    pub fn address(&self) -> String {
        // Address = hex pubkey (simplified). For display, we use pubkey directly.
        // Could also do BLAKE3(pubkey)[0..20] like BTC hash160, but keep full for security.
        self.public_hex.clone()
    }
}

/// Verify ed25519 signature: pub_hex 64 chars, msg bytes, sig_hex 128 chars
pub fn verify_signature(pub_hex: &str, msg: &[u8], sig_hex: &str) -> Result<(), String> {
    let pub_bytes = hex::decode(pub_hex).map_err(|e| format!("bad pubkey hex: {}", e))?;
    if pub_bytes.len() != 32 {
        return Err("pubkey len !=32".to_string());
    }
    let sig_bytes = hex::decode(sig_hex).map_err(|e| format!("bad sig hex: {}", e))?;
    if sig_bytes.len() != 64 {
        return Err("sig len !=64".to_string());
    }
    let mut pk_arr = [0u8; 32];
    pk_arr.copy_from_slice(&pub_bytes);
    let vk = VerifyingKey::from_bytes(&pk_arr).map_err(|e| e.to_string())?;
    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(&sig_bytes);
    let sig = Signature::from_bytes(&sig_arr);
    vk.verify(msg, &sig).map_err(|e| e.to_string())
}

/// Wallet — manages KeyPair + nonce + balance view
#[derive(Debug)]
pub struct Wallet {
    pub keypair: KeyPair,
    pub nonce: u64,
}

impl Wallet {
    pub fn new(keypair: KeyPair) -> Self {
        Self { keypair, nonce: 0 }
    }

    pub fn generate() -> Self {
        Self::new(KeyPair::generate())
    }

    pub fn address(&self) -> String {
        self.keypair.address()
    }

    pub fn sign_transaction(&mut self, tx: &mut crate::block::Transaction) {
        let msg = tx.signing_bytes();
        let sig = self.keypair.sign(&msg);
        tx.signature = Some(sig);
        // bump nonce for next tx
        self.nonce += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_verify() {
        let kp = KeyPair::generate();
        let msg = b"hello neko";
        let sig = kp.sign(msg);
        verify_signature(&kp.public_hex, msg, &sig).unwrap();
        // tamper
        assert!(verify_signature(&kp.public_hex, b"hello neko!", &sig).is_err());
    }
}
