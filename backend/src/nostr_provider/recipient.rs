//! Parsing and canonicalising recipient public keys (npub or hex) for storage and display.

use super::errors::NostrError;
use nostr_sdk::prelude::*;

/// Parses a recipient public key given as `npub` or hex. Empty input and `nsec` private keys
/// are rejected with a user-facing message.
pub fn parse_nostr_recipient_or_error(input: &str) -> Result<PublicKey, NostrError> {
    let value = input.trim();
    if value.is_empty() {
        return Err(NostrError::EmptyRecipient);
    }

    if value.to_lowercase().starts_with("nsec") {
        return Err(NostrError::PrivateKeyAsRecipient);
    }

    PublicKey::parse(value).map_err(NostrError::InvalidRecipient)
}

/// Parses a recipient public key and returns it as `(hex, npub)`.
pub fn canonicalize_nostr_public_key(input: &str) -> Result<(String, String), NostrError> {
    let public_key = parse_nostr_recipient_or_error(input)?;
    // Bech32-encoding a `PublicKey` is infallible.
    let Ok(npub) = public_key.to_bech32();
    Ok((public_key.to_hex(), npub))
}

/// Formats a stored hex public key as an `npub` for display, or `None` if it does not parse.
pub fn nostr_display_target(hex_public_key: &str) -> Option<String> {
    PublicKey::parse(hex_public_key)
        .ok()
        .and_then(|public_key| public_key.to_bech32().ok())
}

/// Parses a recipient public key and returns the hex form stored as the notification target.
pub fn normalize_nostr_recipient_or_error(input: &str) -> Result<String, NostrError> {
    canonicalize_nostr_public_key(input).map(|(hex, _)| hex)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalizes_npub_and_hex_public_keys() {
        let keys = Keys::generate();
        let public_key = keys.public_key();
        let hex = public_key.to_hex();
        let npub = public_key.to_bech32().unwrap();

        assert_eq!(
            canonicalize_nostr_public_key(&format!(" {npub} ")).unwrap(),
            (hex.clone(), npub.clone())
        );
        assert_eq!(
            canonicalize_nostr_public_key(&hex.to_uppercase()).unwrap(),
            (hex, npub)
        );
    }

    #[test]
    fn rejects_empty_invalid_and_private_keys() {
        assert!(matches!(
            canonicalize_nostr_public_key("   "),
            Err(NostrError::EmptyRecipient)
        ));
        assert!(matches!(
            canonicalize_nostr_public_key("nsec1not-a-recipient"),
            Err(NostrError::PrivateKeyAsRecipient)
        ));
        let invalid = canonicalize_nostr_public_key("not-a-public-key").unwrap_err();
        assert!(matches!(invalid, NostrError::InvalidRecipient(_)));
        assert_eq!(
            invalid.to_string(),
            "Enter a valid Nostr public key as npub or 64-character hex"
        );
    }

    #[test]
    fn formats_display_target_from_stored_hex_public_key() {
        let public_key = Keys::generate().public_key();
        let npub = public_key.to_bech32().unwrap();

        assert_eq!(nostr_display_target(&public_key.to_hex()), Some(npub));
        assert_eq!(nostr_display_target("not-a-public-key"), None);
    }
}
