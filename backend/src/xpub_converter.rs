use anyhow::{anyhow, Result};
use bdk_wallet::bitcoin::Network;
use miniscript::descriptor::checksum::desc_checksum;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Command;
use std::time::Duration;
use tokio::time::timeout;

use crate::electrum::ElectrumClient;
use bdk_electrum::electrum_client::ElectrumApi;


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
            let mut total_txs = 0;
            let mut total_balance = 0u64;
            let mut failed_checks = 0;
            let max_failed_checks = 3; // Stop early if too many failures
            
            // Check first 10 addresses for better accuracy, but stop early on repeated failures
            let addresses_to_check = addresses.iter().take(10);
            
            for (index, address) in addresses_to_check.enumerate() {
                // Add timeout for individual address checks
                let check_result = timeout(
                    Duration::from_secs(5), // 5 second timeout per address
                    self.check_address_activity(&electrum_client, address)
                ).await;
                
                match check_result {
                    Ok(Ok((has_activity, score))) => {
                        if has_activity {
                            active_count += 1;
                        }
                        total_score += score;
                        
                        // Also collect individual stats for logging
                        if score > 1000.0 {
                            total_txs += (score / 1000.0) as usize;
                        }
                        if score > 0.0 {
                            total_balance += (score % 1000.0 * 1000.0) as u64;
                        }
                        
                        // Reset failure counter on success
                        failed_checks = 0;
                    }
                    Ok(Err(e)) => {
                        failed_checks += 1;
                        eprintln!("Failed to check address {}: {} (failure {}/{})", 
                                address, e, failed_checks, max_failed_checks);
                        
                        // Stop early if we have too many consecutive failures
                        if failed_checks >= max_failed_checks {
                            eprintln!("Too many failures, stopping address checks for this script type");
                            break;
                        }
                    }
                    Err(_) => {
                        failed_checks += 1;
                        eprintln!("Timeout checking address {} (failure {}/{})", 
                                address, failed_checks, max_failed_checks);
                        
                        if failed_checks >= max_failed_checks {
                            eprintln!("Too many timeouts, stopping address checks for this script type");
                            break;
                        }
                    }
                }
                
                // Small delay between checks to avoid overwhelming Electrum server
                if index < 9 { // Don't delay after the last check
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
            
            // Log summary for this script type
            if total_score > 0.0 {
                println!("    Summary: {} active addresses, {} total txs, {} sats balance", 
                        active_count, total_txs, total_balance);
            }
            
            Ok((active_count, total_score))
        } else {
            // No Electrum client available, fallback to prefix hints
            Ok((0, 0.0))
        }
    }
    
    /// Check if an address has activity using Electrum
    /// Returns (has_activity, activity_score) where score is based on tx count and balance
    async fn check_address_activity(&self, electrum_client: &ElectrumClient, address: &str) -> Result<(bool, f32)> {
        // Parse the address to validate it's correct for the network
        let addr = address.parse::<bdk_wallet::bitcoin::Address<bdk_wallet::bitcoin::address::NetworkUnchecked>>()?
            .require_network(self.network)?;
        
        // Get the script pubkey for Electrum queries
        let script = addr.script_pubkey();
        
        // Get the raw client for direct API calls
        let raw_client = electrum_client.raw_client();
        
        // Query Electrum for transaction history
        let history = match raw_client.script_get_history(&script) {
            Ok(history) => history,
            Err(e) => {
                eprintln!("Failed to get history for address {}: {}", address, e);
                return Ok((false, 0.0));
            }
        };
        
        // Query Electrum for balance
        let balance = match raw_client.script_get_balance(&script) {
            Ok(balance) => balance,
            Err(e) => {
                eprintln!("Failed to get balance for address {}: {}", address, e);
                return Ok((false, 0.0));
            }
        };
        
        // Calculate activity score based on:
        // - Number of transactions (history length) * 1000 points each
        // - Current balance in satoshis / 1000 (so 1 BTC = 100,000 points)
        let tx_count = history.len();
        // Handle signed unconfirmed balance (can be negative)
        let confirmed_sats = balance.confirmed;
        let unconfirmed_sats = balance.unconfirmed.max(0) as u64; // Only count positive unconfirmed
        let total_balance_sats = confirmed_sats + unconfirmed_sats;
        let activity_score = (tx_count as f32 * 1000.0) + (total_balance_sats as f32 / 1000.0);
        
        // Address has activity if it has transactions OR balance
        let has_activity = tx_count > 0 || total_balance_sats > 0;
        
        if has_activity {
            println!("    Address {} has {} txs, balance: {} sats, score: {:.2}", 
                     address, tx_count, total_balance_sats, activity_score);
        }
        
        Ok((has_activity, activity_score))
    }

    /// Convert XPUB to a multipath descriptor by probing different script types
    pub async fn convert_to_descriptor(&self, xpub: &str) -> Result<ConversionResult> {
        println!("\n=== XPUB Conversion Analysis ===");
        println!("Input XPUB: {}", xpub);
        
        // Normalize the XPUB first and show the result
        let normalized_xpub = self.normalize_xpub(xpub)?;
        if normalized_xpub != xpub {
            println!("Normalized XPUB: {} -> {}", xpub, normalized_xpub);
        }
        
        // Check if we have Electrum client for blockchain checking
        let has_electrum = self.electrum_client.is_some();
        println!("Electrum client available: {}", if has_electrum { "Yes" } else { "No" });
        
        if !has_electrum {
            println!("⚠️  No Electrum client - will use prefix hints only");
        }

        // First, derive addresses for all script types
        println!("\nDeriving addresses for all script types...");
        let derived = self.derive_addresses(xpub).await
            .map_err(|e| anyhow!("Failed to derive addresses: {}", e))?;

        let mut best_type = ScriptType::P2WPKH;
        let mut best_score = 0.0;
        let mut best_active = 0;
        let mut script_type_results = Vec::new();

        println!("\n=== Blockchain Activity Analysis ===");
        
        // Score each script type based on blockchain activity
        for (type_name, type_info) in &derived.script_types {
            if let Some(ref error) = type_info.error {
                println!("❌ Script type {}: Failed to generate addresses - {}", type_name, error);
                continue;
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
                _ => {
                    println!("❓ Unknown script type: {}", type_name);
                    continue;
                }
            };

            println!("\n🔍 Checking {} ({})...", script_type.as_str(), type_name);
            println!("  Sample addresses to check:");
            for (i, addr) in addresses.iter().take(3).enumerate() {
                println!("    {}: {}", i + 1, addr);
            }
            
            match self.score_addresses(&addresses).await {
                Ok((active_count, score)) => {
                    // Prefer types with more active addresses, then higher balance/tx count
                    let combined_score = (active_count as f32 * 1000.0) + score;
                    
                    println!("  📊 Results: {} active addresses, activity score: {:.2}", 
                             active_count, score);
                    println!("  📈 Combined score: {:.2}", combined_score);
                    
                    // Store results for final summary
                    script_type_results.push((script_type.clone(), active_count, score, combined_score));
                    
                    // Handle tie-breaking: if scores are equal, prefer newer script types
                    let should_update = if combined_score > best_score {
                        println!("  ✅ New best score!");
                        true
                    } else if combined_score == best_score && combined_score > 0.0 {
                        let current_priority = self.script_type_priority(&script_type);
                        let best_priority = self.script_type_priority(&best_type);
                        if current_priority > best_priority {
                            println!("  ✅ Equal score but higher priority (tie-breaker)!");
                            true
                        } else {
                            println!("  ⚪ Equal or lower score/priority");
                            false
                        }
                    } else {
                        println!("  ⚪ Lower score than current best");
                        false
                    };
                    
                    if should_update {
                        best_score = combined_score;
                        best_type = script_type;
                        best_active = active_count;
                    }
                }
                Err(e) => {
                    println!("  ❌ Failed to analyze addresses: {}", e);
                    script_type_results.push((script_type, 0, 0.0, 0.0));
                }
            }
        }

        // Print summary of all script types
        println!("\n=== Detection Summary ===");
        script_type_results.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal));
        
        for (i, (script_type, active, _score, combined)) in script_type_results.iter().enumerate() {
            let indicator = if i == 0 && *combined > 0.0 { "🥇" } else if *combined > 0.0 { "📈" } else { "⚪" };
            println!("  {} {}: {} active, score {:.2}", 
                     indicator, script_type.as_str(), active, combined);
        }

        // If no activity found, use prefix hint as fallback
        if best_score == 0.0 {
            let prefix_hint = Self::detect_type_from_prefix(xpub);
            println!("\n⚠️  No blockchain activity detected");
            println!("🔍 Using prefix hint: {:?} -> {:?}", &xpub[0..4], prefix_hint);
            best_type = prefix_hint;
        } else {
            println!("\n✅ Activity detected! Best script type: {}", best_type.as_str());
        }

        // Generate the descriptor for the best script type
        println!("\n=== Descriptor Generation ===");
        let descriptor = self.generate_descriptor_for_type(xpub, &best_type)
            .map_err(|e| anyhow!("Failed to generate descriptor for {:?}: {}", best_type, e))?;

        let confidence = if best_score > 0.0 {
            let conf = (best_active as f32 / 10.0).min(1.0); // Max confidence with 10+ active addresses
            println!("📊 Confidence calculation: {} active addresses / 10 = {:.1}%", 
                     best_active, conf * 100.0);
            conf
        } else {
            println!("📊 Confidence: 50% (prefix hint only)");
            0.5 // Medium confidence when using prefix hint
        };

        println!("\n🎯 Final Result:");
        println!("  Script Type: {} (confidence: {:.1}%)", best_type.as_str(), confidence * 100.0);
        println!("  Descriptor: {}", descriptor);
        
        let checksum = descriptor.split('#').nth(1).unwrap_or("missing");
        println!("  Wallet ID: {}", checksum);
        
        println!("=== End XPUB Conversion ===\n");

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

    /// Get priority score for script types (higher = preferred for tie-breaking)
    fn script_type_priority(&self, script_type: &ScriptType) -> u8 {
        match script_type {
            ScriptType::P2TR => 4,    // Newest and most efficient
            ScriptType::P2WPKH => 3,  // Native SegWit, very common
            ScriptType::P2SH => 2,    // Nested SegWit, legacy compatibility
            ScriptType::P2PKH => 1,   // Legacy, oldest
        }
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