//! Sealing a root [`EntryRatchet`] *state* to a shareholder.
//!
//! The vault root is ratcheted like every other entity, but its key is
//! what gets *distributed*: shareholders must hold the ratchet state,
//! not just a derived key, so they can bring it forward to any later
//! height themselves (`inc_by(height - sealed_at)`). That is what lets
//! `save()` just advance instead of re-minting every share, and what
//! makes a grant "from this version onward" by construction.
//!
//! Same construction as [`SecretShare`](crate::SecretShare): ephemeral
//! X25519 → shared secret → AES-256 key-encryption-key. The payload is
//! the bincode-serialized ratchet (~160 bytes, not a multiple of 8), so
//! this uses AES-KW **with padding** (RFC 5649) rather than plain KW.
//!
//! Revocation consequence: a holder can derive every *later* key from
//! their state, so revoking someone means re-seeding the root lineage
//! and re-sealing to the remaining holders. Nothing here enforces that;
//! the vault does.

use serde::{Deserialize, Serialize};

use crate::keys::{PrivateKey, PublicKey, SharingPrivateKey, SharingPublicKey, PUBLIC_KEY_SIZE};
use crate::ratchet::EntryRatchet;
use aes_kw::KekAes256 as Kek;

#[derive(thiserror::Error, Debug)]
pub enum RatchetShareError {
    #[error("key error: {0}")]
    Key(#[from] crate::keys::KeyError),
    #[error("AES-KW {0} error")]
    KeyWrap(&'static str),
    #[error("ratchet state malformed: {0}")]
    Codec(String),
    #[error("share too short")]
    TooShort,
}

/// A root ratchet state sealed to one recipient, plus the manifest
/// height whose root key that state derives — the holder advances by
/// `current_height - sealed_at` to catch up.
#[serde_with::serde_as]
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RatchetShare {
    /// `ephemeral_pubkey (32) || aes_kw_padded(bincode(ratchet))`.
    #[serde_as(as = "serde_with::Bytes")]
    sealed: Vec<u8>,
    sealed_at: u64,
}

impl RatchetShare {
    /// Seal `ratchet` — the root state that derives the root key at
    /// manifest height `sealed_at` — to `recipient`.
    pub fn new(
        ratchet: &EntryRatchet,
        recipient: &PublicKey,
        sealed_at: u64,
    ) -> Result<Self, RatchetShareError> {
        let ephemeral_private = PrivateKey::generate();
        let ephemeral_public = ephemeral_private.public();
        let ephemeral_sharing = SharingPrivateKey::from(&ephemeral_private);
        let recipient_sharing = SharingPublicKey::try_from(recipient)?;
        let kek = Kek::from(ephemeral_sharing.shared_secret(&recipient_sharing));

        let plaintext =
            bincode::serialize(ratchet).map_err(|e| RatchetShareError::Codec(e.to_string()))?;
        let wrapped = kek
            .wrap_with_padding_vec(&plaintext)
            .map_err(|_| RatchetShareError::KeyWrap("wrap"))?;

        let mut sealed = Vec::with_capacity(PUBLIC_KEY_SIZE + wrapped.len());
        sealed.extend_from_slice(&ephemeral_public.to_bytes());
        sealed.extend_from_slice(&wrapped);
        Ok(Self { sealed, sealed_at })
    }

    /// Recover the sealed root state. The caller advances it to the
    /// height it wants to read (see [`Self::sealed_at`]).
    pub fn recover(
        &self,
        recipient_secret: &PrivateKey,
    ) -> Result<EntryRatchet, RatchetShareError> {
        if self.sealed.len() <= PUBLIC_KEY_SIZE {
            return Err(RatchetShareError::TooShort);
        }
        let ephemeral_public = PublicKey::try_from(&self.sealed[..PUBLIC_KEY_SIZE])?;
        let recipient_sharing = SharingPrivateKey::from(recipient_secret);
        let ephemeral_sharing = SharingPublicKey::try_from(&ephemeral_public)?;
        let kek = Kek::from(recipient_sharing.shared_secret(&ephemeral_sharing));

        let plaintext = kek
            .unwrap_with_padding_vec(&self.sealed[PUBLIC_KEY_SIZE..])
            .map_err(|_| RatchetShareError::KeyWrap("unwrap"))?;
        bincode::deserialize(&plaintext).map_err(|e| RatchetShareError::Codec(e.to_string()))
    }

    /// Manifest height whose root key the sealed state derives.
    pub fn sealed_at(&self) -> u64 {
        self.sealed_at
    }
}

impl std::fmt::Debug for RatchetShare {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "RatchetShare(sealed_at={}, {} bytes)",
            self.sealed_at,
            self.sealed.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bob_recovers_the_root_state_alice_sealed_and_catches_up() {
        // Alice seals the root ratchet at height 3 to Bob.
        let bob = PrivateKey::generate();
        let root_at_3 = EntryRatchet::seed().advanced_by(3);
        let share = RatchetShare::new(&root_at_3, &bob.public(), 3).unwrap();

        // Bob recovers the exact state…
        let recovered = share.recover(&bob).unwrap();
        assert_eq!(recovered, root_at_3);
        assert_eq!(share.sealed_at(), 3);

        // …and brings it forward to height 5 to read the current version.
        assert_eq!(
            recovered.advanced_by(5 - 3).key(),
            root_at_3.advanced_by(2).key()
        );
    }

    #[test]
    fn a_share_sealed_to_bob_is_useless_to_mallory() {
        let bob = PrivateKey::generate();
        let mallory = PrivateKey::generate();
        let share = RatchetShare::new(&EntryRatchet::seed(), &bob.public(), 0).unwrap();
        assert!(share.recover(&mallory).is_err());
    }

    #[test]
    fn ratchet_share_round_trips_through_serde() {
        let bob = PrivateKey::generate();
        let share = RatchetShare::new(&EntryRatchet::seed(), &bob.public(), 7).unwrap();
        let bytes = bincode::serialize(&share).unwrap();
        let back: RatchetShare = bincode::deserialize(&bytes).unwrap();
        assert_eq!(back, share);
        assert!(back.recover(&bob).is_ok());
    }
}
