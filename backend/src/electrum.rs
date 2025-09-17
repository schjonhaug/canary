use anyhow::{anyhow, Result};
use bdk_electrum::electrum_client::ElectrumApi;
use bdk_electrum::{electrum_client, BdkElectrumClient};
use bdk_wallet::chain::collections::HashSet;
use bdk_wallet::rusqlite::Connection;
use bdk_wallet::{KeychainKind, PersistedWallet};
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::timeout;

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct BlockHeader {
    pub height: u32,
    pub timestamp: u64,
}

pub const STOP_GAP: usize = 20;
pub const BATCH_SIZE: usize = 20;

#[derive(Clone)]
pub struct ElectrumClient {
    pub client: Arc<BdkElectrumClient<electrum_client::Client>>,
    #[allow(dead_code)]
    pub raw_client: Arc<electrum_client::Client>,
}

impl ElectrumClient {
    pub fn new(url: &str) -> Result<Self> {
        // electrum_client::Client::new() handles both tcp:// and ssl:// schemes automatically
        if !url.starts_with("tcp://") && !url.starts_with("ssl://") {
            return Err(anyhow!(
                "Unsupported Electrum URL scheme. Use 'tcp://' or 'ssl://'"
            ));
        }

        // Create two separate clients - one for BDK, one for raw API access
        let bdk_electrum_client = electrum_client::Client::new(url)?;
        let raw_electrum_client = electrum_client::Client::new(url)?;
        let client = BdkElectrumClient::new(bdk_electrum_client);
        Ok(ElectrumClient {
            client: Arc::new(client),
            raw_client: Arc::new(raw_electrum_client),
        })
    }

    /// Get access to the raw Electrum client for direct API calls
    /// This allows calling methods like script_get_history() and script_get_balance()
    #[allow(dead_code)]
    pub fn raw_client(&self) -> &electrum_client::Client {
        &self.raw_client
    }

    /// Get the highest address index that has been used (has transactions)
    fn get_highest_used_index(wallet: &PersistedWallet<Connection>, keychain: KeychainKind) -> u32 {
        use std::cmp::max;

        let scan_start = Instant::now();
        let mut highest_used: u32 = 0;
        let mut found_usage = false;
        let mut tx_scanned = 0usize;
        let mut outputs_scanned = 0usize;

        for tx_item in wallet.transactions() {
            tx_scanned += 1;
            let tx = &tx_item.tx_node.tx;

            for output in &tx.output {
                outputs_scanned += 1;
                if let Some((output_keychain, index)) =
                    wallet.derivation_of_spk(output.script_pubkey.clone())
                {
                    if output_keychain == keychain {
                        highest_used = max(highest_used, index);
                        found_usage = true;
                    }
                }
            }
        }

        println!(
            "  [stop-gap {:?}] usage scan inspected {} txs / {} outputs in {:.2?}; highest_used={}",
            keychain,
            tx_scanned,
            outputs_scanned,
            scan_start.elapsed(),
            highest_used
        );

        if found_usage {
            highest_used
        } else {
            // No transactions found for this keychain yet, default to 0
            0
        }
    }

    /// Ensure we have at least STOP_GAP addresses revealed beyond the highest used index
    fn ensure_stop_gap_maintained(
        wallet: &mut PersistedWallet<Connection>,
        keychain: KeychainKind,
    ) -> Result<bool> {
        let calc_start = Instant::now();
        let highest_used = Self::get_highest_used_index(wallet, keychain);
        let usage_scan_duration = calc_start.elapsed();
        let current_index = wallet.next_derivation_index(keychain);
        let required_index = highest_used + STOP_GAP as u32;

        if current_index <= required_index {
            let keychain_str = if keychain == KeychainKind::External {
                "external"
            } else {
                "internal"
            };
            println!(
                "  Need more {} addresses: highest used={}, current revealed={}, need={} (scan {:.2?})",
                keychain_str, highest_used, current_index, required_index, usage_scan_duration
            );

            // Reveal addresses up to the required index
            let reveal_start = Instant::now();
            let revealed: Vec<_> = wallet
                .reveal_addresses_to(keychain, required_index)
                .collect();
            let reveal_duration = reveal_start.elapsed();
            println!(
                "  Revealed {} new {} addresses in {:.2?}",
                revealed.len(),
                keychain_str,
                reveal_duration
            );

            Ok(true) // Addresses were revealed
        } else {
            let keychain_str = if keychain == KeychainKind::External {
                "external"
            } else {
                "internal"
            };
            println!(
                "  Stop gap already satisfied for {} keychain (highest={}, current={}, required={}) after {:.2?}",
                keychain_str,
                highest_used,
                current_index,
                required_index,
                usage_scan_duration
            );
            Ok(false) // No new addresses needed
        }
    }

    pub async fn full_scan_wallet(
        &self,
        wallet: &mut PersistedWallet<Connection>,
        custom_stop_gap: Option<usize>,
    ) -> Result<()> {
        println!("Full scanning with electrum...");

        // Print initial balance
        let balance_before = wallet.balance();
        println!("Wallet balance before syncing: {}", balance_before.total());

        // Initial reveal of addresses (start with a smaller batch)
        const INITIAL_REVEAL: u32 = 50;

        println!("Initial address revelation:");
        let ext_revealed: Vec<_> = wallet
            .reveal_addresses_to(KeychainKind::External, INITIAL_REVEAL)
            .collect();
        println!("  Revealed {} external addresses", ext_revealed.len());

        let int_revealed: Vec<_> = wallet
            .reveal_addresses_to(KeychainKind::Internal, INITIAL_REVEAL)
            .collect();
        println!("  Revealed {} internal addresses", int_revealed.len());

        // Loop until we've satisfied the stop gap for both keychains
        let mut scan_iteration = 0;
        loop {
            scan_iteration += 1;
            println!("\nScan iteration {}", scan_iteration);

            // Populate the electrum client's transaction cache
            self.client
                .populate_tx_cache(wallet.tx_graph().full_txs().map(|tx_node| tx_node.tx));

            // Start full scan with progress indicator
            let request = wallet.start_full_scan().inspect({
                let mut stdout = io::stdout();
                let mut once = HashSet::<KeychainKind>::new();
                move |k, spk_i, _| {
                    if once.insert(k) {
                        print!("\nScanning keychain [{:?}]", k);
                    }
                    print!(" {:<3}", spk_i);
                    stdout.flush().expect("must flush");
                }
            });

            // Perform the full scan with timeout protection (120 seconds for more intensive operation)
            let stop_gap = custom_stop_gap.unwrap_or(STOP_GAP);
            println!("Using stop gap: {}", stop_gap);
            let client = Arc::clone(&self.client);
            let update = timeout(Duration::from_secs(120), tokio::task::spawn_blocking(move || {
                client.full_scan(request, stop_gap, BATCH_SIZE, false)
            }))
            .await
            .map_err(|_| anyhow!("Full scan operation timed out after 120 seconds"))?
            .map_err(|e| anyhow!("Full scan task failed: {}", e))?
            .map_err(|e| anyhow!("Full scan failed: {}", e))?;

            println!(); // New line after scan progress

            // Apply the update
            wallet
                .apply_update(update)
                .map_err(|e| anyhow!("Failed to apply update: {}", e))?;

            // Check if we need to reveal more addresses to satisfy the stop gap
            let ext_needs_more = Self::ensure_stop_gap_maintained(wallet, KeychainKind::External)?;
            let int_needs_more = Self::ensure_stop_gap_maintained(wallet, KeychainKind::Internal)?;

            // If no new addresses were revealed, we're done
            if !ext_needs_more && !int_needs_more {
                println!("Stop gap satisfied for both keychains");
                break;
            }

            // Safety check to prevent infinite loops
            if scan_iteration > 10 {
                println!("Warning: Reached maximum scan iterations");
                break;
            }
        }

        // Print final balance
        let balance_after = wallet.balance();
        println!("Wallet balance after syncing: {}", balance_after.total());

        // Print final address statistics
        let ext_total = wallet.next_derivation_index(KeychainKind::External);
        let int_total = wallet.next_derivation_index(KeychainKind::Internal);
        println!(
            "Total addresses revealed - External: {}, Internal: {}",
            ext_total, int_total
        );

        Ok(())
    }

    pub async fn sync_wallet(&self, wallet: &mut PersistedWallet<Connection>) -> Result<()> {
        let total_start = Instant::now();

        // Populate the electrum client's transaction cache
        let cache_start = Instant::now();
        self.client
            .populate_tx_cache(wallet.tx_graph().full_txs().map(|tx_node| tx_node.tx));
        println!(
            "  [electrum] populate_tx_cache completed in {:.2?}",
            cache_start.elapsed()
        );

        // Start sync request (only checks known addresses)
        let request = wallet.start_sync_with_revealed_spks();

        // Perform the sync directly (restored original performance)
        let electrum_sync_start = Instant::now();
        let update = self
            .client
            .sync(request, BATCH_SIZE, false)
            .map_err(|e| anyhow!("Sync failed: {}", e))?;
        println!(
            "  [electrum] primary sync completed in {:.2?}",
            electrum_sync_start.elapsed()
        );

        // Apply the update
        let apply_start = Instant::now();
        wallet
            .apply_update(update)
            .map_err(|e| anyhow!("Failed to apply update: {}", e))?;
        println!(
            "  [electrum] apply_update completed in {:.2?}",
            apply_start.elapsed()
        );

        // After sync, check if we need to reveal more addresses to maintain stop gap
        let stop_gap_external = Self::ensure_stop_gap_maintained(wallet, KeychainKind::External)?;
        let stop_gap_internal = Self::ensure_stop_gap_maintained(wallet, KeychainKind::Internal)?;

        // If new addresses were revealed, we need to sync them too
        if stop_gap_external || stop_gap_internal {
            println!("New addresses revealed, performing additional sync...");

            // Sync only the newly revealed addresses (direct sync for better performance)
            let request = wallet.start_sync_with_revealed_spks();
            let additional_sync_start = Instant::now();
            let update = self
                .client
                .sync(request, BATCH_SIZE, false)
                .map_err(|e| anyhow!("Additional sync failed: {}", e))?;
            println!(
                "  [electrum] additional sync completed in {:.2?}",
                additional_sync_start.elapsed()
            );

            let additional_apply_start = Instant::now();
            wallet
                .apply_update(update)
                .map_err(|e| anyhow!("Failed to apply additional update: {}", e))?;
            println!(
                "  [electrum] additional apply_update completed in {:.2?}",
                additional_apply_start.elapsed()
            );
        }

        println!(
            "  [electrum] total Electrum client sync duration {:.2?}",
            total_start.elapsed()
        );

        Ok(())
    }

    pub async fn get_block_header(&self, height: u32) -> Result<BlockHeader> {
        let client = Arc::clone(&self.client);
        let header = timeout(Duration::from_secs(10), tokio::task::spawn_blocking(move || {
            client.inner.block_header(height as usize)
        }))
        .await
        .map_err(|_| anyhow!("Get block header operation timed out after 10 seconds"))?
        .map_err(|e| anyhow!("Get block header task failed: {}", e))?
        .map_err(|e| anyhow!("Failed to get block header for height {}: {}", height, e))?;

        Ok(BlockHeader {
            height,
            timestamp: header.time as u64,
        })
    }

    pub async fn get_current_block_height(&self) -> Result<u32> {
        let client = Arc::clone(&self.client);
        let height = timeout(Duration::from_secs(10), tokio::task::spawn_blocking(move || {
            client.inner.block_headers_subscribe()
        }))
        .await
        .map_err(|_| anyhow!("Get current block height operation timed out after 10 seconds"))?
        .map_err(|e| anyhow!("Get current block height task failed: {}", e))?
        .map_err(|e| anyhow!("Failed to get current block height: {}", e))?
        .height;
        Ok(height as u32)
    }
}
