use curve25519_dalek::edwards::CompressedEdwardsY;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};

use super::{KeyError, PrivateKey, PublicKey, PRIVATE_KEY_SIZE};

pub struct SharingPublicKey(X25519PublicKey);

impl TryFrom<&PublicKey> for SharingPublicKey {
    type Error = KeyError;
    fn try_from(key: &PublicKey) -> Result<Self, Self::Error> {
        let edwards_point = CompressedEdwardsY::from_slice(&key.to_bytes())
            .map_err(|_| anyhow::anyhow!("public key invalid edwards point"))?
            .decompress()
            .ok_or_else(|| anyhow::anyhow!("public key failed to decompress edwards point"))?;
        Ok(Self(X25519PublicKey::from(
            edwards_point.to_montgomery().to_bytes(),
        )))
    }
}

pub struct SharingPrivateKey(StaticSecret);

impl From<&PrivateKey> for SharingPrivateKey {
    fn from(key: &PrivateKey) -> Self {
        Self(StaticSecret::from(key.0.to_scalar_bytes()))
    }
}

impl SharingPrivateKey {
    pub fn shared_secret(&self, public: &SharingPublicKey) -> [u8; PRIVATE_KEY_SIZE] {
        *self.0.diffie_hellman(&public.0).as_bytes()
    }
}
