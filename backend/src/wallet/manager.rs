//! [`WalletManager`]: the in-memory BDK wallets, the Electrum connection and
//! the shared state the sync schedules work from.

use super::WalletMap;
use crate::config::AppConfig;
use crate::electrum::{ElectrumClient, ElectrumClientManager};
use crate::metadata::{MetadataDb, TransactionNotification, WalletType};
use anyhow::{anyhow, Result};
use bdk_wallet::rusqlite::Connection;
use bdk_wallet::{bitcoin::Network, PersistedWallet};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, Mutex, OwnedSemaphorePermit, Semaphore};
use tracing::{debug, info, warn};

#[cfg(unix)]
use super::fs::create_private_dir;

pub struct WalletManager {
    // Thread-safe HashMap for in-memory wallet storage
    // Each wallet has its own mutex for parallel access
    pub wallets: WalletMap,
    pub wallet_dir: PathBuf,
    /// Electrum client manager with automatic reconnection support
    pub electrum_client_manager: Option<Arc<ElectrumClientManager>>,
    pub metadata_db: MetadataDb,
    pub notification_sender: broadcast::Sender<TransactionNotification>,
    pub(super) network: Network,
    pub(super) config: AppConfig,
    /// Fair one-at-a-time gate for script-heavy work against a self-hosted endpoint.
    pub(super) electrum_work_gate: Option<Arc<Semaphore>>,
}

impl WalletManager {
    pub async fn new(
        notification_sender: broadcast::Sender<TransactionNotification>,
        wallet_dir: PathBuf,
        metadata_db_path: &str,
        network: Network,
        electrum_url: &str,
        config: &AppConfig,
    ) -> Self {
        #[cfg(unix)]
        create_private_dir(&wallet_dir)
            .unwrap_or_else(|e| panic!("Failed to create wallet directory: {e}"));
        #[cfg(not(unix))]
        std::fs::create_dir_all(&wallet_dir)
            .unwrap_or_else(|e| panic!("Failed to create wallet directory: {e}"));

        // Initialize electrum client manager with automatic reconnection support
        let electrum_client_manager = {
            let manager = Arc::new(ElectrumClientManager::new_with_subscriptions(
                electrum_url,
                config.is_self_hosted_mode(),
            ));
            if manager.is_connected().await {
                info!("Connected to Electrum server: {}", electrum_url);
            } else {
                warn!(
                    "ElectrumClientManager created but initial connection to {} failed",
                    electrum_url
                );
                info!("Will attempt reconnection on first sync");
            }
            Some(manager)
        };

        // Initialize metadata database
        let metadata_db = match MetadataDb::new(metadata_db_path, config).await {
            Ok(db) => db,
            Err(e) => {
                warn!("Failed to create metadata database: {}", e);
                panic!("Cannot create WalletManager without metadata database");
            }
        };

        // Initialize thread-safe wallet storage
        let wallets = Arc::new(Mutex::new(HashMap::new()));

        let mut manager = WalletManager {
            wallets: wallets.clone(),
            wallet_dir: wallet_dir.clone(),
            electrum_client_manager,
            metadata_db,
            notification_sender,
            network,
            config: config.clone(),
            electrum_work_gate: config
                .is_self_hosted_mode()
                .then(|| Arc::new(Semaphore::new(1))),
        };

        // Load active wallets on startup (only wallets with active subscriptions)
        info!("Loading active wallets into memory...");
        let load_start = Instant::now();

        // Get wallets with active subscriptions from the database
        match manager.load_active_wallets().await {
            Ok(count) => {
                debug!(
                    "✅ Loaded {} active wallets into memory in {:?}",
                    count,
                    load_start.elapsed()
                );
            }
            Err(e) => {
                warn!("Failed to load wallets on startup: {}", e);
                info!("Wallets will be loaded on-demand during sync");
            }
        }

        manager
    }

    /// Get the network configuration used by all wallets
    pub fn get_network(&self) -> Network {
        self.network
    }

    /// Get the Electrum client manager (for reconnection coordination)
    pub fn get_electrum_manager(&self) -> Option<Arc<ElectrumClientManager>> {
        self.electrum_client_manager.clone()
    }

    /// Get a clone of the current Electrum client (for backward compatibility)
    /// This gets the client from the manager, or None if disconnected
    pub async fn get_electrum_client(&self) -> Option<ElectrumClient> {
        match &self.electrum_client_manager {
            Some(manager) => manager.get_client().await,
            None => None,
        }
    }

    pub(super) async fn acquire_electrum_work(&self) -> Result<Option<OwnedSemaphorePermit>> {
        match &self.electrum_work_gate {
            Some(gate) => {
                Ok(Some(gate.clone().acquire_owned().await.map_err(
                    |error| anyhow!("Electrum work gate closed: {error}"),
                )?))
            }
            None => Ok(None),
        }
    }

    /// Register a newly created wallet into the in-memory storage.
    /// Called by `WalletCreationService` after background wallet setup completes.
    pub(super) async fn register_wallet(
        &self,
        checksum: String,
        wallet: PersistedWallet<Connection>,
        conn: Connection,
    ) {
        let mut wallets_map = self.wallets.lock().await;
        wallets_map.insert(checksum.clone(), Arc::new(Mutex::new((wallet, conn))));
        debug!(
            "[{}] Added newly created wallet to in-memory storage",
            checksum
        );
    }

    /// Get wallet metadata by checksum
    pub async fn get_wallet_by_checksum(
        &self,
        checksum: &str,
    ) -> Result<Option<crate::metadata::WalletMetadata>, anyhow::Error> {
        self.metadata_db.get_wallet_by_checksum(checksum).await
    }

    /// Load all active wallets from disk into memory
    /// This is called on startup to pre-load wallets with active subscriptions
    async fn load_active_wallets(&mut self) -> Result<usize> {
        // Get ready wallets from the database (wallets with active subscriptions)
        let active_wallets = self.metadata_db.get_ready_wallets().await?;

        let mut loaded_count = 0;
        let mut wallets_map = self.wallets.lock().await;

        for wallet_metadata in active_wallets {
            // Address watches use direct Electrum queries, no BDK wallet file
            if wallet_metadata.wallet_type == WalletType::Address {
                debug!(
                    " Skipping address watch: {} ({})",
                    wallet_metadata.name, wallet_metadata.checksum
                );
                continue;
            }

            let wallet_path = self
                .wallet_dir
                .join(format!("{}.sqlite", wallet_metadata.checksum));

            if wallet_path.exists() {
                match Self::load_wallet_from_disk(
                    &wallet_path,
                    &wallet_metadata.descriptor,
                    self.network,
                )
                .await
                {
                    Ok((wallet, conn)) => {
                        wallets_map.insert(
                            wallet_metadata.checksum.clone(),
                            Arc::new(Mutex::new((wallet, conn))),
                        );
                        loaded_count += 1;
                        debug!(
                            " Loaded wallet: {} ({})",
                            wallet_metadata.name, wallet_metadata.checksum
                        );
                    }
                    Err(e) => {
                        warn!(
                            " Failed to load wallet {} from {}: {}",
                            wallet_metadata.name,
                            wallet_path.display(),
                            e
                        );
                    }
                }
            } else {
                warn!(
                    " Wallet file not found for {}: {}",
                    wallet_metadata.name,
                    wallet_path.display()
                );
            }
        }

        Ok(loaded_count)
    }
}
