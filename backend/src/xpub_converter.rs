use anyhow::{anyhow, Result};
use bdk_wallet::bitcoin::Network;
use miniscript::descriptor::checksum::desc_checksum;


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScriptType {
    P2PKH,     // Legacy
    P2SH,      // Nested SegWit
    P2WPKH,    // Native SegWit
    P2TR,      // Taproot
}

impl ScriptType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ScriptType::P2PKH => "Legacy (P2PKH)",
            ScriptType::P2SH => "Nested SegWit (P2SH-P2WPKH)",
            ScriptType::P2WPKH => "Native SegWit (P2WPKH)",
            ScriptType::P2TR => "Taproot (P2TR)",
        }
    }
}

pub struct ConversionResult {
    pub descriptor: String,
    pub detected_type: ScriptType,
    #[allow(dead_code)]
    pub confidence: f32,
}

pub struct XpubConverter {
    network: Network,
}

impl XpubConverter {
    pub fn new(network: Network, _electrum_client: Option<&crate::electrum::ElectrumClient>) -> Self {
        Self {
            network,
        }
    }

    /// Check if the input looks like an extended public key (xpub/ypub/zpub format)
    pub fn is_xpub(input: &str) -> bool {
        // Regex for extended public keys
        let xpub_regex = regex::Regex::new(r"^[xyztuv]pub[1-9A-HJ-NP-Za-km-z]{107,108}$").unwrap();
        xpub_regex.is_match(input.trim())
    }



    

    /// Convert XPUB to a multipath descriptor with forced script type (for fresh wallets)
    pub fn convert_to_descriptor_with_forced_type(&self, xpub: &str, script_type: &str) -> Result<ConversionResult> {
        println!("\n=== XPUB Conversion (Forced Script Type) ===");
        println!("Input XPUB: {}", xpub);
        println!("Forced script type: {}", script_type);
        
        // Normalize the XPUB
        let normalized_xpub = self.normalize_xpub(xpub)?;
        if normalized_xpub != xpub {
            println!("Normalized XPUB: {} -> {}", xpub, normalized_xpub);
        }
        
        // Parse the forced script type
        let forced_type = match script_type.to_lowercase().as_str() {
            "p2pkh" | "legacy" => ScriptType::P2PKH,
            "p2sh" | "nested_segwit" | "nested-segwit" => ScriptType::P2SH,
            "p2wpkh" | "native_segwit" | "native-segwit" => ScriptType::P2WPKH,
            "p2tr" | "taproot" => ScriptType::P2TR,
            _ => return Err(anyhow!("Invalid script type: {}. Valid options: p2pkh, p2sh, p2wpkh, p2tr", script_type)),
        };
        
        println!("Parsed script type: {}", forced_type.as_str());
        
        // Generate descriptor for the forced type
        let descriptor = self.generate_descriptor_for_type(xpub, &forced_type)
            .map_err(|e| anyhow!("Failed to generate descriptor for {:?}: {}", forced_type, e))?;
        
        println!("Generated descriptor: {}", descriptor);
        println!("=== End XPUB Conversion (Forced) ===\n");
        
        Ok(ConversionResult {
            descriptor,
            detected_type: forced_type,
            confidence: 1.0, // 100% confidence since user provided the type
        })
    }


    /// Generate a multipath descriptor for a given script type (without key origin)
    pub fn generate_descriptor_for_type(&self, xpub: &str, script_type: &ScriptType) -> Result<String> {
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
        
        println!("Generated descriptor (no key origin): {}", descriptor_with_checksum);
        
        Ok(descriptor_with_checksum)
    }


    /// Normalize different extended key formats to standard xpub format
    /// Converts ypub/zpub/tpub/upub/vpub to xpub/tpub format for consistency
    pub fn normalize_xpub(&self, extended_key: &str) -> Result<String> {
        if extended_key.len() < 4 {
            return Ok(extended_key.to_string());
        }
        
        let prefix = &extended_key[..4];
        let rest = &extended_key[4..];
        
        match self.network {
            Network::Bitcoin => {
                match prefix {
                    "xpub" => Ok(extended_key.to_string()), // Already normalized
                    "ypub" => Ok(format!("xpub{}", rest)), // Convert ypub to xpub
                    "zpub" => Ok(format!("xpub{}", rest)), // Convert zpub to xpub
                    _ => Ok(extended_key.to_string()), // Return as-is for unknown formats
                }
            }
            Network::Testnet => {
                match prefix {
                    "tpub" => Ok(extended_key.to_string()), // Already normalized
                    "upub" => Ok(format!("tpub{}", rest)), // Convert upub to tpub
                    "vpub" => Ok(format!("tpub{}", rest)), // Convert vpub to tpub
                    "xpub" => Ok(format!("tpub{}", rest)), // Convert mainnet xpub to testnet
                    "ypub" => Ok(format!("tpub{}", rest)), // Convert mainnet ypub to testnet
                    "zpub" => Ok(format!("tpub{}", rest)), // Convert mainnet zpub to testnet
                    _ => Ok(extended_key.to_string()), // Return as-is for unknown formats
                }
            }
            Network::Regtest => {
                // For regtest, use testnet format
                match prefix {
                    "tpub" => Ok(extended_key.to_string()), // Already correct format
                    "upub" => Ok(format!("tpub{}", rest)),
                    "vpub" => Ok(format!("tpub{}", rest)),
                    "xpub" => Ok(format!("tpub{}", rest)),
                    "ypub" => Ok(format!("tpub{}", rest)),
                    "zpub" => Ok(format!("tpub{}", rest)),
                    _ => Ok(extended_key.to_string()),
                }
            }
            Network::Signet => {
                // For signet, use testnet format
                match prefix {
                    "tpub" => Ok(extended_key.to_string()),
                    "upub" => Ok(format!("tpub{}", rest)),
                    "vpub" => Ok(format!("tpub{}", rest)),
                    "xpub" => Ok(format!("tpub{}", rest)),
                    "ypub" => Ok(format!("tpub{}", rest)),
                    "zpub" => Ok(format!("tpub{}", rest)),
                    _ => Ok(extended_key.to_string()),
                }
            }
            _ => {
                // For unknown networks, return as-is
                Ok(extended_key.to_string())
            }
        }
    }
}