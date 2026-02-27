use anyhow::{anyhow, Result};
use bdk_electrum::electrum_client::{self, ElectrumApi, GetBalanceRes, GetHistoryRes};
use bdk_electrum::BdkElectrumClient;
use bdk_wallet::bitcoin::{ScriptBuf, Transaction, Txid};
use bdk_wallet::chain::collections::HashSet;
use bdk_wallet::rusqlite::Connection;
use bdk_wallet::{KeychainKind, PersistedWallet};
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::task::spawn_blocking;
use tokio::time::timeout;
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeader {
    pub height: u32,
    pub timestamp: u64,
}

pub const STOP_GAP: usize = 20;
pub const BATCH_SIZE: usize = 20;
const PRIMARY_SYNC_TIMEOUT_SECS: u64 = 60;
const FULL_SCAN_TIMEOUT_SECS: u64 = 120;
const BLOCK_OP_TIMEOUT_SECS: u64 = 10;

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

        // Set a TCP socket timeout so that blocking reads/writes fail fast when the
        // Electrum server becomes unresponsive.  Without this, spawn_blocking tasks hold
        // the internal RawClient mutexes indefinitely, causing all subsequent Electrum
        // calls to queue up and cascade-timeout.
        let config = electrum_client::Config::builder()
            .timeout(Some(BLOCK_OP_TIMEOUT_SECS as u8))
            .build();
        let electrum_client = electrum_client::Client::from_config(url, config)?;
        let client = BdkElectrumClient::new(electrum_client);
        Ok(ElectrumClient {
            client: Arc::new(client),
        })
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

        debug!(
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

    /// Ensure we have at least the specified stop gap addresses revealed beyond the highest used index
    fn ensure_stop_gap_maintained(
        wallet: &mut PersistedWallet<Connection>,
        keychain: KeychainKind,
        stop_gap: usize,
    ) -> Result<bool> {
        let calc_start = Instant::now();
        let highest_used = Self::get_highest_used_index(wallet, keychain);
        let usage_scan_duration = calc_start.elapsed();
        let current_index = wallet.next_derivation_index(keychain);
        let required_index = highest_used + stop_gap as u32;

        if current_index <= required_index {
            let keychain_str = if keychain == KeychainKind::External {
                "external"
            } else {
                "internal"
            };
            debug!(
                "  Need more {} addresses: highest used={}, current revealed={}, need={} (scan {:.2?})",
                keychain_str,
                highest_used,
                current_index,
                required_index,
                usage_scan_duration
            );

            // Reveal addresses up to the required index
            let reveal_start = Instant::now();
            let revealed: Vec<_> = wallet
                .reveal_addresses_to(keychain, required_index)
                .collect();
            let reveal_duration = reveal_start.elapsed();
            debug!(
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
            debug!(
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
        info!("Full scanning with electrum...");

        // Print initial balance
        let balance_before = wallet.balance();
        info!("Wallet balance before syncing: {}", balance_before.total());

        let stop_gap = custom_stop_gap.unwrap_or(STOP_GAP);
        let initial_reveal: u32 = std::cmp::max(stop_gap as u32, 50);

        info!(
            "Initial address revelation (up to index {}):",
            initial_reveal
        );
        let ext_revealed: Vec<_> = wallet
            .reveal_addresses_to(KeychainKind::External, initial_reveal)
            .collect();
        info!("  Revealed {} external addresses", ext_revealed.len());

        let int_revealed: Vec<_> = wallet
            .reveal_addresses_to(KeychainKind::Internal, initial_reveal)
            .collect();
        info!("  Revealed {} internal addresses", int_revealed.len());

        // Loop until we've satisfied the stop gap for both keychains
        let mut scan_iteration = 0;
        loop {
            scan_iteration += 1;
            info!("\nScan iteration {}", scan_iteration);

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
            info!("Using stop gap: {}", stop_gap);
            let client = Arc::clone(&self.client);
            let update = timeout(
                Duration::from_secs(FULL_SCAN_TIMEOUT_SECS),
                spawn_blocking(move || client.full_scan(request, stop_gap, BATCH_SIZE, false)),
            )
            .await
            .map_err(|_| {
                anyhow!("Full scan operation timed out after {FULL_SCAN_TIMEOUT_SECS} seconds")
            })?
            .map_err(|e| anyhow!("Full scan task failed: {}", e))?
            .map_err(|e| anyhow!("Full scan failed: {}", e))?;

            println!(); // New line after scan progress

            // Apply the update
            wallet
                .apply_update(update)
                .map_err(|e| anyhow!("Failed to apply update: {}", e))?;

            // Check if we need to reveal more addresses to satisfy the stop gap
            let ext_needs_more =
                Self::ensure_stop_gap_maintained(wallet, KeychainKind::External, stop_gap)?;
            let int_needs_more =
                Self::ensure_stop_gap_maintained(wallet, KeychainKind::Internal, stop_gap)?;

            // If no new addresses were revealed, we're done
            if !ext_needs_more && !int_needs_more {
                info!("Stop gap satisfied for both keychains");
                break;
            }

            // Safety check to prevent infinite loops
            if scan_iteration > 10 {
                warn!("Warning: Reached maximum scan iterations");
                break;
            }
        }

        // Print final balance
        let balance_after = wallet.balance();
        info!("Wallet balance after syncing: {}", balance_after.total());

        // Print final address statistics
        let ext_total = wallet.next_derivation_index(KeychainKind::External);
        let int_total = wallet.next_derivation_index(KeychainKind::Internal);
        info!(
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
        debug!(
            "[electrum] populate_tx_cache completed in {:.2?}",
            cache_start.elapsed()
        );

        // Start sync request (only checks known addresses)
        let request = wallet.start_sync_with_revealed_spks();

        // Perform the sync with timeout protection to avoid indefinite hangs
        let electrum_sync_start = Instant::now();
        let client = Arc::clone(&self.client);
        let update = timeout(
            Duration::from_secs(PRIMARY_SYNC_TIMEOUT_SECS),
            spawn_blocking(move || client.sync(request, BATCH_SIZE, false)),
        )
        .await
        .map_err(|_| anyhow!("Sync operation timed out after {PRIMARY_SYNC_TIMEOUT_SECS} seconds"))?
        .map_err(|e| anyhow!("Sync task failed: {}", e))?
        .map_err(|e| anyhow!("Sync failed: {}", e))?;
        debug!(
            "[electrum] primary sync completed in {:.2?}",
            electrum_sync_start.elapsed()
        );

        // Apply the update
        let apply_start = Instant::now();
        wallet
            .apply_update(update)
            .map_err(|e| anyhow!("Failed to apply update: {}", e))?;
        debug!(
            "[electrum] apply_update completed in {:.2?}",
            apply_start.elapsed()
        );

        // After sync, check if we need to reveal more addresses to maintain stop gap
        let stop_gap_external =
            Self::ensure_stop_gap_maintained(wallet, KeychainKind::External, STOP_GAP)?;
        let stop_gap_internal =
            Self::ensure_stop_gap_maintained(wallet, KeychainKind::Internal, STOP_GAP)?;

        // If new addresses were revealed, we need to sync them too
        if stop_gap_external || stop_gap_internal {
            debug!("New addresses revealed, performing additional sync...");

            // Sync only the newly revealed addresses with timeout protection
            let request = wallet.start_sync_with_revealed_spks();
            let additional_sync_start = Instant::now();
            let client = Arc::clone(&self.client);
            let update = timeout(
                Duration::from_secs(PRIMARY_SYNC_TIMEOUT_SECS),
                spawn_blocking(move || client.sync(request, BATCH_SIZE, false)),
            )
            .await
            .map_err(|_| {
                anyhow!(
                    "Additional sync operation timed out after {PRIMARY_SYNC_TIMEOUT_SECS} seconds"
                )
            })?
            .map_err(|e| anyhow!("Additional sync task failed: {}", e))?
            .map_err(|e| anyhow!("Additional sync failed: {}", e))?;
            debug!(
                "[electrum] additional sync completed in {:.2?}",
                additional_sync_start.elapsed()
            );

            let additional_apply_start = Instant::now();
            wallet
                .apply_update(update)
                .map_err(|e| anyhow!("Failed to apply additional update: {}", e))?;
            debug!(
                "[electrum] additional apply_update completed in {:.2?}",
                additional_apply_start.elapsed()
            );
        }

        debug!(
            "[electrum] total Electrum client sync duration {:.2?}",
            total_start.elapsed()
        );

        Ok(())
    }

    pub async fn get_block_header(&self, height: u32) -> Result<BlockHeader> {
        let client = Arc::clone(&self.client);
        let header = timeout(
            Duration::from_secs(BLOCK_OP_TIMEOUT_SECS),
            spawn_blocking(move || client.inner.block_header(height as usize)),
        )
        .await
        .map_err(|_| {
            anyhow!("Get block header operation timed out after {BLOCK_OP_TIMEOUT_SECS} seconds")
        })?
        .map_err(|e| anyhow!("Get block header task failed: {}", e))?
        .map_err(|e| anyhow!("Failed to get block header for height {}: {}", height, e))?;

        Ok(BlockHeader {
            height,
            timestamp: header.time as u64,
        })
    }

    /// Get transaction history for a specific script pubkey (for address watches)
    pub async fn script_get_history(&self, script: &ScriptBuf) -> Result<Vec<GetHistoryRes>> {
        let client = Arc::clone(&self.client);
        let script = script.clone();
        let history = timeout(
            Duration::from_secs(BLOCK_OP_TIMEOUT_SECS),
            spawn_blocking(move || client.inner.script_get_history(&script)),
        )
        .await
        .map_err(|_| anyhow!("script_get_history timed out after {BLOCK_OP_TIMEOUT_SECS} seconds"))?
        .map_err(|e| anyhow!("script_get_history task failed: {}", e))?
        .map_err(|e| anyhow!("script_get_history failed: {}", e))?;

        Ok(history)
    }

    /// Get balance for a specific script pubkey (for address watches)
    pub async fn script_get_balance(&self, script: &ScriptBuf) -> Result<GetBalanceRes> {
        let client = Arc::clone(&self.client);
        let script = script.clone();
        let balance = timeout(
            Duration::from_secs(BLOCK_OP_TIMEOUT_SECS),
            spawn_blocking(move || client.inner.script_get_balance(&script)),
        )
        .await
        .map_err(|_| anyhow!("script_get_balance timed out after {BLOCK_OP_TIMEOUT_SECS} seconds"))?
        .map_err(|e| anyhow!("script_get_balance task failed: {}", e))?
        .map_err(|e| anyhow!("script_get_balance failed: {}", e))?;

        Ok(balance)
    }

    /// Get a full transaction by txid (for address watches)
    pub async fn transaction_get(&self, txid: &Txid) -> Result<Transaction> {
        let client = Arc::clone(&self.client);
        let txid = *txid;
        let tx = timeout(
            Duration::from_secs(BLOCK_OP_TIMEOUT_SECS),
            spawn_blocking(move || client.inner.transaction_get(&txid)),
        )
        .await
        .map_err(|_| anyhow!("transaction_get timed out after {BLOCK_OP_TIMEOUT_SECS} seconds"))?
        .map_err(|e| anyhow!("transaction_get task failed: {}", e))?
        .map_err(|e| anyhow!("transaction_get failed: {}", e))?;

        Ok(tx)
    }

    pub async fn get_current_block_height(&self) -> Result<u32> {
        let client = Arc::clone(&self.client);
        let height = timeout(Duration::from_secs(BLOCK_OP_TIMEOUT_SECS), spawn_blocking(move || {
            client.inner.block_headers_subscribe()
        }))
        .await
        .map_err(|_| anyhow!("Get current block height operation timed out after {BLOCK_OP_TIMEOUT_SECS} seconds"))?
        .map_err(|e| anyhow!("Get current block height task failed: {}", e))?
        .map_err(|e| anyhow!("Failed to get current block height: {}", e))?
        .height;
        Ok(height as u32)
    }
}

/// Manages Electrum client lifecycle with automatic reconnection support.
///
/// This manager wraps the ElectrumClient and provides:
/// - Automatic reconnection when the connection dies (broken pipe, etc.)
/// - Coordination across parallel sync tasks to prevent reconnection storms
/// - Tracking of consecutive failures for alerting
pub struct ElectrumClientManager {
    /// The current Electrum client (None if disconnected)
    client: RwLock<Option<ElectrumClient>>,
    /// URL for reconnection
    url: String,
    /// Flag to prevent concurrent reconnection attempts
    reconnecting: AtomicBool,
    /// Counter for consecutive reconnection failures (for alerting)
    consecutive_failures: AtomicU32,
    /// Flag to track if we've already sent an alert for current outage
    alert_sent: AtomicBool,
    /// Last error message for diagnostics
    last_error: RwLock<Option<String>>,
}

impl ElectrumClientManager {
    /// Create a new manager with initial connection attempt
    pub fn new(url: &str) -> Result<Self> {
        let client = ElectrumClient::new(url).ok();
        let has_client = client.is_some();

        let manager = Self {
            client: RwLock::new(client),
            url: url.to_string(),
            reconnecting: AtomicBool::new(false),
            consecutive_failures: AtomicU32::new(0),
            alert_sent: AtomicBool::new(false),
            last_error: RwLock::new(None),
        };

        if has_client {
            info!(
                "ElectrumClientManager: Initial connection successful to {}",
                url
            );
        } else {
            warn!(
                "ElectrumClientManager: Initial connection failed to {}, will retry on first sync",
                url
            );
        }

        Ok(manager)
    }

    /// Get a clone of the current client for operations
    pub async fn get_client(&self) -> Option<ElectrumClient> {
        self.client.read().await.clone()
    }

    /// Check if we have an active client (passive check - may return true for dead connections)
    pub async fn is_connected(&self) -> bool {
        // Mock instances always report as connected (for testing)
        if self.url == "mock://test" {
            return true;
        }

        self.client.read().await.is_some()
    }

    /// Actively verify connection by making a simple Electrum call.
    /// Returns true if the connection is actually working.
    /// This is more reliable than `is_connected()` but has latency (up to 3 second timeout).
    /// Handles both dead connections and unresponsive servers (e.g., during electrs "compressing").
    pub async fn verify_connection(&self) -> bool {
        // Mock instances always report as connected (for testing)
        if self.url == "mock://test" {
            return true;
        }

        let client = match self.get_client().await {
            Some(c) => c,
            None => return false,
        };

        // Try to get block height as a simple health check with a short timeout
        // Use 3 seconds instead of default 10 to keep API responses fast
        let client_arc = Arc::clone(&client.client);
        let task_handle = spawn_blocking(move || client_arc.inner.block_headers_subscribe());

        let result = timeout(Duration::from_secs(3), task_handle).await;

        match result {
            Ok(Ok(Ok(_))) => true,
            Ok(Ok(Err(e))) => {
                let error_msg = e.to_string();
                debug!(
                    "ElectrumClientManager: Connection verification failed: {}",
                    error_msg
                );
                if Self::is_transport_error(&error_msg) {
                    self.mark_disconnected(&error_msg).await;
                }
                false
            }
            Ok(Err(e)) => {
                debug!(
                    "ElectrumClientManager: Connection verification task failed: {}",
                    e
                );
                false
            }
            Err(_) => {
                // Note: The blocking task may continue running, but this is acceptable
                // because block_headers_subscribe will eventually complete or fail.
                // We can't abort spawn_blocking tasks, but we return immediately.
                debug!(
                    "ElectrumClientManager: Connection verification timed out (server may be compressing)"
                );
                false
            }
        }
    }

    /// Get the Electrum URL
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Get the number of consecutive reconnection failures
    pub fn get_consecutive_failures(&self) -> u32 {
        self.consecutive_failures.load(Ordering::SeqCst)
    }

    /// Check if an alert has been sent for the current outage
    pub fn has_alert_been_sent(&self) -> bool {
        self.alert_sent.load(Ordering::SeqCst)
    }

    /// Mark that an alert has been sent (called by notification system)
    pub fn mark_alert_sent(&self) {
        self.alert_sent.store(true, Ordering::SeqCst);
    }

    /// Atomically check if a reconnection notification should be sent.
    /// Returns true if an alert was previously sent (and atomically resets it to false).
    /// This prevents race conditions where multiple tasks could send duplicate notifications.
    pub fn should_send_reconnected_notification(&self) -> bool {
        self.alert_sent
            .compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    /// Attempt to reconnect. Only one reconnection attempt runs at a time.
    ///
    /// Returns:
    /// - Ok(true) if reconnection succeeded
    /// - Ok(false) if another task is already reconnecting
    /// - Err if reconnection failed
    pub async fn reconnect(&self) -> Result<bool> {
        // Try to acquire reconnection lock (only one task can reconnect)
        if self
            .reconnecting
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            // Another task is already reconnecting
            debug!("ElectrumClientManager: Reconnection already in progress by another task");
            return Ok(false);
        }

        // We have the reconnection lock
        info!(
            "ElectrumClientManager: Attempting reconnection to {}",
            self.url
        );

        let result = match ElectrumClient::new(&self.url) {
            Ok(new_client) => {
                // Update the client
                let mut client_guard = self.client.write().await;
                *client_guard = Some(new_client);
                drop(client_guard);

                // Reset failure tracking on success
                // Note: alert_sent is reset by should_send_reconnected_notification() to avoid race conditions
                self.consecutive_failures.store(0, Ordering::SeqCst);
                *self.last_error.write().await = None;

                info!(
                    "ElectrumClientManager: Successfully reconnected to Electrum server: {}",
                    self.url
                );
                Ok(true)
            }
            Err(e) => {
                let error_msg = e.to_string();
                *self.last_error.write().await = Some(error_msg.clone());

                // Increment failure counter
                let failures = self.consecutive_failures.fetch_add(1, Ordering::SeqCst) + 1;
                error!(
                    "ElectrumClientManager: Reconnection attempt {} failed for {}: {}",
                    failures, self.url, error_msg
                );

                Err(anyhow!("Reconnection failed: {}", error_msg))
            }
        };

        // Release reconnection lock
        self.reconnecting.store(false, Ordering::SeqCst);
        result
    }

    /// Mark connection as failed (clears the client to force reconnection on next use)
    pub async fn mark_disconnected(&self, error: &str) {
        warn!(
            "ElectrumClientManager: Marking connection as disconnected: {}",
            error
        );
        *self.client.write().await = None;
        *self.last_error.write().await = Some(error.to_string());
    }

    /// Check if an error message indicates a transport-level failure requiring reconnection.
    ///
    /// These errors indicate the TCP/TLS connection is broken and cannot be recovered
    /// without creating a new connection.
    pub fn is_transport_error(error_msg: &str) -> bool {
        let msg_lower = error_msg.to_lowercase();
        msg_lower.contains("broken pipe")
            || msg_lower.contains("connection reset")
            || msg_lower.contains("eof")
            || msg_lower.contains("unexpected end")
            || msg_lower.contains("connection closed")
            || msg_lower.contains("connection refused")
            || msg_lower.contains("not connected")
            || msg_lower.contains("write error")
            || msg_lower.contains("read error")
            || msg_lower.contains("stream closed")
            || msg_lower.contains("socket")
            || msg_lower.contains("i/o error")
            || msg_lower.contains("os error 32") // Broken pipe on Linux
            || msg_lower.contains("os error 104") // Connection reset by peer on Linux
    }

    /// Create a mock manager that reports as connected (for testing only).
    /// This creates a manager with no real connection but `is_connected()` returns true.
    pub fn new_mock_connected() -> Self {
        Self {
            client: RwLock::new(None),
            url: "mock://test".to_string(),
            reconnecting: AtomicBool::new(false),
            consecutive_failures: AtomicU32::new(0),
            alert_sent: AtomicBool::new(false),
            last_error: RwLock::new(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_transport_error() {
        // Should be detected as transport errors
        assert!(ElectrumClientManager::is_transport_error("Broken pipe"));
        assert!(ElectrumClientManager::is_transport_error(
            "broken pipe (os error 32)"
        ));
        assert!(ElectrumClientManager::is_transport_error(
            "connection reset by peer"
        ));
        assert!(ElectrumClientManager::is_transport_error("unexpected eof"));
        assert!(ElectrumClientManager::is_transport_error(
            "Connection closed"
        ));
        assert!(ElectrumClientManager::is_transport_error("stream closed"));
        assert!(ElectrumClientManager::is_transport_error("write error"));
        assert!(ElectrumClientManager::is_transport_error("I/O error"));
        assert!(ElectrumClientManager::is_transport_error("os error 32"));
        assert!(ElectrumClientManager::is_transport_error("os error 104"));

        // Should NOT be detected as transport errors
        assert!(!ElectrumClientManager::is_transport_error("timeout"));
        assert!(!ElectrumClientManager::is_transport_error("server error"));
        assert!(!ElectrumClientManager::is_transport_error(
            "invalid response"
        ));
        assert!(!ElectrumClientManager::is_transport_error("parse error"));
    }
}
