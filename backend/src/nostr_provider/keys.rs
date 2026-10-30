//! The instance's Nostr sender identity: generated once and stored as an instance secret.

use crate::metadata::MetadataDb;
use anyhow::Result;
use nostr_sdk::prelude::*;

const NOSTR_SENDER_SECRET_KEY: &str = "nostr_sender_secret_key";

#[derive(Clone)]
pub struct NostrSenderKeys {
    pub(super) keys: Keys,
    pub sender_npub: String,
}

impl std::fmt::Debug for NostrSenderKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NostrSenderKeys")
            .field("sender_npub", &self.sender_npub)
            .field("keys", &"[redacted]")
            .finish()
    }
}

pub async fn ensure_nostr_sender_keys(metadata_db: &MetadataDb) -> Result<NostrSenderKeys> {
    if let Some(secret_hex) = metadata_db
        .get_instance_secret(NOSTR_SENDER_SECRET_KEY)
        .await?
    {
        return sender_keys_from_secret(&secret_hex);
    }

    let keys = Keys::generate();
    let secret_hex = keys.secret_key().to_secret_hex();
    metadata_db
        .set_instance_secret_if_absent(NOSTR_SENDER_SECRET_KEY, &secret_hex)
        .await?;

    let secret_hex = metadata_db
        .get_instance_secret(NOSTR_SENDER_SECRET_KEY)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Nostr sender key missing after insert"))?;
    sender_keys_from_secret(&secret_hex)
}

fn sender_keys_from_secret(secret_hex: &str) -> Result<NostrSenderKeys> {
    let keys = Keys::parse(secret_hex)?;
    let sender_npub = keys.public_key().to_bech32()?;
    Ok(NostrSenderKeys { keys, sender_npub })
}
