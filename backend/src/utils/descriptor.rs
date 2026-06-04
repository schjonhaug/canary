//! Descriptor utility functions for parsing and normalizing Bitcoin descriptors

use miniscript::{descriptor::Wildcard, Descriptor, DescriptorPublicKey, ForEachKey};
use regex::Regex;
use std::{error::Error, fmt};
use tracing::debug;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DescriptorError {
    InvalidStrippedDescriptor(String),
    InvalidDescriptor(String),
    NotMultipath,
    SplitMultipath(String),
    InvalidMultipathCount,
    HardenedDerivationAfterXpub,
}

impl fmt::Display for DescriptorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DescriptorError::InvalidStrippedDescriptor(error) => {
                write!(f, "Invalid stripped descriptor: {}", error)
            }
            DescriptorError::InvalidDescriptor(error) => write!(f, "Invalid descriptor: {}", error),
            DescriptorError::NotMultipath => f.write_str("Descriptor is not a multipath descriptor"),
            DescriptorError::SplitMultipath(error) => {
                write!(f, "Failed to split multipath descriptor: {}", error)
            }
            DescriptorError::InvalidMultipathCount => {
                f.write_str("Multipath descriptor must have exactly 2 paths (receive and change)")
            }
            DescriptorError::HardenedDerivationAfterXpub => f.write_str(
                "Invalid descriptor: hardened derivation steps cannot appear after an xpub. Put the account path in key origin metadata, for example [fingerprint/84h/0h/0h]xpub.../<0;1>/*.",
            ),
        }
    }
}

impl Error for DescriptorError {}

/// Strip key origin information from descriptor to prevent duplicates.
///
/// This function removes key origin paths (e.g., `[fingerprint/derivation/path]`)
/// from descriptors, which allows identifying duplicate wallets that differ
/// only in their key origin information.
///
/// # Arguments
/// * `descriptor_str` - The descriptor string, potentially with key origin and checksum
///
/// # Returns
/// A normalized descriptor string with key origin stripped and checksum recalculated
pub fn strip_key_origin(descriptor_str: &str) -> Result<String, DescriptorError> {
    // First strip any existing checksum (everything after #)
    let without_checksum = if let Some(pos) = descriptor_str.find('#') {
        &descriptor_str[..pos]
    } else {
        descriptor_str
    };

    // Pattern to match [fingerprint/derivation/path] anywhere in the descriptor
    // This handles both bare xpubs and script-wrapped descriptors like wpkh([fingerprint/path]xpub...)
    // Supports both 'h' and '\'' for hardened paths
    let key_origin_pattern = Regex::new(r"\[([0-9a-fA-F]{8})(/\d+[h']?)*\]").unwrap();

    // Strip key origin if present
    let stripped_without_checksum = if key_origin_pattern.is_match(without_checksum) {
        let result = key_origin_pattern.replace_all(without_checksum, "");
        debug!(" Stripped key origin: {} -> {}", without_checksum, result);
        result.to_string()
    } else {
        // No key origin found, return without checksum
        without_checksum.to_string()
    };

    // Parse the stripped descriptor to recalculate checksum
    let descriptor: Descriptor<DescriptorPublicKey> = stripped_without_checksum
        .parse::<Descriptor<DescriptorPublicKey>>()
        .map_err(|e| DescriptorError::InvalidStrippedDescriptor(e.to_string()))?;

    // Convert back to string with new checksum
    let final_descriptor = descriptor.to_string();
    debug!(" Final normalized descriptor: {}", final_descriptor);

    Ok(final_descriptor)
}

/// Parse a multipath descriptor and split it into receive and change descriptors.
///
/// Multipath descriptors contain multiple derivation paths (typically `/<0;1>/*`)
/// that need to be split into separate descriptors for the receive (external)
/// and change (internal) keychains.
///
/// # Arguments
/// * `descriptor_str` - A multipath descriptor string
///
/// # Returns
/// A tuple of `(receive_descriptor, change_descriptor)` strings
pub fn parse_multipath_descriptor(
    descriptor_str: &str,
) -> Result<(String, String), DescriptorError> {
    // Parse the descriptor
    let descriptor: Descriptor<DescriptorPublicKey> = descriptor_str
        .parse::<Descriptor<DescriptorPublicKey>>()
        .map_err(|e| DescriptorError::InvalidDescriptor(e.to_string()))?;

    validate_public_descriptor_derivable(&descriptor)?;

    // Check if it's a multipath descriptor
    if !descriptor.is_multipath() {
        return Err(DescriptorError::NotMultipath);
    }

    // Split multipath descriptor into receive and change descriptors
    let descriptors = descriptor
        .into_single_descriptors()
        .map_err(|e| DescriptorError::SplitMultipath(e.to_string()))?;

    if descriptors.len() != 2 {
        return Err(DescriptorError::InvalidMultipathCount);
    }

    let receive_descriptor = descriptors[0].to_string();
    let change_descriptor = descriptors[1].to_string();

    debug!(" Receive descriptor: {}", receive_descriptor);
    debug!(" Change descriptor: {}", change_descriptor);

    Ok((receive_descriptor, change_descriptor))
}

fn validate_public_descriptor_derivable(
    descriptor: &Descriptor<DescriptorPublicKey>,
) -> Result<(), DescriptorError> {
    if descriptor.for_any_key(public_key_has_hardened_derivation) {
        return Err(DescriptorError::HardenedDerivationAfterXpub);
    }

    Ok(())
}

fn public_key_has_hardened_derivation(key: &DescriptorPublicKey) -> bool {
    match key {
        DescriptorPublicKey::Single(_) => false,
        DescriptorPublicKey::XPub(xpub) => {
            xpub.wildcard == Wildcard::Hardened
                || xpub
                    .derivation_path
                    .as_ref()
                    .iter()
                    .any(|step| step.is_hardened())
        }
        DescriptorPublicKey::MultiXPub(xpub) => {
            xpub.wildcard == Wildcard::Hardened
                || xpub
                    .derivation_paths
                    .paths()
                    .iter()
                    .any(|path| path.as_ref().iter().any(|step| step.is_hardened()))
        }
    }
}

/// Extract the address from an `addr()` descriptor string (BIP-385).
/// We store address watches as `addr(ADDRESS)#checksum` — a standard descriptor format
/// supported by Bitcoin Core but not yet by rust-miniscript.
/// See: https://github.com/rust-bitcoin/rust-miniscript/issues/294
/// See: https://github.com/bitcoindevkit/bdk_wallet/issues/174
pub fn extract_address_from_descriptor(descriptor_str: &str) -> Option<String> {
    let without_checksum = if let Some(pos) = descriptor_str.find('#') {
        &descriptor_str[..pos]
    } else {
        descriptor_str
    };
    let trimmed = without_checksum.trim();
    if trimmed.starts_with("addr(") && trimmed.ends_with(')') {
        Some(trimmed[5..trimmed.len() - 1].to_string())
    } else {
        None
    }
}

/// Extract the public key from a `pk()` descriptor string.
/// We store P2PK watches as `pk(PUBKEY)#checksum` — a standard descriptor format.
pub fn extract_pubkey_from_descriptor(descriptor_str: &str) -> Option<String> {
    let without_checksum = if let Some(pos) = descriptor_str.find('#') {
        &descriptor_str[..pos]
    } else {
        descriptor_str
    };
    let trimmed = without_checksum.trim();
    if trimmed.starts_with("pk(") && trimmed.ends_with(')') {
        Some(trimmed[3..trimmed.len() - 1].to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_TPUB: &str = "tpubDDDa5znrsZrYc3yVHe1iGrmsdrfSELKXK9AkkJL9LNQB2FwTbgtZBdVEunSv5qdLADWyTDXcA5scsjGBjPGsrWmxHuanS6nH5iRh3uZ4Uj5";

    #[test]
    fn test_parse_multipath_descriptor_rejects_hardened_derivation_after_xpub() {
        let descriptor = format!("wpkh({VALID_TPUB}/84h/1h/0h/<0;1>/*)");

        let result = parse_multipath_descriptor(&descriptor);

        assert_eq!(
            result.unwrap_err(),
            DescriptorError::HardenedDerivationAfterXpub
        );
    }

    #[test]
    fn test_parse_multipath_descriptor_rejects_hardened_derivation_in_sh_wpkh() {
        let descriptor = format!("sh(wpkh({VALID_TPUB}/84h/1h/0h/<0;1>/*))");

        let result = parse_multipath_descriptor(&descriptor);

        assert_eq!(
            result.unwrap_err(),
            DescriptorError::HardenedDerivationAfterXpub
        );
    }

    #[test]
    fn test_parse_descriptor_rejects_single_path_hardened_derivation_after_xpub() {
        let descriptor = format!("wpkh({VALID_TPUB}/84h/1h/0h/*)");

        let result = parse_multipath_descriptor(&descriptor);

        assert_eq!(
            result.unwrap_err(),
            DescriptorError::HardenedDerivationAfterXpub
        );
    }

    #[test]
    fn test_parse_multipath_descriptor_rejects_hardened_wildcard_after_xpub() {
        let descriptor = format!("wpkh({VALID_TPUB}/<0;1>/*h)");

        let result = parse_multipath_descriptor(&descriptor);

        assert_eq!(
            result.unwrap_err(),
            DescriptorError::HardenedDerivationAfterXpub
        );
    }

    #[test]
    fn test_parse_multipath_descriptor_accepts_key_origin_hardened_path() {
        let descriptor = format!("wpkh([805c684b/84h/1h/0h]{VALID_TPUB}/<0;1>/*)");
        let normalized = strip_key_origin(&descriptor).unwrap();

        let result = parse_multipath_descriptor(&normalized);

        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_multipath_descriptor_accepts_account_level_xpub() {
        let descriptor = format!("wpkh({VALID_TPUB}/<0;1>/*)");

        let result = parse_multipath_descriptor(&descriptor);

        assert!(result.is_ok());
    }

    #[test]
    fn test_strip_key_origin_with_origin() {
        let descriptor = "wpkh([12345678/84h/0h/0h]xpub6...)/<0;1>/*";
        // This will fail to parse because of the fake xpub, but the pattern matching should work
        let result = strip_key_origin(descriptor);
        // We expect an error since the xpub is invalid, but the test shows the function runs
        assert!(result.is_err());
    }

    #[test]
    fn test_strip_key_origin_without_origin() {
        let descriptor = "wpkh(xpub6...)/<0;1>/*";
        let result = strip_key_origin(descriptor);
        // We expect an error since the xpub is invalid
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_multipath_non_multipath() {
        let descriptor = "wpkh(xpub6...)/*";
        let result = parse_multipath_descriptor(descriptor);
        // We expect an error since it's not a valid descriptor
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_pubkey_from_descriptor_with_checksum() {
        let result = extract_pubkey_from_descriptor(
            "pk(04678afdb0fe5548271967f1a67130b7105cd6a828e03909a67962e0ea1f61deb649f6bc3f4cef38c4f35504e51ec112de5c384df7ba0b8d578a4c702b6bf11d5f)#abcdef12",
        );
        assert_eq!(
            result,
            Some("04678afdb0fe5548271967f1a67130b7105cd6a828e03909a67962e0ea1f61deb649f6bc3f4cef38c4f35504e51ec112de5c384df7ba0b8d578a4c702b6bf11d5f".to_string())
        );
    }

    #[test]
    fn test_extract_pubkey_from_descriptor_without_checksum() {
        let result = extract_pubkey_from_descriptor(
            "pk(04678afdb0fe5548271967f1a67130b7105cd6a828e03909a67962e0ea1f61deb649f6bc3f4cef38c4f35504e51ec112de5c384df7ba0b8d578a4c702b6bf11d5f)",
        );
        assert!(result.is_some());
    }

    #[test]
    fn test_extract_pubkey_from_descriptor_rejects_addr() {
        let result =
            extract_pubkey_from_descriptor("addr(1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa)#checksum");
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_pubkey_from_descriptor_rejects_invalid() {
        assert!(extract_pubkey_from_descriptor("wpkh(xpub...)").is_none());
        assert!(extract_pubkey_from_descriptor("not_a_descriptor").is_none());
        assert!(extract_pubkey_from_descriptor("").is_none());
    }

    #[test]
    fn test_extract_address_from_descriptor_valid() {
        let result =
            extract_address_from_descriptor("addr(1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa)#abc123");
        assert_eq!(
            result,
            Some("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".to_string())
        );
    }

    #[test]
    fn test_extract_address_from_descriptor_rejects_pk() {
        let result = extract_address_from_descriptor("pk(04abc...)#checksum");
        assert!(result.is_none());
    }
}
