use anyhow::{Result, anyhow};
use bdk_electrum::{BdkElectrumClient, electrum_client};
use bdk_electrum::electrum_client::ElectrumApi;
use bdk_wallet::KeychainKind;
use bdk_wallet::PersistedWallet;
use bdk_wallet::chain::collections::HashSet;
use bdk_wallet::rusqlite::Connection;
use std::io::{self, Write};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct BlockHeader {
    pub height: u32,
    pub timestamp: u64,
}

pub const STOP_GAP: usize = 20;
pub const BATCH_SIZE: usize = 5;

pub struct ElectrumClient {
    pub client: BdkElectrumClient<electrum_client::Client>,
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
        Ok(ElectrumClient { client })
    }

    pub fn server_features(&self) -> Result<String> {
        Ok("Connected to Electrum via BDK".to_string())
    }

    pub fn sync_wallet(&self, wallet: &mut PersistedWallet<Connection>) -> Result<()> {
        println!("Syncing with electrum...");

        // Print initial balance
        let balance_before = wallet.balance();
        println!("Wallet balance before syncing: {}", balance_before.total());

        println!("Revealing external addresses:");
        for (i, address) in wallet
            .reveal_addresses_to(KeychainKind::External, 50)
            .enumerate()
        {
            println!("External  {}: {}", i, address);
        }

        println!("Revealing internal addresses:");
        for (i, address) in wallet
            .reveal_addresses_to(KeychainKind::Internal, 50)
            .enumerate()
        {
            println!("Internal  {}: {}", i, address);
        }

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

        // Print final balance
        let balance_after = wallet.balance();
        println!("Wallet balance after syncing: {}", balance_after.total());

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

        Ok(())
    }

    pub fn block_headers_subscribe(&self) -> Result<electrum_client::HeaderNotification> {
        self.client.inner.block_headers_subscribe()
            .map_err(|e| anyhow!("Failed to subscribe to block headers: {}", e))
    }

    pub fn block_headers_pop(&self) -> Option<electrum_client::HeaderNotification> {
        self.client.inner.block_headers_pop().ok().flatten()
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
