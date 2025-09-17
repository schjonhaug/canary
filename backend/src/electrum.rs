use anyhow::{anyhow, Result};
use bdk_electrum::electrum_client::ElectrumApi;
use bdk_electrum::{electrum_client, BdkElectrumClient};
use bdk_wallet::chain::collections::HashSet;
use bdk_wallet::rusqlite::Connection;
use bdk_wallet::{KeychainKind, PersistedWallet};
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct BlockHeader {
    pub height: u32,
    pub timestamp: u64,
}

pub const STOP_GAP: usize = 20;
pub const BATCH_SIZE: usize = 5;

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
        let mut highest_used = 0u32;

        // Get the next derivation index (this is the total number of revealed addresses)
        let next_index = wallet.next_derivation_index(keychain);

        // Check each revealed address to find the highest used one
        for index in 0..next_index {
            // Peek at the address without revealing new ones
            let address = wallet.peek_address(keychain, index);
            // Check if this address has received any funds by checking the transaction graph
            let script = address.script_pubkey();
            let is_used = wallet.transactions().any(|tx| {
                // Check if any output in any transaction is for this address
                tx.tx_node
                    .tx
                    .output
                    .iter()
                    .any(|output| output.script_pubkey == script)
            });

            if is_used {
                highest_used = index;
            }
        }

        highest_used
    }

    /// Ensure we have at least STOP_GAP addresses revealed beyond the highest used index
    fn ensure_stop_gap_maintained(
        wallet: &mut PersistedWallet<Connection>,
        keychain: KeychainKind,
    ) -> Result<bool> {
        let highest_used = Self::get_highest_used_index(wallet, keychain);
        let current_index = wallet.next_derivation_index(keychain);
        let required_index = highest_used + STOP_GAP as u32;

        if current_index <= required_index {
            let keychain_str = if keychain == KeychainKind::External {
                "external"
            } else {
                "internal"
            };
            println!(
                "  Need more {} addresses: highest used={}, current revealed={}, need={}",
                keychain_str, highest_used, current_index, required_index
            );

            // Reveal addresses up to the required index
            let revealed: Vec<_> = wallet
                .reveal_addresses_to(keychain, required_index)
                .collect();
            println!(
                "  Revealed {} new {} addresses",
                revealed.len(),
                keychain_str
            );

            Ok(true) // Addresses were revealed
        } else {
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
        // Populate the electrum client's transaction cache
        self.client
            .populate_tx_cache(wallet.tx_graph().full_txs().map(|tx_node| tx_node.tx));

        // Start sync request (only checks known addresses)
        let request = wallet.start_sync_with_revealed_spks();

        // Perform the sync directly (restored original performance)
        let update = self
            .client
            .sync(request, BATCH_SIZE, false)
            .map_err(|e| anyhow!("Sync failed: {}", e))?;

        // Apply the update
        wallet
            .apply_update(update)
            .map_err(|e| anyhow!("Failed to apply update: {}", e))?;

        // After sync, check if we need to reveal more addresses to maintain stop gap
        let ext_revealed = Self::ensure_stop_gap_maintained(wallet, KeychainKind::External)?;
        let int_revealed = Self::ensure_stop_gap_maintained(wallet, KeychainKind::Internal)?;

        // If new addresses were revealed, we need to sync them too
        if ext_revealed || int_revealed {
            println!("New addresses revealed, performing additional sync...");

            // Sync only the newly revealed addresses with timeout
            let request = wallet.start_sync_with_revealed_spks();
            let client = Arc::clone(&self.client);
            let update = timeout(Duration::from_secs(60), tokio::task::spawn_blocking(move || {
                client.sync(request, BATCH_SIZE, false)
            }))
            .await
            .map_err(|_| anyhow!("Additional sync operation timed out after 60 seconds"))?
            .map_err(|e| anyhow!("Additional sync task failed: {}", e))?
            .map_err(|e| anyhow!("Additional sync failed: {}", e))?;

            wallet
                .apply_update(update)
                .map_err(|e| anyhow!("Failed to apply additional update: {}", e))?;
        }

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
