//! Nostr DM notifications.
//!
//! - `keys` owns the instance's sender identity.
//! - `recipient` parses and canonicalises recipient public keys.
//! - `mode` owns the NIP-17/NIP-04 mode setting.
//! - `provider` owns `NostrProvider` and its `NotificationProvider` impl.
//! - `dm` builds and publishes NIP-17 and NIP-04 messages.
//! - `relays` discovers, filters and connects relays.
//! - `client` builds the signing client and parses the `.onion` SOCKS proxy setting.
//! - `errors` owns `NostrError`, its user-facing messages and API error codes.

mod client;
mod dm;
mod errors;
mod keys;
mod mode;
mod provider;
mod recipient;
mod relays;
#[cfg(test)]
mod tests;

pub use client::parse_nostr_onion_socks_proxy;
pub use dm::NostrSendSuccess;
pub use errors::{NostrError, RelayFailures};
pub use keys::{NostrSenderKeys, ensure_nostr_sender_keys};
pub use mode::{NostrDmMode, UnsupportedNostrDmMode, get_nostr_dm_mode, set_nostr_dm_mode};
pub use provider::NostrProvider;
pub use recipient::{
    canonicalize_nostr_public_key, normalize_nostr_recipient_or_error, nostr_display_target,
    parse_nostr_recipient_or_error,
};

const NOSTR_SEND_CONCURRENCY: usize = 3;
