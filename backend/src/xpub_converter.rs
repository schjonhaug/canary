use anyhow::{anyhow, Result};
use bdk_wallet::bitcoin::{Address, Network, PublicKey};
use miniscript::descriptor::checksum::Engine as DescriptorChecksumEngine;
use regex::Regex;
use std::str::FromStr;
use std::sync::LazyLock;
use xyzpub::{convert_version, Version};

/// BIP32 and SLIP-132 extended public keys, including Sparrow multisig `Ypub`/`Zpub`/`Upub`/`Vpub`.
static EXTENDED_PUBLIC_KEY_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(xpub|ypub|zpub|Ypub|Zpub|tpub|upub|vpub|Upub|Vpub)[1-9A-HJ-NP-Za-km-z]{107,108}")
        .expect("extended public key pattern")
});

static BARE_SINGLE_SIG_EXTENDED_KEY_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[xyztuv]pub[1-9A-HJ-NP-Za-km-z]{107,108}$").expect("bare xpub pattern")
});

fn descriptor_checksum(descriptor: &str) -> Result<String> {
    let mut engine = DescriptorChecksumEngine::new();
    engine
        .input(descriptor)
        .map_err(|e| anyhow!("Failed to calculate descriptor checksum: {}", e))?;
    Ok(engine.checksum())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScriptType {
    P2PKH,  // Legacy
    P2SH,   // Nested SegWit
    P2WPKH, // Native SegWit
    P2TR,   // Taproot
}

pub struct XpubConverter {
    network: Network,
}

impl XpubConverter {
    pub fn new(
        network: Network,
        _electrum_client: Option<&crate::electrum::ElectrumClient>,
    ) -> Self {
        Self { network }
    }

    /// Check if the input is a bare single-sig extended public key (xpub/ypub/zpub/tpub/upub/vpub).
    ///
    /// Uppercase SLIP-132 multisig prefixes (`Ypub`/`Zpub`/`Upub`/`Vpub`) are excluded so a
    /// pasted Sparrow multisig key is not wrapped as a single-sig descriptor.
    pub fn is_xpub(input: &str) -> bool {
        BARE_SINGLE_SIG_EXTENDED_KEY_PATTERN.is_match(input.trim())
    }

    /// Convert SLIP-132 prefixes to BIP32 `xpub`/`tpub`, preserving the key's own network.
    pub fn normalize_extended_key_to_bip32(extended_key: &str) -> Result<String> {
        if extended_key.len() < 4 {
            return Ok(extended_key.to_string());
        }

        let prefix = &extended_key[..4];
        let target_version = match prefix {
            "xpub" | "ypub" | "zpub" | "Ypub" | "Zpub" => Version::Xpub,
            "tpub" | "upub" | "vpub" | "Upub" | "Vpub" => Version::Tpub,
            _ => return Ok(extended_key.to_string()),
        };

        if prefix == "xpub" || prefix == "tpub" {
            return Ok(extended_key.to_string());
        }

        convert_version(extended_key, &target_version)
            .map_err(|e| anyhow!("Failed to convert extended key: {:?}", e))
    }

    /// Rewrite SLIP-132 keys inside a descriptor so rust-miniscript can parse them.
    pub fn normalize_extended_keys_in_descriptor(descriptor: &str) -> Result<String> {
        let mut first_error: Option<anyhow::Error> = None;
        let rewritten =
            EXTENDED_PUBLIC_KEY_PATTERN.replace_all(descriptor, |caps: &regex::Captures| {
                match Self::normalize_extended_key_to_bip32(&caps[0]) {
                    Ok(normalized) => normalized,
                    Err(error) => {
                        if first_error.is_none() {
                            first_error = Some(error);
                        }
                        caps[0].to_string()
                    }
                }
            });

        match first_error {
            Some(error) => Err(error),
            None => Ok(rewritten.into_owned()),
        }
    }

    /// Generate a multipath descriptor for a given script type (without key origin)
    pub fn generate_descriptor_for_type(
        &self,
        xpub: &str,
        script_type: &ScriptType,
    ) -> Result<String> {
        // For watch-only wallets, we strip key origin to prevent duplicate wallets
        // Same XPUB with different fingerprints would create different checksums
        let normalized_xpub = self.normalize_xpub(xpub)?;

        let descriptor_without_checksum = match script_type {
            ScriptType::P2PKH => format!("pkh({}/<0;1>/*)", normalized_xpub),
            ScriptType::P2SH => format!("sh(wpkh({}/<0;1>/*))", normalized_xpub),
            ScriptType::P2WPKH => format!("wpkh({}/<0;1>/*)", normalized_xpub),
            ScriptType::P2TR => format!("tr({}/<0;1>/*)", normalized_xpub),
        };

        // Calculate checksum and append it to the descriptor
        let checksum = descriptor_checksum(&descriptor_without_checksum)?;

        let descriptor_with_checksum = format!("{}#{}", descriptor_without_checksum, checksum);

        Ok(descriptor_with_checksum)
    }

    /// Normalize different extended key formats to standard xpub format
    /// Converts ypub/zpub/tpub/upub/vpub to xpub/tpub format for consistency
    /// Uses proper base58check decoding/encoding via the xyzpub crate
    pub fn normalize_xpub(&self, extended_key: &str) -> Result<String> {
        if extended_key.len() < 4 {
            return Ok(extended_key.to_string());
        }

        let prefix = &extended_key[..4];

        // Determine target version based on network
        let target_version = match self.network {
            Network::Bitcoin => Version::Xpub,
            Network::Testnet | Network::Regtest | Network::Signet => Version::Tpub,
            _ => return Ok(extended_key.to_string()),
        };

        // Check if conversion is needed
        let needs_conversion = match (prefix, &target_version) {
            ("xpub", Version::Xpub) => false, // Already normalized for mainnet
            ("tpub", Version::Tpub) => false, // Already normalized for testnet/regtest
            (
                "ypub" | "zpub" | "Ypub" | "Zpub" | "upub" | "vpub" | "Upub" | "Vpub" | "tpub"
                | "xpub",
                _,
            ) => true,
            _ => false, // Unknown format, return as-is
        };

        if !needs_conversion {
            return Ok(extended_key.to_string());
        }

        // Use xyzpub for proper base58check conversion
        convert_version(extended_key, &target_version)
            .map_err(|e| anyhow!("Failed to convert extended key: {:?}", e))
    }

    /// Detect which network a key belongs to based on its prefix
    pub fn get_key_network(key: &str) -> Option<Network> {
        if key.len() < 4 {
            return None;
        }

        let prefix = &key[..4];
        match prefix {
            "xpub" | "ypub" | "zpub" | "Ypub" | "Zpub" => Some(Network::Bitcoin),
            "tpub" | "upub" | "vpub" | "Upub" | "Vpub" => Some(Network::Testnet), // Also covers regtest/signet
            _ => None,
        }
    }

    /// Validate that a key is compatible with the expected network
    pub fn validate_key_network(key: &str, expected_network: Network) -> Result<()> {
        if Self::get_key_network(key).is_none() {
            return Ok(()); // Not a recognized extended public key, skip validation
        }

        let detected_network = Self::get_key_network(key);

        match (detected_network, expected_network) {
            (Some(Network::Bitcoin), Network::Bitcoin) => Ok(()),
            (Some(Network::Testnet), Network::Testnet) => Ok(()),
            (Some(Network::Testnet), Network::Regtest) => Ok(()),
            (Some(Network::Testnet), Network::Signet) => Ok(()),
            (Some(detected), expected) => {
                let detected_name = match detected {
                    Network::Bitcoin => "mainnet",
                    Network::Testnet => "testnet",
                    Network::Regtest => "regtest",
                    Network::Signet => "signet",
                    _ => "unknown",
                };
                let expected_name = match expected {
                    Network::Bitcoin => "mainnet",
                    Network::Testnet => "testnet",
                    Network::Regtest => "regtest",
                    Network::Signet => "signet",
                    _ => "unknown",
                };
                Err(anyhow!(
                    "Network mismatch: key appears to be for {} but server is running on {}",
                    detected_name,
                    expected_name
                ))
            }
            (None, _) => Ok(()), // Unknown key format, skip validation
        }
    }

    /// Check if the input is a valid Bitcoin public key (compressed or uncompressed)
    pub fn is_bitcoin_public_key(input: &str) -> bool {
        PublicKey::from_str(input.trim()).is_ok()
    }

    /// Wrap a Bitcoin public key in a pk() descriptor string with checksum
    pub fn pubkey_to_descriptor(pubkey: &str) -> Result<String> {
        let pubkey_trimmed = pubkey.trim();
        // Validate the public key first
        PublicKey::from_str(pubkey_trimmed).map_err(|e| anyhow!("Invalid public key: {}", e))?;
        let descriptor_without_checksum = format!("pk({})", pubkey_trimmed);
        let checksum = descriptor_checksum(&descriptor_without_checksum)?;
        Ok(format!("{}#{}", descriptor_without_checksum, checksum))
    }

    /// Check if the input is a valid Bitcoin address (any type, any network)
    pub fn is_bitcoin_address(input: &str) -> bool {
        Address::from_str(input.trim()).is_ok()
    }

    /// Validate that a Bitcoin address is compatible with the expected network
    pub fn validate_address_network(address: &str, expected_network: Network) -> Result<()> {
        let parsed = Address::from_str(address.trim())
            .map_err(|e| anyhow!("Invalid Bitcoin address: {}", e))?;

        parsed.require_network(expected_network).map_err(|_| {
            let expected_name = match expected_network {
                Network::Bitcoin => "mainnet",
                Network::Testnet => "testnet",
                Network::Regtest => "regtest",
                Network::Signet => "signet",
                _ => "unknown",
            };
            anyhow!(
                "Address is not valid for {}. Please use a {} address.",
                expected_name,
                expected_name
            )
        })?;

        Ok(())
    }

    /// Wrap a Bitcoin address in an addr() descriptor string with checksum
    pub fn address_to_descriptor(address: &str) -> Result<String> {
        let descriptor_without_checksum = format!("addr({})", address.trim());
        let checksum = descriptor_checksum(&descriptor_without_checksum)?;
        Ok(format!("{}#{}", descriptor_without_checksum, checksum))
    }

    /// Validate that a descriptor is compatible with the expected network
    /// Extracts XPUBs from within descriptors and validates each one.
    /// Also handles raw Bitcoin address inputs.
    pub fn validate_descriptor_network(descriptor: &str, expected_network: Network) -> Result<()> {
        // Public keys are network-agnostic, no validation needed
        if Self::is_bitcoin_public_key(descriptor) {
            return Ok(());
        }

        // Check if the input is a raw Bitcoin address
        if Self::is_bitcoin_address(descriptor) {
            return Self::validate_address_network(descriptor, expected_network);
        }

        for key_match in EXTENDED_PUBLIC_KEY_PATTERN.find_iter(descriptor) {
            Self::validate_key_network(key_match.as_str(), expected_network)?;
        }

        // Also check if the entire descriptor is just a bare XPUB
        if Self::is_xpub(descriptor) {
            Self::validate_key_network(descriptor, expected_network)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Genesis block coinbase public key (uncompressed)
    const GENESIS_PUBKEY: &str = "04678afdb0fe5548271967f1a67130b7105cd6a828e03909a67962e0ea1f61deb649f6bc3f4cef38c4f35504e51ec112de5c384df7ba0b8d578a4c702b6bf11d5f";

    #[test]
    fn test_is_bitcoin_public_key_uncompressed() {
        assert!(XpubConverter::is_bitcoin_public_key(GENESIS_PUBKEY));
    }

    #[test]
    fn test_is_bitcoin_public_key_compressed() {
        // Compressed key (33 bytes / 66 hex chars starting with 02 or 03)
        assert!(XpubConverter::is_bitcoin_public_key(
            "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
        ));
    }

    #[test]
    fn test_is_bitcoin_public_key_rejects_invalid() {
        assert!(!XpubConverter::is_bitcoin_public_key("not_a_key"));
        assert!(!XpubConverter::is_bitcoin_public_key(""));
        assert!(!XpubConverter::is_bitcoin_public_key(
            "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa"
        ));
        assert!(!XpubConverter::is_bitcoin_public_key("04abcdef")); // too short
    }

    #[test]
    fn test_is_bitcoin_public_key_trims_whitespace() {
        let padded = format!("  {}  ", GENESIS_PUBKEY);
        assert!(XpubConverter::is_bitcoin_public_key(&padded));
    }

    #[test]
    fn test_pubkey_to_descriptor() {
        let result = XpubConverter::pubkey_to_descriptor(GENESIS_PUBKEY).unwrap();
        assert!(result.starts_with("pk("));
        assert!(result.contains(GENESIS_PUBKEY));
        assert!(result.contains('#')); // has checksum
    }

    #[test]
    fn test_pubkey_to_descriptor_invalid() {
        assert!(XpubConverter::pubkey_to_descriptor("not_a_key").is_err());
    }

    #[test]
    fn test_validate_descriptor_network_bypasses_pubkey() {
        // Public keys are network-agnostic, should pass on any network
        assert!(
            XpubConverter::validate_descriptor_network(GENESIS_PUBKEY, Network::Bitcoin).is_ok()
        );
        assert!(
            XpubConverter::validate_descriptor_network(GENESIS_PUBKEY, Network::Testnet).is_ok()
        );
    }

    #[test]
    fn test_is_xpub_rejects_sparrow_multisig_prefixes() {
        let xpub = "xpub6DEzNop46vmxR49zYWFnMwmEfawSNmAMf6dLH5YKDY463twtvw1XD7ihwJRLPRGZJz799VPFzXHpZu6WdhT29WnaeuChS6aZHZPFmqczR5K";
        let zpub_multisig = convert_version(xpub, &Version::ZpubMultisig).unwrap();

        assert!(XpubConverter::is_xpub(xpub));
        assert!(!XpubConverter::is_xpub(&zpub_multisig));
        assert_eq!(
            XpubConverter::get_key_network(&zpub_multisig),
            Some(Network::Bitcoin)
        );
    }

    #[test]
    fn test_validate_descriptor_network_detects_zpub_multisig() {
        let xpub = "xpub6DEzNop46vmxR49zYWFnMwmEfawSNmAMf6dLH5YKDY463twtvw1XD7ihwJRLPRGZJz799VPFzXHpZu6WdhT29WnaeuChS6aZHZPFmqczR5K";
        let zpub_multisig = convert_version(xpub, &Version::ZpubMultisig).unwrap();
        let descriptor = format!(
            "wsh(sortedmulti(2,{zpub_multisig}/<0;1>/*,{zpub_multisig}/<0;1>/*,{zpub_multisig}/<0;1>/*))"
        );

        assert!(XpubConverter::validate_descriptor_network(&descriptor, Network::Bitcoin).is_ok());
        assert!(XpubConverter::validate_descriptor_network(&descriptor, Network::Testnet).is_err());
    }

    #[test]
    fn test_normalize_extended_keys_in_descriptor_converts_vpub_multisig() {
        let tpub = "tpubDCMRAYcH71Gagskm7E5peNMYB5sKaLLwtn2c4Rb3CMUTRVUk5dkpsskhspa5MEcVZ11LwTcM7R5mzndUCG9WabYcT5hfQHbYVoaLFBZHPCi";
        let vpub_multisig = convert_version(tpub, &Version::VpubMultisig).unwrap();
        let descriptor = format!("wsh(sortedmulti(2,{vpub_multisig}/<0;1>/*))");

        let normalized = XpubConverter::normalize_extended_keys_in_descriptor(&descriptor).unwrap();

        assert!(normalized.contains(tpub));
        assert!(!normalized.contains("Vpub"));
    }
}
