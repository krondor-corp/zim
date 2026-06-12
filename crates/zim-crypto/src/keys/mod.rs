mod private;
mod public;
mod sharing;

pub use private::PrivateKey;
pub use public::PublicKey;
pub use sharing::{SharingPrivateKey, SharingPublicKey};

pub const PRIVATE_KEY_SIZE: usize = 32;
pub const PUBLIC_KEY_SIZE: usize = 32;

#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("key error: {0}")]
    Default(#[from] anyhow::Error),
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_keypair_generation() {
        let private_key = PrivateKey::generate();
        let public_key = private_key.public();

        let private_hex = private_key.to_hex();
        let recovered_private = PrivateKey::from_hex(&private_hex).unwrap();
        assert_eq!(private_key.to_bytes(), recovered_private.to_bytes());

        let public_hex = public_key.to_hex();
        let recovered_public = PublicKey::from_hex(&public_hex).unwrap();
        assert_eq!(public_key.to_bytes(), recovered_public.to_bytes());
    }

    #[test]
    fn test_pem_serialization() {
        let private_key = PrivateKey::generate();

        let pem = private_key.to_pem();
        let recovered_private = PrivateKey::from_pem(&pem).unwrap();
        assert_eq!(private_key.to_bytes(), recovered_private.to_bytes());

        assert_eq!(
            private_key.public().to_bytes(),
            recovered_private.public().to_bytes()
        );
    }

    #[test]
    fn test_sign_and_verify() {
        let secret_key = PrivateKey::generate();
        let public_key = secret_key.public();
        let message = b"hello, world!";

        let signature = secret_key.sign(message);
        assert!(public_key.verify(message, &signature).is_ok());

        let wrong_message = b"hello, world?";
        assert!(public_key.verify(wrong_message, &signature).is_err());

        let other_key = PrivateKey::generate().public();
        assert!(other_key.verify(message, &signature).is_err());
    }
}
