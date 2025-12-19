use anyhow::{anyhow, Result};
use bdk_wallet::bitcoin::Network;
use miniscript::descriptor::checksum::desc_checksum;
use xyzpub::{convert_version, Version};

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

    /// Check if the input looks like an extended public key (xpub/ypub/zpub format)
    pub fn is_xpub(input: &str) -> bool {
        // Regex for extended public keys
        let xpub_regex = regex::Regex::new(r"^[xyztuv]pub[1-9A-HJ-NP-Za-km-z]{107,108}$").unwrap();
        xpub_regex.is_match(input.trim())
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
        let checksum = desc_checksum(&descriptor_without_checksum)
            .map_err(|e| anyhow!("Failed to calculate descriptor checksum: {}", e))?;

        let descriptor_with_checksum = format!("{}#{}", descriptor_without_checksum, checksum);

        println!(
            "Generated descriptor (no key origin): {}",
            descriptor_with_checksum
        );

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
            ("ypub" | "zpub" | "upub" | "vpub" | "tpub" | "xpub", _) => true,
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
            "xpub" | "ypub" | "zpub" => Some(Network::Bitcoin),
            "tpub" | "upub" | "vpub" => Some(Network::Testnet), // Also covers regtest/signet
            _ => None,
        }
    }

    /// Validate that a key is compatible with the expected network
    pub fn validate_key_network(key: &str, expected_network: Network) -> Result<()> {
        if !Self::is_xpub(key) {
            return Ok(()); // Not an XPUB, skip validation
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

    /// Validate that a descriptor is compatible with the expected network
    /// Extracts XPUBs from within descriptors and validates each one
    pub fn validate_descriptor_network(descriptor: &str, expected_network: Network) -> Result<()> {
        // Regex to find extended public keys within descriptors
        // Matches [prefix]pub followed by base58 chars, optionally wrapped in key origin info
        let xpub_regex =
            regex::Regex::new(r"(?:\[[^\]]*\])?([xyztuv]pub[1-9A-HJ-NP-Za-km-z]+)").unwrap();

        for captures in xpub_regex.captures_iter(descriptor) {
            if let Some(key_match) = captures.get(1) {
                let key = key_match.as_str();
                Self::validate_key_network(key, expected_network)?;
            }
        }

        // Also check if the entire descriptor is just a bare XPUB
        if Self::is_xpub(descriptor) {
            Self::validate_key_network(descriptor, expected_network)?;
        }

        Ok(())
    }
}
