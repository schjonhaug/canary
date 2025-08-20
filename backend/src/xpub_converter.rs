use anyhow::{anyhow, Result};
use bdk_wallet::bitcoin::Network;
use miniscript::descriptor::checksum::desc_checksum;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Command;
use std::time::Duration;
use tokio::time::timeout;

use crate::electrum::ElectrumClient;


#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct AddressInfo {
    pub path: String,
    pub address: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScriptTypeInfo {
    pub descriptor_template: String,
    pub receiving_addresses: Vec<AddressInfo>,
    pub change_addresses: Vec<AddressInfo>,
    pub all_addresses: Vec<AddressInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DerivedAddresses {
    pub xpub: String,
    pub network: String,
    pub script_types: HashMap<String, ScriptTypeInfo>,
}

pub struct ConversionResult {
    pub descriptor: String,
    pub detected_type: ScriptType,
    pub confidence: f32,
}

pub struct XpubConverter {
    network: Network,
    electrum_client: Option<ElectrumClient>,
}

impl XpubConverter {
    pub fn new(network: Network, electrum_client: Option<&ElectrumClient>) -> Self {
        Self {
            network,
            electrum_client: electrum_client.cloned(),
        }
    }

    /// Check if the input looks like an extended public key (xpub/ypub/zpub format)
    pub fn is_xpub(input: &str) -> bool {
        // Regex for extended public keys
        let xpub_regex = regex::Regex::new(r"^[xyztuv]pub[1-9A-HJ-NP-Za-km-z]{107,108}$").unwrap();
        xpub_regex.is_match(input.trim())
    }

    /// Detect the likely script type based on the xpub prefix
    pub fn detect_type_from_prefix(xpub: &str) -> ScriptType {
        let prefix = &xpub[0..4];
        match prefix {
            "xpub" | "tpub" => ScriptType::P2WPKH, // Default to modern SegWit for xpub
            "ypub" | "upub" => ScriptType::P2SH,   // Nested SegWit
            "zpub" | "vpub" => ScriptType::P2WPKH, // Native SegWit
            _ => ScriptType::P2WPKH, // Default fallback
        }
    }

    /// Call the Node.js script to derive addresses for all script types
    async fn derive_addresses(&self, xpub: &str) -> Result<DerivedAddresses> {
        let network_str = match self.network {
            Network::Bitcoin => "mainnet",
            Network::Testnet | Network::Signet | Network::Regtest => "testnet",
            _ => "testnet", // Default fallback for other networks
        };

        let script_path = std::env::current_dir()?
            .join("xpub-tools")
            .join("scripts")
            .join("xpub_converter.js");

        if !script_path.exists() {
            return Err(anyhow!("Node.js converter script not found at: {}", script_path.display()));
        }

        // Run the Node.js script with a timeout
        let result = timeout(
            Duration::from_secs(30), // 30 second timeout
            tokio::task::spawn_blocking({
                let xpub = xpub.to_string();
                let network_str = network_str.to_string();
                let script_path = script_path.clone();
                move || {
                    Command::new("node")
                        .arg(script_path)
                        .arg(&xpub)
                        .arg(&network_str)
                        .arg("5") // Generate 5 addresses per type
                        .output()
                }
            })
        ).await??;

        let output = result?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Node.js script failed: {}", stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let derived: DerivedAddresses = serde_json::from_str(&stdout)
            .map_err(|e| anyhow!("Failed to parse Node.js output: {}", e))?;

        Ok(derived)
    }

    /// Score addresses based on Electrum activity (transactions, balance)
    /// Returns (active_addresses_count, total_activity_score)
    async fn score_addresses(&self, addresses: &[String]) -> Result<(usize, f32)> {
        if let Some(ref electrum_client) = self.electrum_client {
            let mut active_count = 0;
            let mut total_score = 0.0;
            
            println!("Script type {}: {} active addresses, score: {:.8}", 
                     "P2WPKH", active_count, total_score); // This will be updated by caller
            
            for address in addresses.iter().take(5) { // Only check first 5 addresses for performance
                match self.check_address_activity(&electrum_client, address).await {
                    Ok((has_activity, score)) => {
                        if has_activity {
                            active_count += 1;
                        }
                        total_score += score;
                    }
                    Err(e) => {
                        eprintln!("Failed to check address {}: {}", address, e);
                        // Continue checking other addresses
                    }
                }
            }
            
            Ok((active_count, total_score))
        } else {
            // No Electrum client available, fallback to prefix hints
            Ok((0, 0.0))
        }
    }
    
    /// Check if an address has activity using Electrum
    /// This is a simplified implementation that validates addresses but doesn't check activity
    async fn check_address_activity(&self, _electrum_client: &ElectrumClient, address: &str) -> Result<(bool, f32)> {
        // Parse the address to validate it's correct for the network
        let _addr = address.parse::<bdk_wallet::bitcoin::Address<bdk_wallet::bitcoin::address::NetworkUnchecked>>()?
            .require_network(self.network)?;
        
        // TODO: In a full implementation, we would:
        // 1. Calculate the script hash from the address
        // 2. Query Electrum for script history: electrum_client.script_get_history()  
        // 3. Query Electrum for balance: electrum_client.script_get_balance()
        // 4. Calculate activity score based on transaction count and balance
        //
        // For now, this is a placeholder that validates addresses but doesn't check activity
        // This maintains the fallback to prefix-based detection while providing the infrastructure
        
        Ok((false, 0.0)) // No activity detected (placeholder)
    }

    /// Convert XPUB to a multipath descriptor by probing different script types
    pub async fn convert_to_descriptor(&self, xpub: &str) -> Result<ConversionResult> {
        println!("Converting XPUB to descriptor: {}", xpub);

        // First, derive addresses for all script types
        let derived = self.derive_addresses(xpub).await?;

        let mut best_type = ScriptType::P2WPKH;
        let mut best_score = 0.0;
        let mut best_active = 0;

        // Score each script type based on blockchain activity
        for (type_name, type_info) in &derived.script_types {
            if type_info.error.is_some() {
                continue; // Skip types that failed to generate
            }

            let addresses: Vec<String> = type_info.all_addresses
                .iter()
                .map(|addr| addr.address.clone())
                .collect();

            let script_type = match type_name.as_str() {
                "p2pkh" => ScriptType::P2PKH,
                "p2sh" => ScriptType::P2SH,
                "p2wpkh" => ScriptType::P2WPKH,
                "p2tr" => ScriptType::P2TR,
                _ => continue,
            };

            match self.score_addresses(&addresses).await {
                Ok((active_count, score)) => {
                    println!("Script type {:?}: {} active addresses, score: {:.8}", 
                             script_type, active_count, score);

                    // Prefer types with more active addresses, then higher balance/tx count
                    let combined_score = (active_count as f32 * 1000.0) + score;
                    
                    if combined_score > best_score {
                        best_score = combined_score;
                        best_type = script_type;
                        best_active = active_count;
                    }
                }
                Err(e) => {
                    eprintln!("Failed to score addresses for {:?}: {}", script_type, e);
                }
            }
        }

        // If no activity found, use prefix hint as fallback
        if best_score == 0.0 {
            best_type = Self::detect_type_from_prefix(xpub);
            println!("No activity detected, using prefix hint: {:?}", best_type);
        }

        // Generate the descriptor for the best script type
        let descriptor = self.generate_descriptor_for_type(xpub, &best_type)?;

        let confidence = if best_score > 0.0 {
            (best_active as f32 / 10.0).min(1.0) // Max confidence with 10+ active addresses
        } else {
            0.5 // Medium confidence when using prefix hint
        };

        println!("Selected script type: {:?} (confidence: {:.1}%)", 
                 best_type, confidence * 100.0);

        Ok(ConversionResult {
            descriptor,
            detected_type: best_type,
            confidence,
        })
    }

    /// Generate a multipath descriptor for a given script type (without key origin)
    fn generate_descriptor_for_type(&self, xpub: &str, script_type: &ScriptType) -> Result<String> {
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