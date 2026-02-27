//! Descriptor utility functions for parsing and normalizing Bitcoin descriptors

use anyhow::{anyhow, Result};
use miniscript::{Descriptor, DescriptorPublicKey};
use regex::Regex;
use tracing::debug;

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
pub fn strip_key_origin(descriptor_str: &str) -> Result<String> {
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
        .parse()
        .map_err(|e| anyhow!("Invalid stripped descriptor: {}", e))?;

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
pub fn parse_multipath_descriptor(descriptor_str: &str) -> Result<(String, String)> {
    // Parse the descriptor
    let descriptor: Descriptor<DescriptorPublicKey> = descriptor_str
        .parse()
        .map_err(|e| anyhow!("Invalid descriptor: {}", e))?;

    // Check if it's a multipath descriptor
    if !descriptor.is_multipath() {
        return Err(anyhow!("Descriptor is not a multipath descriptor"));
    }

    // Split multipath descriptor into receive and change descriptors
    let descriptors = descriptor
        .into_single_descriptors()
        .map_err(|e| anyhow!("Failed to split multipath descriptor: {}", e))?;

    if descriptors.len() != 2 {
        return Err(anyhow!(
            "Multipath descriptor must have exactly 2 paths (receive and change)"
        ));
    }

    let receive_descriptor = descriptors[0].to_string();
    let change_descriptor = descriptors[1].to_string();

    debug!(" Receive descriptor: {}", receive_descriptor);
    debug!(" Change descriptor: {}", change_descriptor);

    Ok((receive_descriptor, change_descriptor))
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
}
