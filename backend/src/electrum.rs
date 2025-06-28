use bdk_electrum::{electrum_client, BdkElectrumClient};
use bdk_wallet::{PersistedWallet, ChangeSet};
use bdk_wallet::file_store::Store;
use bdk_wallet::chain::collections::HashSet;
use bdk_wallet::KeychainKind;
use std::error::Error;
use std::io::{self, Write};

const STOP_GAP: usize = 20;
const BATCH_SIZE: usize = 5;

pub struct ElectrumClient {
    client: BdkElectrumClient<electrum_client::Client>,
}

impl ElectrumClient {
    pub fn new_regtest() -> Result<Self, Box<dyn Error>> {
        let client = BdkElectrumClient::new(electrum_client::Client::new("tcp://127.0.0.1:50001")?);
        Ok(ElectrumClient { client })
    }

    pub fn server_features(&self) -> Result<String, Box<dyn Error>> {
        Ok("Connected to Electrum via BDK".to_string())
    }

    pub fn sync_wallet(&self, wallet: &mut PersistedWallet<Store<ChangeSet>>) -> Result<(), Box<dyn Error>> {
        println!("Syncing with electrum...");
        
        // Print initial balance
        let balance_before = wallet.balance();
        println!("Wallet balance before syncing: {}", balance_before.total());

        println!("Revealing external addresses:");
        for (i, address) in wallet.reveal_addresses_to(KeychainKind::External, 50).enumerate() {
            println!("External  {}: {}", i, address);
        }
        
        println!("Revealing internal addresses:");
        for (i, address) in wallet.reveal_addresses_to(KeychainKind::Internal, 50).enumerate() {
            println!("Internal  {}: {}", i, address);
        }
        
        // Populate the electrum client's transaction cache
        self.client.populate_tx_cache(wallet.tx_graph().full_txs().map(|tx_node| tx_node.tx));
        
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
        let update = self.client.full_scan(request, STOP_GAP, BATCH_SIZE, false)
            .map_err(|e| format!("Full scan failed: {}", e))?;
        
        println!(); // New line after scan progress
        
        // Apply the update
        wallet.apply_update(update)
            .map_err(|e| format!("Failed to apply update: {}", e))?;
        
        // Print final balance
        let balance_after = wallet.balance();
        println!("Wallet balance after syncing: {}", balance_after.total());
        
        Ok(())
    }

    pub fn sync_wallet_incremental(&self, wallet: &mut PersistedWallet<Store<ChangeSet>>) -> Result<String, Box<dyn Error>> {

        // Populate the electrum client's transaction cache
        self.client.populate_tx_cache(wallet.tx_graph().full_txs().map(|tx_node| tx_node.tx));
        
        // Start sync request (only checks known addresses)
        let request = wallet.start_sync_with_revealed_spks();
        
        // Perform the sync
        let update = self.client.sync(request, BATCH_SIZE, false)
            .map_err(|e| format!("Sync failed: {}", e))?;
        
        // Create readable summary
        let mut summary = String::new();
        
        // TxUpdate summary
        summary.push_str(&format!("📦 TxUpdate:\n"));
        summary.push_str(&format!("  - Transactions: {}\n", update.tx_update.txs.len()));
        summary.push_str(&format!("  - Floating txouts: {}\n", update.tx_update.txouts.len()));
        summary.push_str(&format!("  - Anchors (confirmations): {}\n", update.tx_update.anchors.len()));
        summary.push_str(&format!("  - Seen in mempool: {}\n", update.tx_update.seen_ats.len()));
        summary.push_str(&format!("  - Evicted from mempool: {}\n", update.tx_update.evicted_ats.len()));
        
        // List transaction IDs if any
        if !update.tx_update.txs.is_empty() {
            summary.push_str("  📄 Transaction IDs:\n");
            for tx in &update.tx_update.txs {
                let txid = tx.compute_txid();
                summary.push_str(&format!("    - {}\n", txid));
            }
        }
        
        // List anchors if any
        if !update.tx_update.anchors.is_empty() {
            summary.push_str("  ⚓ Anchors (confirmations):\n");
            for (anchor, txid) in &update.tx_update.anchors {
                summary.push_str(&format!("    - {} at block {}\n", txid, anchor.block_id.height));
            }
        }
        
        // List seen_ats if any
        if !update.tx_update.seen_ats.is_empty() {
            summary.push_str("  👀 Seen in mempool:\n");
            for (txid, timestamp) in &update.tx_update.seen_ats {
                summary.push_str(&format!("    - {} at {}\n", txid, timestamp));
            }
        }
        
        // List evicted_ats if any
        if !update.tx_update.evicted_ats.is_empty() {
            summary.push_str("  🗑️ Evicted from mempool:\n");
            for (txid, timestamp) in &update.tx_update.evicted_ats {
                summary.push_str(&format!("    - {} at {}\n", txid, timestamp));
            }
        }
        
        // Chain update summary
        match &update.chain_update {
            Some(checkpoint) => {
                summary.push_str(&format!("\n⛓️ Chain Update:\n"));
                summary.push_str(&format!("  - Tip height: {}\n", checkpoint.height()));
                summary.push_str(&format!("  - Tip hash: {}\n", checkpoint.hash()));
            }
            None => {
                summary.push_str("\n⛓️ Chain Update: None\n");
            }
        }
        
        // Apply the update
        wallet.apply_update(update)
            .map_err(|e| format!("Failed to apply update: {}", e))?;
        
        Ok(summary)
    }
}