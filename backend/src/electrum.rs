use bdk_electrum::{electrum_client, BdkElectrumClient};
use bdk_wallet::PersistedWallet;
use rusqlite::Connection;
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

    pub fn sync_wallet(&self, wallet: &mut PersistedWallet<Connection>) -> Result<(), Box<dyn Error>> {
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

    pub fn sync_wallet_incremental(&self, wallet: &mut PersistedWallet<Connection>) -> Result<(), Box<dyn Error>> {

        // Populate the electrum client's transaction cache
        self.client.populate_tx_cache(wallet.tx_graph().full_txs().map(|tx_node| tx_node.tx));
        
        // Start sync request (only checks known addresses)
        let request = wallet.start_sync_with_revealed_spks();
        
        // Perform the sync
        let update = self.client.sync(request, BATCH_SIZE, false)
            .map_err(|e| format!("Sync failed: {}", e))?;
        
        // Apply the update
        wallet.apply_update(update)
            .map_err(|e| format!("Failed to apply update: {}", e))?;
        
        Ok(())
    }
}