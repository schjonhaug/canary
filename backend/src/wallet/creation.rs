//! Wallet creation: validating the submitted descriptor, xpub, address or
//! public key, saving its metadata and finishing the BDK setup and first scan
//! in the background.

use super::manager::WalletManager;
use super::persistence::{create_wallet_file, persist_wallet};
use crate::electrum::ElectrumClient;
use crate::metadata::{MetadataDb, WalletMetadata, WalletStatus, WalletType, extract_checksum};
use crate::sync::AddressWatchSyncResult;
use crate::utils::amount::sats_to_i64;
use crate::utils::{compact_wallet_key_input, parse_multipath_descriptor, strip_key_origin};
use anyhow::{Result, anyhow};
use bdk_wallet::bitcoin::Network;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::task::spawn_blocking;
use tracing::{debug, error, warn};

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

/// Generate a unique 8-character alphanumeric ID for use as a wallet checksum PK.
/// Used when multiple users watch the same address and need distinct wallet records.
fn generate_unique_wallet_id() -> String {
    use rand::RngExt;
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::rng();
    (0..8)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// Standalone wallet creation function that doesn't require `WalletManager` mutex
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

    /// Shared implementation for creating a single-script watch wallet (`addr()` or `pk()`).
    /// Uses [`WalletType::Address`] for both, since the sync dispatch path is the same:
    /// `script_from_watch_descriptor()` in `sync/address_watch.rs` handles resolving either descriptor
    /// type to the correct `ScriptBuf`.
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
                .map_or_else(|| extract_checksum(descriptor), |w| w.checksum.clone());
            return Err(anyhow!(
                "This script is already being watched with ID: {existing_checksum}."
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
                WalletType::Address,
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
                        .update_wallet_status_if_not_deleted(&checksum_clone, WalletStatus::Ready)
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
                        .update_wallet_status_if_not_deleted(&checksum_clone, WalletStatus::Failed)
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

    /// Create wallet without blocking `WalletManager`
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

        debug!("Creating wallet from multipath descriptor");

        let descriptor = compact_wallet_key_input(descriptor_str);
        let descriptor_str = descriptor.as_str();

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
            let checksum = extract_checksum(&normalized_descriptor);
            return Err(anyhow!(
                "This wallet has already been added with ID: {checksum}. Ask the wallet owner to add you as a contact for notifications."
            ));
        }

        // Parse and validate the normalized multipath descriptor
        parse_multipath_descriptor(&normalized_descriptor)?;

        // Extract checksum from the normalized descriptor for consistent filename
        let checksum = extract_checksum(&normalized_descriptor);
        let wallet_filename_with_ext = format!("{checksum}.sqlite");
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
            stop_gap: stop_gap.map(std::string::ToString::to_string),
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
            _ => return Err(anyhow!("Invalid script type: {script_type_str}")),
        };

        // Create converter and generate descriptor
        let converter = XpubConverter::new(self.network, self.electrum_client.as_ref());
        let descriptor = converter.generate_descriptor_for_type(xpub, script_type)?;

        // Strip key origin for consistency
        let normalized_descriptor = strip_key_origin(&descriptor)?;

        // Check if this descriptor already exists
        if self
            .metadata_db
            .descriptor_exists(&normalized_descriptor)
            .await?
        {
            let checksum = extract_checksum(&normalized_descriptor);
            return Err(anyhow!(
                "This wallet has already been added with ID: {checksum}. Ask the wallet owner to add you as a contact for notifications."
            ));
        }

        // Parse multipath descriptor
        parse_multipath_descriptor(&normalized_descriptor)?;

        // Extract checksum
        let checksum = extract_checksum(&normalized_descriptor);
        let wallet_filename_with_ext = format!("{checksum}.sqlite");
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
            stop_gap: stop_gap.map(std::string::ToString::to_string),
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
                    .update_wallet_status_if_not_deleted(&wallet_checksum, WalletStatus::Failed)
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

impl WalletManager {
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

        let wallet_path = ctx.wallet_path.clone();
        let descriptor = ctx.descriptor.clone();
        let network = ctx.network;
        // Boxed so `persist_wallet` can move it to the blocking pool and back.
        let mut state = Box::new(
            spawn_blocking(move || create_wallet_file(&wallet_path, descriptor, network)).await??,
        );

        // One BDK full scan owns discovery. Advanced selections are stop gaps (consecutive
        // unused scripts), existing-wallet automatic recovery uses gap 500, and fresh wallets
        // keep the normal 20-script gap.
        if let Some(ref client) = ctx.electrum_client {
            let scan_gap = recovery_scan_gap(ctx.is_fresh_wallet, stop_gap);

            let _work_permit = wallet_manager.acquire_electrum_work().await?;
            if let Err(e) = client.full_scan_wallet(&mut state.0, scan_gap).await {
                return Err(anyhow!(
                    "[{}] Failed to full scan wallet during background creation: {}",
                    ctx.checksum,
                    e
                ));
            }

            // Persisting the scanned BDK state is part of initial creation. If this
            // fails, the UI should expose the terminal v1 recovery path: delete and
            // add the wallet again instead of presenting an unsaved wallet as ready.
            state = persist_wallet(state).await.map_err(|e| {
                anyhow!(
                    "[{}] Failed to persist wallet after sync: {}",
                    ctx.checksum,
                    e
                )
            })?;
        }
        let wallet = &state.0;

        // Update balance in metadata database
        let balance = sats_to_i64(wallet.balance().total().to_sat());
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
                wallet,
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
            .update_wallet_status_if_not_deleted(&ctx.checksum, WalletStatus::Ready)
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
