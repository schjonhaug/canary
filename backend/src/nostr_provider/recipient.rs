//! Parsing and canonicalising recipient public keys (npub or hex) for storage and display.

use nostr_sdk::prelude::*;

pub fn parse_nostr_recipient_or_error(input: &str) -> Result<PublicKey, String> {
    let value = input.trim();
    if value.is_empty() {
        return Err("Nostr recipient cannot be empty".to_string());
    }

    if value.to_lowercase().starts_with("nsec") {
        return Err("Enter a recipient npub, not a private nsec key".to_string());
    }

    PublicKey::parse(value)
        .map_err(|_| "Enter a valid Nostr public key as npub or 64-character hex".to_string())
}

pub fn canonicalize_nostr_public_key(input: &str) -> Result<(String, String), String> {
    let public_key = parse_nostr_recipient_or_error(input)?;
    let npub = public_key
        .to_bech32()
        .map_err(|_| "Failed to format Nostr public key".to_string())?;
    Ok((public_key.to_hex(), npub))
}

pub fn nostr_display_target(hex_public_key: &str) -> Option<String> {
    PublicKey::parse(hex_public_key)
        .ok()
        .and_then(|public_key| public_key.to_bech32().ok())
}

pub fn normalize_nostr_recipient_or_error(input: &str) -> Result<String, String> {
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
        assert!(canonicalize_nostr_public_key("   ")
            .unwrap_err()
            .contains("cannot be empty"));
        assert!(canonicalize_nostr_public_key("nsec1not-a-recipient")
            .unwrap_err()
            .contains("not a private nsec key"));
        assert!(canonicalize_nostr_public_key("not-a-public-key")
            .unwrap_err()
            .contains("valid Nostr public key"));
    }

    #[test]
    fn formats_display_target_from_stored_hex_public_key() {
        let public_key = Keys::generate().public_key();
        let npub = public_key.to_bech32().unwrap();

        assert_eq!(nostr_display_target(&public_key.to_hex()), Some(npub));
        assert_eq!(nostr_display_target("not-a-public-key"), None);
    }
}
