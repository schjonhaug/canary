use anyhow::{Result, anyhow};
use bdk_electrum::{BdkElectrumClient, electrum_client};
use bdk_electrum::electrum_client::ElectrumApi;
use bdk_wallet::{KeychainKind, PersistedWallet};
use bdk_wallet::chain::collections::HashSet;
use bdk_wallet::rusqlite::Connection;
use std::io::{self, Write};
use std::sync::Arc;
use serde::{Deserialize, Serialize};

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
}

impl ElectrumClient {
    pub fn new(url: &str) -> Result<Self> {
        // electrum_client::Client::new() handles both tcp:// and ssl:// schemes automatically
        if !url.starts_with("tcp://") && !url.starts_with("ssl://") {
            return Err(anyhow!(
                "Unsupported Electrum URL scheme. Use 'tcp://' or 'ssl://'"
            ));
        }

        let electrum_client = electrum_client::Client::new(url)?;
        let client = BdkElectrumClient::new(electrum_client);
        Ok(ElectrumClient { client: Arc::new(client) })
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
                tx.tx_node.tx.output.iter().any(|output| {
                    output.script_pubkey == script
                })
            });
            
            if is_used {
                highest_used = index;
            }
        }
        
        highest_used
    }

    /// Ensure we have at least STOP_GAP addresses revealed beyond the highest used index
    fn ensure_stop_gap_maintained(wallet: &mut PersistedWallet<Connection>, keychain: KeychainKind) -> Result<bool> {
        let highest_used = Self::get_highest_used_index(wallet, keychain);
        let current_index = wallet.next_derivation_index(keychain);
        let required_index = highest_used + STOP_GAP as u32;
        
        if current_index <= required_index {
            let keychain_str = if keychain == KeychainKind::External { "external" } else { "internal" };
            println!("  Need more {} addresses: highest used={}, current revealed={}, need={}",
                keychain_str, highest_used, current_index, required_index);
            
            // Reveal addresses up to the required index
            let revealed: Vec<_> = wallet.reveal_addresses_to(keychain, required_index).collect();
            println!("  Revealed {} new {} addresses", revealed.len(), keychain_str);
            
            Ok(true) // Addresses were revealed
        } else {
            Ok(false) // No new addresses needed
        }
    }


    pub fn sync_wallet(&self, wallet: &mut PersistedWallet<Connection>) -> Result<()> {
        println!("Syncing with electrum...");

        // Print initial balance
        let balance_before = wallet.balance();
        println!("Wallet balance before syncing: {}", balance_before.total());

        // Initial reveal of addresses (start with a smaller batch)
        const INITIAL_REVEAL: u32 = 50;
        
        println!("Initial address revelation:");
        let ext_revealed: Vec<_> = wallet.reveal_addresses_to(KeychainKind::External, INITIAL_REVEAL).collect();
        println!("  Revealed {} external addresses", ext_revealed.len());
        
        let int_revealed: Vec<_> = wallet.reveal_addresses_to(KeychainKind::Internal, INITIAL_REVEAL).collect();
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

            // Perform the full scan
            let update = self
                .client
                .full_scan(request, STOP_GAP, BATCH_SIZE, false)
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
        println!("Total addresses revealed - External: {}, Internal: {}", ext_total, int_total);

        Ok(())
    }

    pub fn sync_wallet_incremental(&self, wallet: &mut PersistedWallet<Connection>) -> Result<()> {
        // Populate the electrum client's transaction cache
        self.client
            .populate_tx_cache(wallet.tx_graph().full_txs().map(|tx_node| tx_node.tx));

        // Start sync request (only checks known addresses)
        let request = wallet.start_sync_with_revealed_spks();

        // Perform the sync
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
            
            // Sync only the newly revealed addresses
            let request = wallet.start_sync_with_revealed_spks();
            let update = self
                .client
                .sync(request, BATCH_SIZE, false)
                .map_err(|e| anyhow!("Additional sync failed: {}", e))?;
                
            wallet
                .apply_update(update)
                .map_err(|e| anyhow!("Failed to apply additional update: {}", e))?;
        }

        Ok(())
    }


    pub fn get_block_header(&self, height: u32) -> Result<BlockHeader> {
        let header = self.client.inner.block_header(height as usize)
            .map_err(|e| anyhow!("Failed to get block header for height {}: {}", height, e))?;
        
        Ok(BlockHeader {
            height,
            timestamp: header.time as u64,
        })
    }

    pub fn get_current_block_height(&self) -> Result<u32> {
        let height = self.client.inner.block_headers_subscribe()
            .map_err(|e| anyhow!("Failed to get current block height: {}", e))?
            .height;
        Ok(height as u32)
    }
}
