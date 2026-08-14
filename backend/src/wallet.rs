use crate::config::AppConfig;
use crate::config::NetworkConfig;
use crate::electrum::{ElectrumClient, ElectrumClientManager};
use crate::metadata::{MetadataDb, TransactionNotification, WalletMetadata};
use crate::sync::{AddressWatchSyncResult, DescriptorWalletSyncResult};
use crate::utils::{parse_multipath_descriptor, strip_key_origin};
use anyhow::{anyhow, Result};
use bdk_wallet::rusqlite::Connection;
use bdk_wallet::{bitcoin::Network, bitcoin::ScriptBuf, PersistedWallet, Wallet};
use futures::future::join_all;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, Mutex, OwnedSemaphorePermit, Semaphore};
use tracing::{debug, error, info, warn};

#[cfg(test)]
use bdk_wallet::KeychainKind;

/// Individual wallet entry containing BDK wallet and its SQLite connection
type WalletEntry = Arc<Mutex<(PersistedWallet<Connection>, Connection)>>;

/// Thread-safe map of wallet checksums to wallet entries
type WalletMap = Arc<Mutex<HashMap<String, WalletEntry>>>;

/// Context for completing wallet creation with deep scanning
struct WalletCreationContext {
    wallet_path: PathBuf,
    descriptor: String,
    network: Network,
    electrum_client: Option<ElectrumClient>,
    metadata_db: MetadataDb,
    checksum: String,
    is_fresh_wallet: bool,
    stop_gap: Option<String>,
}

/// Maximum number of wallets to sync in parallel
const MAX_PARALLEL_SYNCS: usize = 10;

#[derive(Debug, Clone)]
enum SelfHostedWorkKind {
    Descriptor(Box<WalletMetadata>),
    AddressGroup {
        descriptor: String,
        watchers: Vec<WalletMetadata>,
    },
}

#[derive(Debug, Clone)]
struct SelfHostedWorkItem {
    key: String,
    last_synced_at: Option<chrono::DateTime<chrono::Utc>>,
    kind: SelfHostedWorkKind,
}

impl SelfHostedWorkItem {
    fn due_at(
        &self,
        interval: Duration,
        now: chrono::DateTime<chrono::Utc>,
    ) -> chrono::DateTime<chrono::Utc> {
        self.last_synced_at
            .and_then(|synced| {
                chrono::Duration::from_std(interval)
                    .ok()
                    .map(|interval| synced + interval)
            })
            .unwrap_or(now)
    }
}

fn parse_last_synced_at(value: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    match chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f") {
        Ok(timestamp) => Some(timestamp.and_utc()),
        Err(error) => {
            warn!(
                "Could not parse persisted wallet last_synced_at timestamp '{value}'; treating it as never synced: {error}"
            );
            None
        }
    }
}

fn build_self_hosted_queue(wallets: Vec<WalletMetadata>) -> Vec<SelfHostedWorkItem> {
    let (address_watches, descriptor_wallets): (Vec<_>, Vec<_>) = wallets
        .into_iter()
        .partition(|wallet| wallet.wallet_type == "address");

    let mut items: Vec<SelfHostedWorkItem> = descriptor_wallets
        .into_iter()
        .map(|wallet| SelfHostedWorkItem {
            key: wallet.checksum.clone(),
            last_synced_at: wallet
                .last_synced_at
                .as_deref()
                .and_then(parse_last_synced_at),
            kind: SelfHostedWorkKind::Descriptor(Box::new(wallet)),
        })
        .collect();

    let mut address_groups: HashMap<String, Vec<WalletMetadata>> = HashMap::new();
    for watch in address_watches {
        address_groups
            .entry(watch.descriptor.clone())
            .or_default()
            .push(watch);
    }
    for (descriptor, mut watchers) in address_groups {
        watchers.sort_by(|left, right| left.checksum.cmp(&right.checksum));
        let stable_checksum = watchers
            .first()
            .expect("address group is never empty")
            .checksum
            .clone();
        let timestamps: Vec<_> = watchers
            .iter()
            .map(|watch| {
                watch
                    .last_synced_at
                    .as_deref()
                    .and_then(parse_last_synced_at)
            })
            .collect();
        let last_synced_at = if timestamps.iter().any(Option::is_none) {
            None
        } else {
            timestamps.into_iter().flatten().min()
        };
        items.push(SelfHostedWorkItem {
            key: format!("address:{stable_checksum}"),
            last_synced_at,
            kind: SelfHostedWorkKind::AddressGroup {
                descriptor,
                watchers,
            },
        });
    }

    // Never-synced first, then oldest completion time, then a stable key.
    items.sort_by(|left, right| {
        left.last_synced_at
            .is_some()
            .cmp(&right.last_synced_at.is_some())
            .then_with(|| left.last_synced_at.cmp(&right.last_synced_at))
            .then_with(|| left.key.cmp(&right.key))
    });
    items
}

fn select_due_self_hosted_item<'a>(
    queue: &'a [SelfHostedWorkItem],
    interval: Duration,
    now_wall: chrono::DateTime<chrono::Utc>,
    now_instant: Instant,
    failure_deferred_until: &HashMap<String, Instant>,
) -> Option<&'a SelfHostedWorkItem> {
    queue.iter().find(|item| {
        item.due_at(interval, now_wall) <= now_wall
            && failure_deferred_until
                .get(&item.key)
                .is_none_or(|until| *until <= now_instant)
    })
}

fn recovery_scan_gap(is_fresh_wallet: bool, requested: Option<&str>) -> usize {
    let default = if is_fresh_wallet {
        crate::electrum::STOP_GAP
    } else {
        500
    };
    match requested {
        None | Some("auto") => default,
        Some(value) => match value.parse::<usize>() {
            Ok(gap) => gap,
            Err(error) => {
                warn!(
                    "Could not parse requested wallet recovery gap '{value}'; using {default}: {error}"
                );
                default
            }
        },
    }
}

fn active_address_watch_descriptors(wallets: &[WalletMetadata]) -> HashSet<&str> {
    wallets
        .iter()
        .filter(|wallet| wallet.wallet_type == "address" && wallet.status != "deleted")
        .map(|wallet| wallet.descriptor.as_str())
        .collect()
}

fn address_watch_sync_targets(watchers: &[WalletMetadata]) -> Vec<(String, bool)> {
    watchers
        .iter()
        .map(|watcher| (watcher.checksum.clone(), watcher.status == "pending"))
        .collect()
}

/// Generate a unique 8-character alphanumeric ID for use as a wallet checksum PK.
/// Used when multiple users watch the same address and need distinct wallet records.
fn generate_unique_wallet_id() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::rng();
    (0..8)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

#[cfg(unix)]
fn restrict_permissions(path: &std::path::Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(path, PermissionsExt::from_mode(mode))
}

#[cfg(unix)]
fn sqlite_sidecar_path(path: &std::path::Path, suffix: &str) -> PathBuf {
    let mut sidecar = path.as_os_str().to_os_string();
    sidecar.push(suffix);
    PathBuf::from(sidecar)
}

/// Standalone wallet creation function that doesn't require WalletManager mutex
/// This allows wallet creation to be non-blocking and concurrent
pub struct WalletCreationService {
    wallet_dir: PathBuf,
    metadata_db: MetadataDb,
    electrum_client: Option<ElectrumClient>,
    network: Network,
    // Reference to WalletManager for registering new wallets
    wallet_manager: Arc<WalletManager>,
}

impl WalletCreationService {
    pub fn new(
        wallet_dir: PathBuf,
        metadata_db: MetadataDb,
        electrum_client: Option<ElectrumClient>,
        network: Network,
        wallet_manager: Arc<WalletManager>,
    ) -> Self {
        Self {
            wallet_dir,
            metadata_db,
            electrum_client,
            network,
            wallet_manager,
        }
    }

    /// Create a single-address watch (no BDK wallet, uses direct Electrum queries)
    async fn create_from_address(
        &self,
        name: &str,
        address: &str,
        user_id: &str,
    ) -> Result<WalletMetadata> {
        use crate::xpub_converter::XpubConverter;

        debug!("Creating address watch for: {}", address);

        // Validate address network
        XpubConverter::validate_address_network(address, self.network)?;

        // Convert to addr() descriptor with checksum
        let descriptor = XpubConverter::address_to_descriptor(address)?;

        self.create_single_script_watch(name, &descriptor, user_id)
            .await
    }

    /// Create a P2PK (Pay-to-Public-Key) watch using direct Electrum queries.
    async fn create_from_pubkey(
        &self,
        name: &str,
        pubkey: &str,
        user_id: &str,
    ) -> Result<WalletMetadata> {
        use crate::xpub_converter::XpubConverter;

        debug!("Creating P2PK watch for pubkey: {}", pubkey);

        // Convert to pk() descriptor with checksum
        let descriptor = XpubConverter::pubkey_to_descriptor(pubkey)?;

        self.create_single_script_watch(name, &descriptor, user_id)
            .await
    }

    /// Shared implementation for creating a single-script watch wallet (addr() or pk()).
    /// Uses wallet_type = "address" for both, since the sync dispatch path is the same:
    /// `script_from_watch_descriptor()` in sync.rs handles resolving either descriptor
    /// type to the correct ScriptBuf.
    async fn create_single_script_watch(
        &self,
        name: &str,
        descriptor: &str,
        user_id: &str,
    ) -> Result<WalletMetadata> {
        // Check if THIS USER already watches this script
        if self
            .metadata_db
            .descriptor_exists_for_user(descriptor, user_id)
            .await?
        {
            let existing_wallets = self.metadata_db.get_wallets_for_user(Some(user_id)).await?;
            let existing_checksum = existing_wallets
                .iter()
                .find(|w| w.descriptor == descriptor)
                .map(|w| w.checksum.clone())
                .unwrap_or_else(|| self.metadata_db.extract_checksum(descriptor));
            return Err(anyhow!(
                "This script is already being watched with ID: {}.",
                existing_checksum
            ));
        }

        // If another user already watches this script, generate a unique short ID
        // so both users get independent wallet records.
        let checksum_override = if self.metadata_db.descriptor_exists(descriptor).await? {
            Some(generate_unique_wallet_id())
        } else {
            None // First watcher — use the standard descriptor checksum
        };

        // Insert wallet metadata with 'address' type (covers both addr() and pk() watches)
        let checksum = self
            .metadata_db
            .insert_wallet_with_type_and_checksum(
                name,
                descriptor,
                user_id,
                "address",
                checksum_override.as_deref(),
            )
            .await?;

        let wallet_metadata = self
            .metadata_db
            .get_wallet_by_checksum(&checksum)
            .await?
            .ok_or_else(|| anyhow!("Failed to retrieve created watch wallet metadata"))?;

        // Spawn a background task for the initial Electrum sync
        let metadata_db = self.metadata_db.clone();
        let electrum_manager = self.wallet_manager.electrum_client_manager.clone();
        let notification_sender = self.wallet_manager.notification_sender.clone();
        let config = self.wallet_manager.config.clone();
        let descriptor_clone = descriptor.to_string();
        let checksum_clone = checksum.clone();
        let electrum_work_gate = self.wallet_manager.electrum_work_gate.clone();

        tokio::spawn(async move {
            let _work_permit = match electrum_work_gate {
                Some(gate) => match gate.acquire_owned().await {
                    Ok(permit) => Some(permit),
                    Err(error) => {
                        error!("[{checksum_clone}] Electrum work gate closed: {error}");
                        return;
                    }
                },
                None => None,
            };
            let sync_service = crate::sync::WalletSyncService::new(
                metadata_db.clone(),
                notification_sender,
                config,
            );
            // suppress_notifications=true: this is the initial sync, so all transactions
            // are historical and should not trigger alerts to contacts
            match sync_service
                .sync_address_watch(
                    &checksum_clone,
                    &descriptor_clone,
                    electrum_manager.as_deref(),
                    true,
                )
                .await
            {
                Ok(AddressWatchSyncResult::Completed { has_changes }) => {
                    // Mark as ready only after successful sync
                    if let Err(e) = metadata_db
                        .update_wallet_status_if_not_deleted(&checksum_clone, "ready")
                        .await
                    {
                        warn!(
                            "[{}] Failed to promote wallet to ready: {}",
                            checksum_clone, e
                        );
                    }
                    debug!(
                        "[{}] Initial watch sync completed (changes={})",
                        checksum_clone, has_changes
                    );
                }
                Ok(AddressWatchSyncResult::SkippedNoClient) => {
                    debug!(
                        "[{}] No Electrum client yet, wallet stays pending",
                        checksum_clone
                    );
                }
                Err(e) => {
                    error!("[{}] Initial watch sync failed: {}", checksum_clone, e);
                    if let Err(status_error) = metadata_db
                        .update_wallet_status_if_not_deleted(&checksum_clone, "failed")
                        .await
                    {
                        warn!(
                            "[{}] Failed to mark wallet as failed after initial watch sync error: {}",
                            checksum_clone, status_error
                        );
                    }
                }
            }
        });

        Ok(wallet_metadata)
    }

    /// Create wallet without blocking WalletManager
    /// Returns wallet metadata immediately while background task handles BDK wallet creation
    pub async fn create_wallet_non_blocking(
        &self,
        name: &str,
        descriptor_str: &str,
        user_id: &str,
        is_fresh_wallet: bool,
        script_type: Option<&str>,
        stop_gap: Option<&str>,
    ) -> Result<WalletMetadata> {
        use crate::xpub_converter::XpubConverter;

        debug!("Creating wallet from multipath descriptor:");
        debug!(" Name: {}", name);
        debug!(" Input descriptor: {}", descriptor_str);

        // Validate network compatibility (defense-in-depth)
        XpubConverter::validate_descriptor_network(descriptor_str, self.network)?;

        // Check if input is a raw public key (P2PK watch)
        if XpubConverter::is_bitcoin_public_key(descriptor_str) {
            return self.create_from_pubkey(name, descriptor_str, user_id).await;
        }

        // Check if input is a single Bitcoin address
        if XpubConverter::is_bitcoin_address(descriptor_str) {
            return self
                .create_from_address(name, descriptor_str, user_id)
                .await;
        }

        // Check if input is an XPUB - convert to descriptor with script type
        if XpubConverter::is_xpub(descriptor_str) && !is_fresh_wallet {
            // Use provided script type, or default to P2WPKH (most common for existing wallets)
            let effective_script_type = match script_type {
                Some(st) if st != "auto" => st,
                _ => {
                    debug!("No script type specified for XPUB, defaulting to p2wpkh");
                    "p2wpkh"
                }
            };

            debug!(
                "Converting XPUB to descriptor with script type '{}'",
                effective_script_type
            );
            return self
                .create_from_xpub_with_known_type(
                    name,
                    descriptor_str,
                    user_id,
                    effective_script_type,
                    stop_gap,
                )
                .await;
        }

        // Strip key origin to prevent duplicate wallets with same XPUB
        let normalized_descriptor = strip_key_origin(descriptor_str)?;

        // Check if normalized descriptor already exists
        if self
            .metadata_db
            .descriptor_exists(&normalized_descriptor)
            .await?
        {
            let checksum = self.metadata_db.extract_checksum(&normalized_descriptor);
            return Err(anyhow!("This wallet has already been added with ID: {}. Ask the wallet owner to add you as a contact for notifications.", checksum));
        }

        // Parse and validate the normalized multipath descriptor
        parse_multipath_descriptor(&normalized_descriptor)?;

        // Extract checksum from the normalized descriptor for consistent filename
        let checksum = self.metadata_db.extract_checksum(&normalized_descriptor);
        let wallet_filename_with_ext = format!("{}.sqlite", checksum);
        debug!(
            "[{}] Wallet filename: {}",
            checksum, wallet_filename_with_ext
        );

        // Create wallet file path
        let wallet_path = self.wallet_dir.join(&wallet_filename_with_ext);
        debug!("[{}] Wallet file path: {}", checksum, wallet_path.display());

        // Check if wallet file already exists
        if wallet_path.exists() {
            return Err(anyhow!("Wallet file already exists"));
        }

        // PHASE 1: Save wallet metadata immediately (synchronous)
        let wallet_checksum = self
            .metadata_db
            .insert_wallet(name, &normalized_descriptor, user_id)
            .await?;
        debug!(
            "[{}] Metadata saved with checksum: {}",
            checksum, wallet_checksum
        );

        // Get wallet metadata to return immediately
        let wallet_metadata = self
            .metadata_db
            .get_wallet_by_descriptor(&normalized_descriptor)
            .await?
            .ok_or_else(|| anyhow!("Failed to retrieve created wallet metadata"))?;

        // PHASE 2: Spawn background task for slow operations
        let wallet_manager_clone = self.wallet_manager.clone();
        let ctx = WalletCreationContext {
            wallet_path: wallet_path.clone(),
            descriptor: normalized_descriptor,
            network: self.network,
            electrum_client: self.electrum_client.clone(),
            metadata_db: self.metadata_db.clone(),
            checksum: checksum.clone(),
            is_fresh_wallet,
            stop_gap: stop_gap.map(|s| s.to_string()),
        };

        Self::spawn_background_wallet_creation(
            wallet_checksum.clone(),
            ctx,
            wallet_manager_clone,
            self.metadata_db.clone(),
        );

        Ok(wallet_metadata)
    }

    // Helper methods that mirror WalletManager functionality
    async fn create_from_xpub_with_known_type(
        &self,
        name: &str,
        xpub: &str,
        user_id: &str,
        script_type_str: &str,
        stop_gap: Option<&str>,
    ) -> Result<WalletMetadata> {
        use crate::xpub_converter::{ScriptType, XpubConverter};

        debug!(
            "Creating XPUB wallet with known script type: {}",
            script_type_str
        );

        // Validate network compatibility (defense-in-depth)
        XpubConverter::validate_key_network(xpub, self.network)?;

        // Parse script type
        let script_type = match script_type_str {
            "p2wpkh" => ScriptType::P2WPKH,
            "p2sh" => ScriptType::P2SH,
            "p2pkh" => ScriptType::P2PKH,
            "p2tr" => ScriptType::P2TR,
            _ => return Err(anyhow!("Invalid script type: {}", script_type_str)),
        };

        // Create converter and generate descriptor
        let converter = XpubConverter::new(self.network, self.electrum_client.as_ref());
        let descriptor = converter.generate_descriptor_for_type(xpub, &script_type)?;

        debug!("Generated descriptor: {}", descriptor);

        // Strip key origin for consistency
        let normalized_descriptor = strip_key_origin(&descriptor)?;

        // Check if this descriptor already exists
        if self
            .metadata_db
            .descriptor_exists(&normalized_descriptor)
            .await?
        {
            let checksum = self.metadata_db.extract_checksum(&normalized_descriptor);
            return Err(anyhow!("This wallet has already been added with ID: {}. Ask the wallet owner to add you as a contact for notifications.", checksum));
        }

        // Parse multipath descriptor
        parse_multipath_descriptor(&normalized_descriptor)?;

        // Extract checksum
        let checksum = self.metadata_db.extract_checksum(&normalized_descriptor);
        let wallet_filename_with_ext = format!("{}.sqlite", checksum);
        let wallet_path = self.wallet_dir.join(&wallet_filename_with_ext);

        // Check if wallet file already exists
        if wallet_path.exists() {
            return Err(anyhow!("Wallet file already exists"));
        }

        // Save wallet metadata immediately
        let wallet_checksum = self
            .metadata_db
            .insert_wallet(name, &normalized_descriptor, user_id)
            .await?;
        debug!(
            "[{}] Metadata saved with checksum: {}",
            checksum, wallet_checksum
        );

        // Get wallet metadata to return immediately
        let wallet_metadata = self
            .metadata_db
            .get_wallet_by_descriptor(&normalized_descriptor)
            .await?
            .ok_or_else(|| anyhow!("Failed to retrieve created wallet metadata"))?;

        // PHASE 2: Spawn background task for slow operations
        let wallet_manager_clone = self.wallet_manager.clone();
        let ctx = WalletCreationContext {
            wallet_path: wallet_path.clone(),
            descriptor: normalized_descriptor,
            network: self.network,
            electrum_client: self.electrum_client.clone(),
            metadata_db: self.metadata_db.clone(),
            checksum: checksum.clone(),
            is_fresh_wallet: true, // Fresh wallet for XPUB with known type
            stop_gap: stop_gap.map(|s| s.to_string()),
        };

        Self::spawn_background_wallet_creation(
            wallet_checksum.clone(),
            ctx,
            wallet_manager_clone,
            self.metadata_db.clone(),
        );

        Ok(wallet_metadata)
    }

    fn spawn_background_wallet_creation(
        wallet_checksum: String,
        ctx: WalletCreationContext,
        wallet_manager: Arc<WalletManager>,
        metadata_db: MetadataDb,
    ) {
        tokio::spawn(async move {
            debug!(
                "[{}] Starting background wallet creation with stop gap: {:?}",
                ctx.checksum, ctx.stop_gap
            );
            if let Err(e) =
                WalletManager::complete_wallet_creation_with_stop_gap(ctx, wallet_manager).await
            {
                error!(
                    "[{}] Background wallet creation failed: {}",
                    wallet_checksum, e
                );
                if let Err(status_error) = metadata_db
                    .update_wallet_status_if_not_deleted(&wallet_checksum, "failed")
                    .await
                {
                    warn!(
                        "[{}] Failed to mark wallet as failed after background creation error: {}",
                        wallet_checksum, status_error
                    );
                }
            } else {
                debug!(
                    "[{}] Background wallet creation with scan depth completed",
                    wallet_checksum
                );
            }
        });
    }
}

pub struct WalletManager {
    // Thread-safe HashMap for in-memory wallet storage
    // Each wallet has its own mutex for parallel access
    pub wallets: WalletMap,
    pub wallet_dir: PathBuf,
    /// Electrum client manager with automatic reconnection support
    pub electrum_client_manager: Option<Arc<ElectrumClientManager>>,
    pub metadata_db: MetadataDb,
    pub notification_sender: broadcast::Sender<TransactionNotification>,
    network: Network,
    config: AppConfig,
    /// Fair one-at-a-time gate for script-heavy work against a self-hosted endpoint.
    electrum_work_gate: Option<Arc<Semaphore>>,
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
        std::fs::create_dir_all(&wallet_dir)
            .unwrap_or_else(|e| panic!("Failed to create wallet directory: {e}"));
        #[cfg(unix)]
        restrict_permissions(&wallet_dir, 0o700)
            .unwrap_or_else(|e| panic!("Failed to restrict wallet directory permissions: {e}"));

        // Initialize electrum client manager with automatic reconnection support
        let electrum_client_manager = match ElectrumClientManager::new_with_subscriptions(
            electrum_url,
            config.is_self_hosted_mode(),
        ) {
            Ok(manager) => {
                let manager = Arc::new(manager);
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
            }
            Err(e) => {
                error!(
                    "❌ Failed to create ElectrumClientManager for {}: {}",
                    electrum_url, e
                );
                info!("Wallet sync will not work without Electrum connection!");
                None
            }
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

    async fn acquire_electrum_work(&self) -> Result<Option<OwnedSemaphorePermit>> {
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
    /// Called by WalletCreationService after background wallet setup completes.
    pub async fn register_wallet(
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

    /// Background task to complete wallet creation with scan depth support
    async fn complete_wallet_creation_with_stop_gap(
        ctx: WalletCreationContext,
        wallet_manager: Arc<WalletManager>,
    ) -> Result<()> {
        let stop_gap = ctx.stop_gap.as_deref();
        debug!(
            "[{}] Starting background wallet creation with stop gap: {:?}",
            ctx.checksum, stop_gap
        );

        // Create SQLite connection
        let mut db = Connection::open(&ctx.wallet_path).map_err(|e| {
            anyhow!(
                "Failed to create connection to {}: {}",
                ctx.wallet_path.display(),
                e
            )
        })?;
        #[cfg(unix)]
        restrict_permissions(&ctx.wallet_path, 0o600)?;
        // If a later creation step fails, keep the partial BDK SQLite file with the
        // failed wallet record so the normal delete cleanup path can remove both.

        // Canary validates the descriptor before spawning this task. Keep the normalized
        // two-path form intact so BDK owns the receive/change split used for persistence.
        let mut wallet = Wallet::create_from_two_path_descriptor(ctx.descriptor.clone())
            .network(ctx.network)
            .create_wallet(&mut db)
            .map_err(|e| anyhow!("Failed to create wallet: {}", e))?;

        // Persist initial wallet state
        wallet
            .persist(&mut db)
            .map_err(|e| anyhow!("Failed to persist wallet: {}", e))?;
        #[cfg(unix)]
        for path in [
            ctx.wallet_path.clone(),
            sqlite_sidecar_path(&ctx.wallet_path, "-wal"),
            sqlite_sidecar_path(&ctx.wallet_path, "-shm"),
        ] {
            if path.exists() {
                restrict_permissions(&path, 0o600)?;
            }
        }

        // One BDK full scan owns discovery. Advanced selections are stop gaps (consecutive
        // unused scripts), existing-wallet automatic recovery uses gap 500, and fresh wallets
        // keep the normal 20-script gap.
        if let Some(ref client) = ctx.electrum_client {
            let scan_gap = recovery_scan_gap(ctx.is_fresh_wallet, stop_gap);

            let _work_permit = wallet_manager.acquire_electrum_work().await?;
            if let Err(e) = client.full_scan_wallet(&mut wallet, scan_gap).await {
                return Err(anyhow!(
                    "[{}] Failed to full scan wallet during background creation: {}",
                    ctx.checksum,
                    e
                ));
            }

            // Persisting the scanned BDK state is part of initial creation. If this
            // fails, the UI should expose the terminal v1 recovery path: delete and
            // add the wallet again instead of presenting an unsaved wallet as ready.
            if let Err(e) = wallet.persist(&mut db) {
                return Err(anyhow!(
                    "[{}] Failed to persist wallet after sync: {}",
                    ctx.checksum,
                    e
                ));
            }
        }

        // Update balance in metadata database
        let balance = wallet.balance().total().to_sat() as i64;
        if let Err(e) = ctx
            .metadata_db
            .update_wallet_balance_by_checksum(&ctx.checksum, balance)
            .await
        {
            error!(
                "[{}] Warning: Failed to update wallet balance: {}",
                ctx.checksum, e
            );
        }

        // Extract historical transactions after sync
        if let Err(e) =
            crate::sync::WalletSyncService::extract_historical_transactions_for_background(
                &wallet,
                &ctx.checksum,
                &ctx.metadata_db,
                ctx.electrum_client.as_ref(),
            )
            .await
        {
            error!(
                "[{}] Warning: Failed to extract historical transactions: {}",
                ctx.checksum, e
            );
        }

        // Update last synced timestamp
        if let Err(e) = ctx
            .metadata_db
            .update_wallet_last_synced(&ctx.checksum)
            .await
        {
            error!(
                "[{}] Warning: Failed to update wallet last synced: {}",
                ctx.checksum, e
            );
        }

        // Mark wallet as ready only if not already deleted (prevents race condition with deletion)
        match ctx
            .metadata_db
            .update_wallet_status_if_not_deleted(&ctx.checksum, "ready")
            .await
        {
            Ok(true) => {
                debug!(
                    "[{}] Wallet marked as ready - available for frontend display",
                    ctx.checksum
                );
            }
            Ok(false) => {
                debug!(
                    "[{}] Wallet status not updated (deleted during creation)",
                    ctx.checksum
                );
                return Ok(()); // Skip adding to memory since wallet is deleted
            }
            Err(e) => {
                error!("[{}] Failed to mark wallet as ready: {}", ctx.checksum, e);
                // Don't add to memory if DB state is unknown - next sync cycle will handle it
                return Ok(());
            }
        }

        // Add wallet to in-memory storage after it's fully set up and marked as ready
        if let Ok((wallet, conn)) =
            Self::load_wallet_from_disk(&ctx.wallet_path, &ctx.descriptor, ctx.network).await
        {
            wallet_manager
                .register_wallet(ctx.checksum.clone(), wallet, conn)
                .await;
        } else {
            error!(
                "[{}] Failed to load wallet into memory after creation",
                ctx.checksum
            );
        }

        debug!(
            "[{}] Background wallet creation with scan depth completed",
            ctx.checksum
        );
        Ok(())
    }

    /// Clean up deleted wallets - remove from memory, disk, and database
    async fn cleanup_deleted_wallets(&self) -> Result<()> {
        // Get ready wallets from database (source of truth)
        let ready_wallets = self.metadata_db.get_ready_wallets().await?;

        // Pending address watches already own the same endpoint subscription that their initial
        // sync will use. Keep shared scripts until every non-deleted watcher is gone.
        let non_deleted_wallets = self.metadata_db.get_all_wallets().await?;

        // Get wallets marked as deleted in database
        let deleted_wallets = self.metadata_db.get_deleted_wallets().await?;

        // Create set of valid checksums from database
        let valid_checksums: HashSet<String> =
            ready_wallets.iter().map(|w| w.checksum.clone()).collect();
        let active_watch_descriptors = active_address_watch_descriptors(&non_deleted_wallets);

        // Collect wallets that need to be removed
        let mut wallets_to_remove = Vec::new();

        // Check wallets in memory against valid checksums
        {
            let wallets_map = self.wallets.lock().await;
            for checksum in wallets_map.keys() {
                if !valid_checksums.contains(checksum) {
                    wallets_to_remove.push(checksum.clone());
                }
            }
        }

        // Also add wallets marked as deleted in database (even if not in memory)
        for deleted_wallet in &deleted_wallets {
            if !wallets_to_remove.contains(&deleted_wallet.checksum) {
                wallets_to_remove.push(deleted_wallet.checksum.clone());
            }
        }

        let mut scripts_to_forget = HashSet::<ScriptBuf>::new();

        // Clean up each removed wallet: memory, disk, and database
        for checksum in wallets_to_remove {
            debug!("Cleaning up wallet {} (deleted or expired)", checksum);
            let deleted_metadata = deleted_wallets
                .iter()
                .find(|wallet| wallet.checksum == checksum);
            let wallet_path = self.wallet_dir.join(format!("{}.sqlite", checksum));
            let mut removed_from_memory = false;

            // Remove from memory (if it was loaded)
            {
                let mut wallets_map = self.wallets.lock().await;
                if let Some(wallet_arc) = wallets_map.remove(&checksum) {
                    removed_from_memory = true;
                    // Persist final state before removal
                    let mut wallet_data = wallet_arc.lock().await;
                    let (wallet, conn) = &mut *wallet_data;
                    scripts_to_forget.extend(
                        wallet
                            .spk_index()
                            .revealed_spks(..)
                            .map(|(_, script)| script),
                    );
                    if let Err(e) = wallet.persist(conn) {
                        warn!("Failed to persist wallet before removal: {}", e);
                    }
                    debug!("Removed wallet from memory");
                }
            }

            if let Some(metadata) = deleted_metadata {
                if metadata.wallet_type == "address" {
                    // Multiple users may share one address-watch subscription. Keep it until the
                    // last active watcher is gone.
                    if !active_watch_descriptors.contains(metadata.descriptor.as_str()) {
                        match crate::sync::WalletSyncService::script_from_watch_descriptor(
                            &metadata.descriptor,
                            self.network,
                        ) {
                            Ok(script) => {
                                scripts_to_forget.insert(script);
                            }
                            Err(error) => warn!(
                                "Could not resolve deleted address watch {} for Electrum cleanup: {error}",
                                metadata.checksum
                            ),
                        }
                    }
                } else if !removed_from_memory && wallet_path.exists() {
                    match Self::load_wallet_from_disk(
                        &wallet_path,
                        &metadata.descriptor,
                        self.network,
                    )
                    .await
                    {
                        Ok((wallet, _)) => scripts_to_forget.extend(
                            wallet
                                .spk_index()
                                .revealed_spks(..)
                                .map(|(_, script)| script),
                        ),
                        Err(error) => warn!(
                            "Could not load deleted wallet {} for Electrum cleanup: {error}",
                            metadata.checksum
                        ),
                    }
                }
            }

            // Delete wallet file from disk (only for descriptor-type wallets; address watches have no file)
            if wallet_path.exists() {
                if let Err(e) = std::fs::remove_file(&wallet_path) {
                    error!(
                        "  Warning: Failed to delete wallet file {}: {}",
                        wallet_path.display(),
                        e
                    );
                } else {
                    debug!("Deleted wallet file: {}", wallet_path.display());
                }
            }

            // Hard delete from database (if it was marked as deleted)
            if let Err(e) = self
                .metadata_db
                .hard_delete_wallet_by_checksum(&checksum)
                .await
            {
                error!(
                    "  Warning: Failed to hard delete wallet {} from database: {}",
                    checksum, e
                );
            } else {
                debug!("Hard deleted wallet {} from database", checksum);
            }
        }

        if !scripts_to_forget.is_empty() && self.electrum_work_gate.is_some() {
            let _work_permit = self.acquire_electrum_work().await?;
            if let Some(manager) = &self.electrum_client_manager {
                if let Err(error) = manager
                    .forget_scripts(scripts_to_forget.into_iter().collect())
                    .await
                {
                    warn!("Failed to clean up deleted Electrum subscriptions: {error}");
                }
            }
        }

        Ok(())
    }

    async fn sync_self_hosted_work_item(&self, item: &SelfHostedWorkItem) -> Result<()> {
        use crate::sync::WalletSyncService;

        let _work_permit = self.acquire_electrum_work().await?;
        match &item.kind {
            SelfHostedWorkKind::AddressGroup {
                descriptor,
                watchers,
            } => {
                let sync_service = WalletSyncService::new(
                    self.metadata_db.clone(),
                    self.notification_sender.clone(),
                    self.config.clone(),
                );
                let targets = address_watch_sync_targets(watchers);
                let result = if watchers.len() == 1 {
                    sync_service
                        .sync_address_watch(
                            &targets[0].0,
                            descriptor,
                            self.electrum_client_manager.as_deref(),
                            targets[0].1,
                        )
                        .await
                } else {
                    sync_service
                        .sync_address_watch_group(
                            &targets,
                            descriptor,
                            self.electrum_client_manager.as_deref(),
                        )
                        .await
                }?;

                if matches!(result, AddressWatchSyncResult::SkippedNoClient) {
                    return Err(anyhow!("No Electrum client available"));
                }
                for watcher in watchers.iter().filter(|watch| watch.status == "pending") {
                    self.metadata_db
                        .update_wallet_status_if_not_deleted(&watcher.checksum, "ready")
                        .await?;
                }
            }
            SelfHostedWorkKind::Descriptor(metadata) => {
                let wallet_entry = if let Some(entry) =
                    self.wallets.lock().await.get(&metadata.checksum).cloned()
                {
                    entry
                } else {
                    let wallet_path = self
                        .wallet_dir
                        .join(format!("{}.sqlite", metadata.checksum));
                    let (wallet, connection) = Self::load_wallet_from_disk(
                        &wallet_path,
                        &metadata.descriptor,
                        self.network,
                    )
                    .await?;
                    let entry = Arc::new(Mutex::new((wallet, connection)));
                    self.wallets
                        .lock()
                        .await
                        .insert(metadata.checksum.clone(), entry.clone());
                    entry
                };

                let mut wallet_data = wallet_entry.lock().await;
                let (wallet, connection) = &mut *wallet_data;
                let sync_service = WalletSyncService::new(
                    self.metadata_db.clone(),
                    self.notification_sender.clone(),
                    self.config.clone(),
                );
                match sync_service
                    .sync_wallet_by_checksum(
                        wallet,
                        &metadata.checksum,
                        self.electrum_client_manager.as_deref(),
                    )
                    .await?
                {
                    DescriptorWalletSyncResult::Completed => {
                        wallet.persist(connection).map_err(|error| {
                            anyhow!("Failed to persist wallet {}: {error}", metadata.name)
                        })?;
                        if metadata.status == "pending" {
                            self.metadata_db
                                .update_wallet_status_if_not_deleted(&metadata.checksum, "ready")
                                .await?;
                        }
                    }
                    DescriptorWalletSyncResult::SkippedNoClient => {
                        return Err(anyhow!("No Electrum client available"));
                    }
                }
            }
        }
        Ok(())
    }

    /// Run the self-hosted oldest-first due-time queue.
    ///
    /// Each successful completion establishes the next target time. Failures are deferred by one
    /// interval so a broken wallet cannot starve healthy wallets. The shared semaphore is acquired
    /// for one item only, allowing wallet creation to join the same fair queue between recurring
    /// items.
    pub async fn run_self_hosted_sync_queue(&self, sync_interval_secs: u64) -> Result<()> {
        let sync_interval = Duration::from_secs(sync_interval_secs.max(1));
        let mut failure_deferred_until: HashMap<String, Instant> = HashMap::new();

        loop {
            if let Err(error) = self.cleanup_deleted_wallets().await {
                warn!("Self-hosted wallet cleanup failed; retrying in {sync_interval:?}: {error}");
                tokio::time::sleep(sync_interval).await;
                continue;
            }
            let network_config = NetworkConfig::from_network(self.network);
            let wallets = match self
                .metadata_db
                .get_wallets_for_tier_sync(
                    &crate::subscription::SubscriptionTier::Team,
                    &network_config,
                )
                .await
            {
                Ok(wallets) => wallets,
                Err(error) => {
                    warn!(
                        "Could not refresh self-hosted Electrum queue; retrying in {sync_interval:?}: {error}"
                    );
                    tokio::time::sleep(sync_interval).await;
                    continue;
                }
            };
            let queue = build_self_hosted_queue(wallets);
            let active_keys: HashSet<_> = queue.iter().map(|item| item.key.as_str()).collect();
            failure_deferred_until.retain(|key, _| active_keys.contains(key.as_str()));

            if queue.is_empty() {
                debug!("Self-hosted Electrum queue is empty");
                tokio::time::sleep(sync_interval).await;
                continue;
            }

            let now_wall = chrono::Utc::now();
            let now_instant = Instant::now();
            let selected = select_due_self_hosted_item(
                &queue,
                sync_interval,
                now_wall,
                now_instant,
                &failure_deferred_until,
            );

            let Some(item) = selected else {
                let wait = queue
                    .iter()
                    .map(|item| {
                        let due_wait = (item.due_at(sync_interval, now_wall) - now_wall)
                            .to_std()
                            .unwrap_or(Duration::ZERO);
                        let failure_wait = failure_deferred_until
                            .get(&item.key)
                            .map(|until| until.saturating_duration_since(now_instant))
                            .unwrap_or(Duration::ZERO);
                        due_wait.max(failure_wait)
                    })
                    .min()
                    .unwrap_or(sync_interval);
                debug!(
                    "Self-hosted Electrum queue depth={}, next item due in {:.2?}",
                    queue.len(),
                    wait
                );
                tokio::time::sleep(wait).await;
                continue;
            };

            let oldest_lateness = queue
                .iter()
                .map(|queued| {
                    (now_wall - queued.due_at(sync_interval, now_wall))
                        .to_std()
                        .unwrap_or(Duration::ZERO)
                })
                .max()
                .unwrap_or(Duration::ZERO);
            info!(
                "Self-hosted Electrum queue depth={}, oldest_item_lateness={:.2?}, starting={}",
                queue.len(),
                oldest_lateness,
                item.key
            );
            let work_start = Instant::now();
            match self.sync_self_hosted_work_item(item).await {
                Ok(()) => {
                    failure_deferred_until.remove(&item.key);
                    info!(
                        "Self-hosted Electrum work item {} completed in {:.2?}",
                        item.key,
                        work_start.elapsed()
                    );
                }
                Err(error) => {
                    failure_deferred_until.insert(item.key.clone(), Instant::now() + sync_interval);
                    warn!(
                        "Self-hosted Electrum work item {} failed after {:.2?}; deferred for {:.2?}: {}",
                        item.key,
                        work_start.elapsed(),
                        sync_interval,
                        error
                    );
                }
            }
        }
    }

    /// Sync all wallets for a specific subscription tier in parallel
    pub async fn sync_tier_parallel(
        &self,
        tier: crate::subscription::SubscriptionTier,
    ) -> Result<()> {
        use crate::sync::WalletSyncService;

        let sync_start = Instant::now();

        // First, perform wallet cleanup (remove deleted wallets from memory and disk)
        self.cleanup_deleted_wallets().await?;

        // Convert Network to NetworkConfig for the query
        let network_config = NetworkConfig::from_network(self.network);

        // Get wallets for this tier from metadata
        let tier_wallets = self
            .metadata_db
            .get_wallets_for_tier_sync(&tier, &network_config)
            .await?;

        // Get summary of non-syncing wallets
        if let Ok(non_syncing_summary) = self.metadata_db.get_non_syncing_wallets_summary().await {
            if non_syncing_summary.total_non_syncing > 0 {
                let mut reasons = Vec::new();
                if non_syncing_summary.expired_trials > 0 {
                    reasons.push(format!(
                        "{} expired trials",
                        non_syncing_summary.expired_trials
                    ));
                }
                if non_syncing_summary.cancelled_subscriptions > 0 {
                    reasons.push(format!(
                        "{} cancelled",
                        non_syncing_summary.cancelled_subscriptions
                    ));
                }
                if non_syncing_summary.expired_subscriptions > 0 {
                    reasons.push(format!(
                        "{} expired",
                        non_syncing_summary.expired_subscriptions
                    ));
                }
                if non_syncing_summary.past_due_subscriptions > 0 {
                    reasons.push(format!(
                        "{} past_due",
                        non_syncing_summary.past_due_subscriptions
                    ));
                }
                if non_syncing_summary.inactive_wallets > 0 {
                    reasons.push(format!("{} inactive", non_syncing_summary.inactive_wallets));
                }

                info!(
                    "🔒 Subscription status: {} wallets not syncing ({})",
                    non_syncing_summary.total_non_syncing,
                    reasons.join(", ")
                );
            }
        }

        if tier_wallets.is_empty() {
            debug!("No {:?} tier wallets to sync", tier);
            return Ok(());
        }

        // Partition into BDK wallets and address watches
        let (address_watches, bdk_wallets): (Vec<_>, Vec<_>) = tier_wallets
            .into_iter()
            .partition(|w| w.wallet_type == "address");

        // Sync address watches in parallel, grouped by descriptor to deduplicate
        // Electrum queries when multiple users watch the same address
        if !address_watches.is_empty() {
            // Group address watches by descriptor
            let groups = self
                .metadata_db
                .get_address_watches_grouped_by_descriptor(&address_watches)
                .await;

            let total_watchers = address_watches.len();
            let unique_descriptors = groups.len();
            debug!(
                "🔍 Syncing {} {:?} tier address watches ({} unique descriptors)",
                total_watchers, tier, unique_descriptors
            );

            let semaphore = Arc::new(Semaphore::new(MAX_PARALLEL_SYNCS));
            let metadata_db = self.metadata_db.clone();
            let notification_sender = self.notification_sender.clone();
            let electrum_manager = self.electrum_client_manager.clone();
            let config = self.config.clone();

            // Spawn one task per unique descriptor (not per watcher)
            let addr_tasks: Vec<_> = groups
                .into_iter()
                .map(|(descriptor, watchers)| {
                    let semaphore = semaphore.clone();
                    let metadata_db = metadata_db.clone();
                    let notification_sender = notification_sender.clone();
                    let electrum_manager = electrum_manager.clone();
                    let config = config.clone();
                    let watcher_count = watchers.len();

                    tokio::spawn(async move {
                        let _permit = semaphore
                            .acquire()
                            .await
                            .map_err(|e| anyhow!("Failed to acquire semaphore: {}", e))?;

                        let sync_service = crate::sync::WalletSyncService::new(
                            metadata_db.clone(),
                            notification_sender,
                            config,
                        );

                        let targets = address_watch_sync_targets(&watchers);

                        if watcher_count == 1 {
                            let (checksum, is_pending) = &targets[0];
                            // Single watcher — suppress notifications on first sync to avoid
                            // alerting on historical transactions
                            match sync_service
                                .sync_address_watch(
                                    checksum,
                                    &descriptor,
                                    electrum_manager.as_deref(),
                                    *is_pending, // suppress on first sync for pending wallets
                                )
                                .await
                            {
                                Ok(AddressWatchSyncResult::Completed { .. }) => {
                                    // Promote pending watcher to ready
                                    if *is_pending {
                                        if let Err(e) = metadata_db
                                            .update_wallet_status_if_not_deleted(
                                                checksum,
                                                "ready",
                                            )
                                            .await
                                        {
                                            warn!(
                                                "Failed to promote address watch {} to ready: {}",
                                                checksum, e
                                            );
                                        }
                                    }
                                    Ok(watcher_count)
                                }
                                Ok(AddressWatchSyncResult::SkippedNoClient) => Ok(watcher_count),
                                Err(e) => {
                                    warn!("Failed to sync address watch {}: {}", checksum, e);
                                    Err(e)
                                }
                            }
                        } else {
                            // Multiple watchers share Electrum queries, but notification
                            // suppression remains specific to each pending watcher.
                            match sync_service
                                .sync_address_watch_group(
                                    &targets,
                                    &descriptor,
                                    electrum_manager.as_deref(),
                                )
                                .await
                            {
                                Ok(AddressWatchSyncResult::Completed { .. }) => {
                                    // Promote any pending watchers to ready
                                    for w in &watchers {
                                        if w.status == "pending" {
                                            if let Err(e) = metadata_db
                                                .update_wallet_status_if_not_deleted(
                                                    &w.checksum,
                                                    "ready",
                                                )
                                                .await
                                            {
                                                warn!(
                                                    "Failed to promote address watch {} to ready: {}",
                                                    w.checksum, e
                                                );
                                            }
                                        }
                                    }
                                    Ok(watcher_count)
                                }
                                Ok(AddressWatchSyncResult::SkippedNoClient) => Ok(watcher_count),
                                Err(e) => {
                                    warn!(
                                        "Failed to sync address watch group ({}): {}",
                                        descriptor, e
                                    );
                                    Err(e)
                                }
                            }
                        }
                    })
                })
                .collect();

            let results = join_all(addr_tasks).await;
            let mut addr_success = 0;
            let mut addr_failure = 0;
            for result in results {
                match result {
                    Ok(Ok(count)) => addr_success += count,
                    _ => addr_failure += 1,
                }
            }
            debug!(
                "🔍 Address watch sync done - Success: {}, Failed: {}",
                addr_success, addr_failure
            );
        }

        let tier_wallets = bdk_wallets;

        if tier_wallets.is_empty() {
            debug!("No {:?} tier BDK wallets to sync", tier);
            return Ok(());
        }

        debug!(
            "🔄 Starting parallel sync for {} {:?} tier wallets (in-memory)",
            tier_wallets.len(),
            tier
        );

        // Ensure all BDK tier wallets are loaded in memory
        {
            let wallets_map = self.wallets.lock().await;
            let mut missing_wallets = Vec::new();

            for wallet_metadata in &tier_wallets {
                if !wallets_map.contains_key(&wallet_metadata.checksum) {
                    missing_wallets.push(wallet_metadata.clone());
                }
            }

            drop(wallets_map); // Release lock before loading

            // Load any missing wallets into memory
            if !missing_wallets.is_empty() {
                info!(
                    " Loading {} missing wallets into memory",
                    missing_wallets.len()
                );
                for wallet_metadata in missing_wallets {
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
                                let mut wallets_map = self.wallets.lock().await;
                                wallets_map.insert(
                                    wallet_metadata.checksum.clone(),
                                    Arc::new(Mutex::new((wallet, conn))),
                                );
                                debug!(
                                    " Loaded wallet: {} ({})",
                                    wallet_metadata.name, wallet_metadata.checksum
                                );
                            }
                            Err(e) => {
                                error!(
                                    "Failed to load wallet {} into memory: {}",
                                    wallet_metadata.name, e
                                );
                            }
                        }
                    } else {
                        error!("Wallet file not found: {}", wallet_path.display());
                    }
                }
            }
        }

        // Get wallet references from in-memory storage
        let wallet_refs: Vec<_> = {
            let wallets_map = self.wallets.lock().await;

            tier_wallets
                .iter()
                .filter_map(|metadata| {
                    wallets_map
                        .get(&metadata.checksum)
                        .map(|wallet_arc| (metadata.clone(), wallet_arc.clone()))
                })
                .collect()
        };

        if wallet_refs.is_empty() {
            warn!("No wallets found in memory for sync");
            return Ok(());
        }

        // Create semaphore to limit concurrent syncs
        let semaphore = Arc::new(Semaphore::new(MAX_PARALLEL_SYNCS));

        // Prepare shared resources
        let metadata_db = self.metadata_db.clone();
        let notification_sender = self.notification_sender.clone();
        let electrum_manager = self.electrum_client_manager.clone();
        let config = self.config.clone();

        // Create parallel sync tasks using in-memory wallets
        let sync_tasks: Vec<_> = wallet_refs
            .into_iter()
            .map(|(wallet_metadata, wallet_arc)| {
                let semaphore = semaphore.clone();
                let metadata_db = metadata_db.clone();
                let notification_sender = notification_sender.clone();
                let electrum_manager = electrum_manager.clone();
                let config = config.clone();

                tokio::spawn(async move {
                    // Acquire semaphore permit
                    let _permit = semaphore
                        .acquire()
                        .await
                        .map_err(|e| anyhow!("Failed to acquire semaphore: {}", e))?;

                    let wallet_start = Instant::now();

                    // Lock the specific wallet for sync
                    let mut wallet_data = wallet_arc.lock().await;
                    let (wallet, conn) = &mut *wallet_data;

                    // Create sync service with config for mode-based retry logic
                    let metadata_db_ref = metadata_db.clone();
                    let sync_service =
                        WalletSyncService::new(metadata_db, notification_sender, config);

                    // Perform sync
                    match sync_service
                        .sync_wallet_by_checksum(
                            wallet,
                            &wallet_metadata.checksum,
                            electrum_manager.as_deref(),
                        )
                        .await
                    {
                        Ok(DescriptorWalletSyncResult::Completed) => {
                            // A successful Electrum sync can stage BDK state even when Canary's
                            // transaction reconciliation found no app-level changes.
                            wallet.persist(conn).map_err(|e| {
                                anyhow!(
                                    "Failed to persist wallet {} after sync: {}",
                                    wallet_metadata.name,
                                    e
                                )
                            })?;

                            // Promote pending wallet to ready after first successful sync
                            if wallet_metadata.status == "pending" {
                                if let Err(e) = metadata_db_ref
                                    .update_wallet_status_if_not_deleted(
                                        &wallet_metadata.checksum,
                                        "ready",
                                    )
                                    .await
                                {
                                    warn!(
                                        "Failed to promote wallet {} to ready: {}",
                                        wallet_metadata.name, e
                                    );
                                }
                            }

                            let sync_duration = wallet_start.elapsed();
                            debug!(
                                "✅ Synced wallet {} in {:.2}s (from memory)",
                                wallet_metadata.name,
                                sync_duration.as_secs_f64()
                            );
                            Ok(wallet_metadata.name)
                        }
                        Ok(DescriptorWalletSyncResult::SkippedNoClient) => {
                            // No Electrum client available, skip
                            Ok(wallet_metadata.name)
                        }
                        Err(e) => {
                            warn!("Failed to sync wallet {}: {}", wallet_metadata.name, e);
                            Err(e)
                        }
                    }
                })
            })
            .collect();

        // Wait for all sync tasks to complete
        let results = join_all(sync_tasks).await;

        // Count successes and failures
        let mut success_count = 0;
        let mut failure_count = 0;

        for result in results {
            match result {
                Ok(Ok(_)) => success_count += 1,
                _ => failure_count += 1,
            }
        }

        let total_duration = sync_start.elapsed();
        debug!(
            "🏁 Parallel in-memory sync completed in {:.2}s - Success: {}, Failed: {}",
            total_duration.as_secs_f64(),
            success_count,
            failure_count
        );

        Ok(())
    }

    /// Apply subscription tier limits by setting is_active status on wallets and contacts
    pub async fn apply_subscription_limits(
        &self,
        user_id: &str,
        tier: &str,
        subscription_status: &str,
        is_admin: bool,
        trial_ends_at: Option<String>,
        subscription_ends_at: Option<String>,
    ) -> Result<(), anyhow::Error> {
        // Check if subscription has expired or failed payment
        let is_subscription_active = crate::subscription::is_subscription_active(
            subscription_status,
            trial_ends_at.as_deref(),
            subscription_ends_at.as_deref(),
        );

        if is_admin {
            tracing::info!("🎯 Applying unlimited limits for admin user {}", user_id);
        } else if !is_subscription_active {
            tracing::info!(
                "🎯 Deactivating all wallets for user {} (status: {})",
                user_id,
                subscription_status
            );
        } else {
            tracing::info!(
                "🎯 Applying {} tier limits for user {} (status: {})",
                tier,
                user_id,
                subscription_status
            );
        }

        // Get all wallets for this user ordered by creation time (oldest first)
        let wallets = self
            .metadata_db
            .get_wallets_for_user_oldest_first(user_id)
            .await?;

        // Determine wallet limit based on subscription status, tier, and admin status
        let wallet_limit = if is_admin {
            usize::MAX // Unlimited for admin
        } else if !is_subscription_active {
            0 // No active wallets for inactive subscriptions (expired, past_due, or canceled with no remaining access)
        } else {
            match tier {
                "personal" => 1,
                "team" => 5,
                _ => 1, // Default to personal limits for unknown tiers
            }
        };

        // Apply wallet limits. Failed wallets are recoverable records, not active
        // subscriptions slots, so keep them inactive and skip them when counting.
        let mut active_wallet_count = 0;
        let mut non_failed_wallet_count = 0;
        for wallet in &wallets {
            let (should_be_active, wallet_position) =
                crate::subscription::wallet_active_limit_decision(
                    &wallet.status,
                    wallet_limit,
                    &mut active_wallet_count,
                    &mut non_failed_wallet_count,
                );

            // Update is_active status only if it changed
            if wallet.is_active != should_be_active {
                if let Err(e) = self
                    .metadata_db
                    .update_wallet_active_status(&wallet.checksum, should_be_active)
                    .await
                {
                    tracing::error!(
                        "Failed to set wallet {} active status to {}: {}",
                        wallet.checksum,
                        should_be_active,
                        e
                    );
                } else {
                    if wallet.status == "failed" {
                        tracing::info!(
                            "Set wallet {} active status to false (wallet is in failed state)",
                            wallet.checksum
                        );
                    } else {
                        tracing::info!(
                            "Set wallet {} active status to {} (position: {})",
                            wallet.checksum,
                            should_be_active,
                            wallet_position.expect("non-failed wallet must have a position")
                        );
                    }
                }
            }
        }

        // For active wallets, apply contact limits
        let contact_limit = if is_admin {
            usize::MAX // Unlimited for admin
        } else if !is_subscription_active {
            0 // No active contacts for inactive subscriptions (expired, past_due, or canceled with no remaining access)
        } else {
            match tier {
                "personal" => 1,
                "team" => 5,
                _ => 1, // Default to personal limits
            }
        };

        for wallet in wallets.iter() {
            if wallet.is_active {
                // Get contacts ordered by creation time (oldest first)
                let contacts = self
                    .metadata_db
                    .get_contacts_oldest_first_for_limits(&wallet.checksum)
                    .await?;

                // Apply contact limits
                for (i, contact) in contacts.iter().enumerate() {
                    let should_be_active = i < contact_limit;

                    // Update is_active status only if it changed
                    if contact.is_active != should_be_active {
                        if let Some(contact_id) = &contact.id {
                            if let Err(e) = self
                                .metadata_db
                                .update_contact_active_status(contact_id, should_be_active)
                                .await
                            {
                                tracing::error!(
                                    "Failed to set contact {} active status to {}: {}",
                                    contact_id,
                                    should_be_active,
                                    e
                                );
                            } else {
                                tracing::info!(
                                    "Set contact {} active status to {} (position: {})",
                                    contact.name,
                                    should_be_active,
                                    i + 1
                                );
                            }
                        }
                    }
                }
            }
        }

        Ok(())
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
            if wallet_metadata.wallet_type == "address" {
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

    /// Load a single wallet from disk
    /// Returns the wallet and its database connection
    async fn load_wallet_from_disk(
        wallet_path: &Path,
        expected_descriptor: &str,
        network: Network,
    ) -> Result<(PersistedWallet<Connection>, Connection)> {
        // Run blocking I/O in a separate thread
        let path = wallet_path.to_path_buf();
        let expected_descriptor = expected_descriptor.to_string();
        let result = tokio::task::spawn_blocking(move || {
            let conn = Connection::open(&path)
                .map_err(|e| anyhow!("Failed to open wallet database: {}", e))?;

            let mut conn = conn;
            let wallet = Wallet::load()
                .two_path_descriptor(expected_descriptor)
                .check_network(network)
                .load_wallet(&mut conn)
                .map_err(|e| anyhow!("Failed to load wallet from database: {}", e))?;

            let wallet_opt = wallet.ok_or_else(|| anyhow!("No wallet data found in file"))?;
            Ok::<(PersistedWallet<Connection>, Connection), anyhow::Error>((wallet_opt, conn))
        })
        .await??;

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bdk_wallet::rusqlite::OptionalExtension;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const TWO_PATH_DESCRIPTOR: &str = "wpkh(tpubDDnGNapGEY6AZAdQbfRJgMg9fvz8pUBrLwvyvUqEgcUfgzM6zc2eVK4vY9x9L5FJWdX8WumXuLEDV5zDZnTfbn87vLe9XceCFwTu9so9Kks/<0;1>/*)";
    const THREE_PATH_DESCRIPTOR: &str = "wpkh(tpubDDnGNapGEY6AZAdQbfRJgMg9fvz8pUBrLwvyvUqEgcUfgzM6zc2eVK4vY9x9L5FJWdX8WumXuLEDV5zDZnTfbn87vLe9XceCFwTu9so9Kks/<0;1;2>/*)";

    #[cfg(unix)]
    #[test]
    fn restrict_permissions_sets_owner_only_modes() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("wallet.sqlite");
        std::fs::File::create(&file_path).unwrap();

        restrict_permissions(temp_dir.path(), 0o700).unwrap();
        restrict_permissions(&file_path, 0o600).unwrap();

        assert_eq!(
            std::fs::metadata(temp_dir.path())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&file_path).unwrap().permissions().mode() & 0o777,
            0o600
        );

        assert!(restrict_permissions(&temp_dir.path().join("missing"), 0o600).is_err());
    }

    fn queue_wallet(checksum: &str, last_synced_at: Option<&str>) -> WalletMetadata {
        WalletMetadata {
            checksum: checksum.to_string(),
            name: checksum.to_string(),
            descriptor: format!("descriptor-{checksum}"),
            hex_color: "#000000".to_string(),
            created_at: "2026-08-10 00:00:00".to_string(),
            balance_total: Some(0),
            last_activity: None,
            status: "ready".to_string(),
            contact_count: None,
            user_id: "foss-user".to_string(),
            is_active: true,
            balance_fiat: None,
            fiat_currency: None,
            wallet_type: "descriptor".to_string(),
            last_synced_at: last_synced_at.map(str::to_string),
        }
    }

    #[test]
    fn pending_address_watches_keep_shared_subscriptions_active() {
        let mut ready = queue_wallet("ready", None);
        ready.wallet_type = "address".to_string();
        ready.descriptor = "addr(ready-address)".to_string();

        let mut pending = queue_wallet("pending", None);
        pending.wallet_type = "address".to_string();
        pending.status = "pending".to_string();
        pending.descriptor = "addr(shared-address)".to_string();

        let mut deleted = queue_wallet("deleted", None);
        deleted.wallet_type = "address".to_string();
        deleted.status = "deleted".to_string();
        deleted.descriptor = "addr(shared-address)".to_string();

        let mut deleted_only = queue_wallet("deleted-only", None);
        deleted_only.wallet_type = "address".to_string();
        deleted_only.status = "deleted".to_string();
        deleted_only.descriptor = "addr(deleted-address)".to_string();

        let wallets = [ready, pending, deleted, deleted_only];
        let active = active_address_watch_descriptors(&wallets);

        assert!(active.contains("addr(ready-address)"));
        assert!(active.contains("addr(shared-address)"));
        assert!(!active.contains("addr(deleted-address)"));
    }

    #[test]
    fn mixed_address_watch_group_suppresses_only_pending_watcher() {
        let mut ready = queue_wallet("ready", None);
        ready.wallet_type = "address".to_string();
        ready.descriptor = "addr(shared-address)".to_string();

        let mut pending = ready.clone();
        pending.checksum = "pending".to_string();
        pending.status = "pending".to_string();

        assert_eq!(
            address_watch_sync_targets(&[ready, pending]),
            vec![("ready".to_string(), false), ("pending".to_string(), true)]
        );
    }

    #[test]
    fn self_hosted_queue_orders_never_synced_then_oldest_with_stable_tie_breaker() {
        let queue = build_self_hosted_queue(vec![
            queue_wallet("new-b", None),
            queue_wallet("recent", Some("2026-08-10 12:00:00.000")),
            queue_wallet("old-b", Some("2026-08-10 10:00:00.000")),
            queue_wallet("new-a", None),
            queue_wallet("old-a", Some("2026-08-10 10:00:00.000")),
        ]);
        let keys: Vec<_> = queue.iter().map(|item| item.key.as_str()).collect();
        assert_eq!(keys, ["new-a", "new-b", "old-a", "old-b", "recent"]);
    }

    #[test]
    fn self_hosted_queue_scales_oldest_first_for_reported_wallet_counts() {
        for count in [1usize, 9, 20] {
            let wallets = (0..count)
                .rev()
                .map(|index| {
                    queue_wallet(
                        &format!("wallet-{index:02}"),
                        Some(&format!("2026-08-10 10:{index:02}:00.000")),
                    )
                })
                .collect();
            let queue = build_self_hosted_queue(wallets);
            let keys: Vec<_> = queue.iter().map(|item| item.key.clone()).collect();
            let expected: Vec<_> = (0..count)
                .map(|index| format!("wallet-{index:02}"))
                .collect();
            assert_eq!(keys, expected);
        }
    }

    #[test]
    fn self_hosted_due_time_is_measured_from_completion() {
        let completed = queue_wallet("wallet", Some("2026-08-10 10:00:30.000"));
        let item = build_self_hosted_queue(vec![completed]).remove(0);
        let now = parse_last_synced_at("2026-08-10 10:00:45.000").unwrap();
        assert_eq!(
            item.due_at(Duration::from_secs(60), now),
            parse_last_synced_at("2026-08-10 10:01:30.000").unwrap()
        );
    }

    #[test]
    fn recovery_scan_gap_uses_bdk_full_scan_depths() {
        assert_eq!(recovery_scan_gap(true, Some("auto")), 20);
        assert_eq!(recovery_scan_gap(false, Some("auto")), 500);
        assert_eq!(recovery_scan_gap(false, Some("invalid")), 500);
        for selected in [250usize, 500, 750, 1000] {
            assert_eq!(
                recovery_scan_gap(false, Some(&selected.to_string())),
                selected
            );
        }
    }

    #[test]
    fn deferred_failure_does_not_starve_healthy_wallets() {
        let queue = build_self_hosted_queue(vec![
            queue_wallet("failed-oldest", None),
            queue_wallet("healthy", None),
        ]);
        let now_wall = chrono::Utc::now();
        let now_instant = Instant::now();
        let mut deferred = HashMap::new();
        deferred.insert(
            "failed-oldest".to_string(),
            now_instant + Duration::from_secs(60),
        );

        let selected = select_due_self_hosted_item(
            &queue,
            Duration::from_secs(60),
            now_wall,
            now_instant,
            &deferred,
        )
        .unwrap();
        assert_eq!(selected.key, "healthy");
    }

    #[tokio::test]
    async fn self_hosted_work_gate_keeps_peak_concurrency_at_one() {
        let gate = Arc::new(Semaphore::new(1));
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();

        for _ in 0..20 {
            let gate = gate.clone();
            let active = active.clone();
            let peak = peak.clone();
            tasks.push(tokio::spawn(async move {
                let _permit = gate.acquire_owned().await.unwrap();
                let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(current, Ordering::SeqCst);
                tokio::task::yield_now().await;
                active.fetch_sub(1, Ordering::SeqCst);
            }));
        }
        for task in tasks {
            task.await.unwrap();
        }
        assert_eq!(peak.load(Ordering::SeqCst), 1);
    }

    fn create_persisted_test_wallet(path: &Path) {
        let mut conn = Connection::open(path).unwrap();
        let mut wallet = Wallet::create_from_two_path_descriptor(TWO_PATH_DESCRIPTOR)
            .network(Network::Regtest)
            .create_wallet(&mut conn)
            .unwrap();
        let _: Vec<_> = wallet
            .reveal_addresses_to(KeychainKind::External, 7)
            .collect();
        let _: Vec<_> = wallet
            .reveal_addresses_to(KeychainKind::Internal, 4)
            .collect();
        assert!(wallet.persist(&mut conn).unwrap());
    }

    #[test]
    fn bdk_two_path_creation_matches_canarys_former_split() {
        let (receive_descriptor, change_descriptor) =
            parse_multipath_descriptor(TWO_PATH_DESCRIPTOR).unwrap();
        let legacy_wallet = Wallet::create(receive_descriptor, change_descriptor)
            .network(Network::Regtest)
            .create_wallet_no_persist()
            .unwrap();
        let native_wallet = Wallet::create_from_two_path_descriptor(TWO_PATH_DESCRIPTOR)
            .network(Network::Regtest)
            .create_wallet_no_persist()
            .unwrap();

        for keychain in [KeychainKind::External, KeychainKind::Internal] {
            assert_eq!(
                legacy_wallet.public_descriptor(keychain),
                native_wallet.public_descriptor(keychain)
            );
            for index in [0, 1, 20] {
                assert_eq!(
                    legacy_wallet.peek_address(keychain, index).address,
                    native_wallet.peek_address(keychain, index).address
                );
            }
        }
    }

    #[test]
    fn bdk_two_path_creation_rejects_wrong_path_shapes() {
        let non_multipath = TWO_PATH_DESCRIPTOR.replace("<0;1>", "0");
        assert!(Wallet::create_from_two_path_descriptor(non_multipath)
            .network(Network::Regtest)
            .create_wallet_no_persist()
            .is_err());
        assert!(
            Wallet::create_from_two_path_descriptor(THREE_PATH_DESCRIPTOR)
                .network(Network::Regtest)
                .create_wallet_no_persist()
                .is_err()
        );

        let hardened_after_xpub = TWO_PATH_DESCRIPTOR.replace("/<0;1>/*", "/84h/<0;1>/*");
        assert!(parse_multipath_descriptor(&hardened_after_xpub).is_err());
    }

    #[tokio::test]
    async fn persisted_wallet_load_checks_descriptor_and_network() {
        let temp_dir = tempfile::tempdir().unwrap();
        let wallet_path = temp_dir.path().join("wallet.sqlite");
        create_persisted_test_wallet(&wallet_path);

        let (wallet, conn) = WalletManager::load_wallet_from_disk(
            &wallet_path,
            TWO_PATH_DESCRIPTOR,
            Network::Regtest,
        )
        .await
        .unwrap();
        assert_eq!(wallet.network(), Network::Regtest);
        drop(wallet);
        drop(conn);

        let mismatching_descriptor = TWO_PATH_DESCRIPTOR.replace("<0;1>", "<1;0>");
        assert!(WalletManager::load_wallet_from_disk(
            &wallet_path,
            &mismatching_descriptor,
            Network::Regtest,
        )
        .await
        .is_err());
        assert!(WalletManager::load_wallet_from_disk(
            &wallet_path,
            TWO_PATH_DESCRIPTOR,
            Network::Bitcoin,
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn bdk_v2_sqlite_wallet_migrates_and_reloads() {
        let temp_dir = tempfile::tempdir().unwrap();
        let wallet_path = temp_dir.path().join("wallet.sqlite");
        create_persisted_test_wallet(&wallet_path);

        // bdk_wallet 2.3 used wallet schema v0. Recreate that state from the otherwise
        // identical persisted data so this test remains self-contained.
        {
            let conn = Connection::open(&wallet_path).unwrap();
            conn.execute_batch(
                "DROP TABLE bdk_wallet_locked_outpoints;
                 UPDATE bdk_schemas SET version = 0 WHERE name = 'bdk_wallet';",
            )
            .unwrap();
            let lock_table: Option<String> = conn
                .query_row(
                    "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'bdk_wallet_locked_outpoints'",
                    [],
                    |row| row.get(0),
                )
                .optional()
                .unwrap();
            assert!(lock_table.is_none());
        }

        let (wallet, conn) = WalletManager::load_wallet_from_disk(
            &wallet_path,
            TWO_PATH_DESCRIPTOR,
            Network::Regtest,
        )
        .await
        .unwrap();

        assert_eq!(wallet.network(), Network::Regtest);
        assert_eq!(wallet.derivation_index(KeychainKind::External), Some(7));
        assert_eq!(wallet.derivation_index(KeychainKind::Internal), Some(4));
        let (receive_descriptor, change_descriptor) =
            parse_multipath_descriptor(TWO_PATH_DESCRIPTOR).unwrap();
        assert_eq!(
            wallet.public_descriptor(KeychainKind::External).to_string(),
            receive_descriptor
        );
        assert_eq!(
            wallet.public_descriptor(KeychainKind::Internal).to_string(),
            change_descriptor
        );
        assert_eq!(
            conn.query_row(
                "SELECT version FROM bdk_schemas WHERE name = 'bdk_wallet'",
                [],
                |row| row.get::<_, u32>(0),
            )
            .unwrap(),
            1
        );
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'bdk_wallet_locked_outpoints'",
                [],
                |row| row.get::<_, u32>(0),
            )
            .unwrap(),
            1
        );
        drop(wallet);
        drop(conn);

        let (reloaded, _conn) = WalletManager::load_wallet_from_disk(
            &wallet_path,
            TWO_PATH_DESCRIPTOR,
            Network::Regtest,
        )
        .await
        .unwrap();
        assert_eq!(reloaded.derivation_index(KeychainKind::External), Some(7));
        assert_eq!(reloaded.derivation_index(KeychainKind::Internal), Some(4));
    }
}
