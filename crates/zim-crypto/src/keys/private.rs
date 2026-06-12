use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};

use super::{KeyError, PublicKey, PRIVATE_KEY_SIZE};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivateKey(pub SigningKey);

impl From<[u8; PRIVATE_KEY_SIZE]> for PrivateKey {
    fn from(secret: [u8; PRIVATE_KEY_SIZE]) -> Self {
        Self(SigningKey::from_bytes(&secret))
    }
}

impl From<SigningKey> for PrivateKey {
    fn from(key: SigningKey) -> Self {
        Self(key)
    }
}

impl PrivateKey {
    pub fn from_hex(hex: &str) -> Result<Self, KeyError> {
        let hex = hex.strip_prefix("0x").unwrap_or(hex);
        let mut buff = [0; PRIVATE_KEY_SIZE];
        hex::decode_to_slice(hex, &mut buff)
            .map_err(|_| anyhow::anyhow!("private key hex decode error"))?;
        Ok(Self::from(buff))
    }

    pub fn generate() -> Self {
        let mut bytes = [0u8; PRIVATE_KEY_SIZE];
        getrandom::getrandom(&mut bytes).expect("failed to generate random bytes");
        Self::from(bytes)
    }

    pub fn public(&self) -> PublicKey {
        PublicKey::from(self.0.verifying_key())
    }

    pub fn to_bytes(&self) -> [u8; PRIVATE_KEY_SIZE] {
        self.0.to_bytes()
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.to_bytes())
    }

    pub fn to_pem(&self) -> String {
        let pem = pem::Pem::new("PRIVATE KEY", self.to_bytes());
        pem::encode(&pem)
    }

    pub fn from_pem(pem_str: &str) -> Result<Self, KeyError> {
        let pem = pem::parse(pem_str).map_err(|e| anyhow::anyhow!("failed to parse PEM: {}", e))?;

        if pem.tag() != "PRIVATE KEY" {
            return Err(anyhow::anyhow!("invalid PEM tag, expected PRIVATE KEY").into());
        }

        let contents = pem.contents();
        if contents.len() != PRIVATE_KEY_SIZE {
            return Err(anyhow::anyhow!(
                "invalid private key size in PEM, expected {}, got {}",
                PRIVATE_KEY_SIZE,
                contents.len()
            )
            .into());
        }

        let mut bytes = [0u8; PRIVATE_KEY_SIZE];
        bytes.copy_from_slice(contents);
        Ok(Self::from(bytes))
    }

    pub fn sign(&self, msg: &[u8]) -> ed25519_dalek::Signature {
        use ed25519_dalek::Signer;
        self.0.sign(msg)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_generate_and_bytes_roundtrip() {
        let key = PrivateKey::generate();
        let bytes = key.to_bytes();
        let recovered = PrivateKey::from(bytes);
        assert_eq!(key.to_bytes(), recovered.to_bytes());
    }

    #[test]
    fn test_hex_roundtrip() {
        let key = PrivateKey::generate();
        let hex = key.to_hex();
        let recovered = PrivateKey::from_hex(&hex).unwrap();
        assert_eq!(key.to_bytes(), recovered.to_bytes());
    }

    #[test]
    fn test_pem_roundtrip() {
        let key = PrivateKey::generate();
        let pem = key.to_pem();
        let recovered = PrivateKey::from_pem(&pem).unwrap();
        assert_eq!(key.to_bytes(), recovered.to_bytes());
    }
}
