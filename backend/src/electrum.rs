use crate::electrum_history::{SharedHistoryCache, SubscriptionHistoryClient};
use anyhow::{anyhow, Result};
use bdk_electrum::electrum_client::{self, ElectrumApi, GetBalanceRes, GetHistoryRes};
use bdk_electrum::BdkElectrumClient;
use bdk_wallet::bitcoin::{ScriptBuf, Transaction, Txid};
use bdk_wallet::rusqlite::Connection;
use bdk_wallet::{KeychainKind, PersistedWallet};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};
use tokio::task::{spawn_blocking, JoinHandle};
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
const CANCELLATION_DRAIN_TIMEOUT_SECS: u64 = 45;

fn self_hosted_operation_gate(subscriptions_enabled: bool) -> Option<Arc<Mutex<()>>> {
    subscriptions_enabled.then(|| Arc::new(Mutex::new(())))
}

async fn await_blocking_with_timeout<T>(
    task: &mut JoinHandle<T>,
    deadline: Duration,
    operation: &str,
    update_cancellation: impl Fn(bool),
) -> Result<T>
where
    T: Send + 'static,
{
    match timeout(deadline, &mut *task).await {
        Ok(result) => result.map_err(|error| anyhow!("{operation} task failed: {error}")),
        Err(_) => {
            update_cancellation(false);
            warn!(
                "{operation} exceeded {:.2?}; cancellation requested, draining the blocking worker for at most {CANCELLATION_DRAIN_TIMEOUT_SECS}s",
                deadline
            );
            match timeout(
                Duration::from_secs(CANCELLATION_DRAIN_TIMEOUT_SECS),
                &mut *task,
            )
            .await
            {
                Ok(_) => Err(anyhow!(
                    "{operation} timed out after {} seconds",
                    deadline.as_secs()
                )),
                Err(_) => {
                    // Tokio cannot abort a spawn_blocking closure once it is running. The adapter
                    // checks cancellation between bounded socket RPCs; reaching this branch means
                    // the connection must be replaced before more work is attempted.
                    update_cancellation(true);
                    task.abort();
                    Err(anyhow!(
                        "{operation} timed out after {} seconds and its worker did not stop within {CANCELLATION_DRAIN_TIMEOUT_SECS} seconds; connection replacement required",
                        deadline.as_secs()
                    ))
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct ElectrumClient {
    client: Arc<BdkElectrumClient<SubscriptionHistoryClient<Arc<electrum_client::Client>>>>,
    polling_client: Arc<BdkElectrumClient<SubscriptionHistoryClient<Arc<electrum_client::Client>>>>,
    // Subscription-enabled self-hosted clients share one socket and cancellation state between
    // recurring and polling adapters. Serialize those operations at the client boundary so a
    // health/header lookup cannot interfere with another worker's cancellation. Cloud clients
    // keep their existing parallelism and do not allocate this gate.
    operation_gate: Option<Arc<Mutex<()>>>,
}

impl ElectrumClient {
    pub fn new(url: &str) -> Result<Self> {
        Self::new_with_history_cache(url, SharedHistoryCache::default(), false)
    }

    fn new_with_history_cache(
        url: &str,
        history_cache: SharedHistoryCache,
        subscriptions_enabled: bool,
    ) -> Result<Self> {
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
            .timeout(Some(Duration::from_secs(BLOCK_OP_TIMEOUT_SECS)))
            .build();
        let electrum_client = Arc::new(electrum_client::Client::from_config(url, config)?);

        // electrum-client negotiates this when opening the connection but does not expose the
        // stored value on its general Client wrapper. A diagnostic server.version call records
        // the same negotiated range without changing sync behavior.
        match electrum_client.raw_call(
            "server.version",
            [
                electrum_client::Param::String(format!("Canary {}", env!("CARGO_PKG_VERSION"))),
                electrum_client::Param::StringVec(vec!["1.4".to_string(), "1.6".to_string()]),
            ],
        ) {
            Ok(value) => match serde_json::from_value::<electrum_client::ServerVersionRes>(value) {
                Ok(version) => info!(
                    "Electrum connection negotiated protocol {} with {}",
                    version.protocol_version, version.server_software_version
                ),
                Err(error) => debug!("Could not decode Electrum protocol diagnostics: {error}"),
            },
            Err(error) => debug!("Could not query Electrum protocol diagnostics: {error}"),
        }

        let subscription_client = SubscriptionHistoryClient::new(
            Arc::clone(&electrum_client),
            history_cache,
            subscriptions_enabled,
        );
        let polling_client =
            SubscriptionHistoryClient::polling(electrum_client, &subscription_client);
        Ok(ElectrumClient {
            client: Arc::new(BdkElectrumClient::new(subscription_client)),
            polling_client: Arc::new(BdkElectrumClient::new(polling_client)),
            operation_gate: self_hosted_operation_gate(subscriptions_enabled),
        })
    }

    async fn acquire_operation(&self) -> Option<tokio::sync::MutexGuard<'_, ()>> {
        match &self.operation_gate {
            Some(gate) => Some(gate.lock().await),
            None => None,
        }
    }

    fn populate_caches(&self, wallet: &PersistedWallet<Connection>) {
        let txs: Vec<_> = wallet
            .tx_graph()
            .full_txs()
            .map(|tx_node| tx_node.tx.clone())
            .collect();
        self.client.populate_tx_cache(txs.iter().cloned());
        self.polling_client.populate_tx_cache(txs);

        let anchors: Vec<_> = wallet
            .tx_graph()
            .all_anchors()
            .iter()
            .map(|(txid, anchors)| (*txid, anchors.iter().cloned().collect::<Vec<_>>()))
            .collect();
        self.client.populate_anchor_cache(anchors.iter().cloned());
        self.polling_client.populate_anchor_cache(anchors);
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
        scan_gap: usize,
    ) -> Result<()> {
        let _operation = self.acquire_operation().await;
        info!("Full scanning with Electrum (scan gap: {scan_gap})...");

        // Print initial balance
        let balance_before = wallet.balance();
        info!("Wallet balance before syncing: {}", balance_before.total());

        // Full scans deliberately use the polling client: speculative recovery scripts should
        // not remain subscribed. BDK applies last_active_indices from this response, so no manual
        // pre-reveal or repeated deep-sync loop is needed.
        self.populate_caches(wallet);
        let request = wallet.start_full_scan();
        let client = Arc::clone(&self.polling_client);
        client
            .inner
            .start_work()
            .map_err(|error| anyhow!("Full scan cannot start: {error}"))?;
        let cancellation_client = Arc::clone(&client);
        let mut scan_task =
            spawn_blocking(move || client.full_scan(request, scan_gap, BATCH_SIZE, false));
        let scan_result = await_blocking_with_timeout(
            &mut scan_task,
            Duration::from_secs(FULL_SCAN_TIMEOUT_SECS),
            "Full scan operation",
            move |poison| cancellation_client.inner.update_timeout_state(poison),
        )
        .await?;
        let update = scan_result.map_err(|e| anyhow!("Full scan failed: {}", e))?;

        wallet
            .apply_update(update)
            .map_err(|e| anyhow!("Failed to apply update: {}", e))?;

        // Retain only Canary's normal lookahead beyond activity. This only grows reveal indices,
        // so legacy wallets that already persisted a deeper reveal are never shrunk.
        Self::ensure_stop_gap_maintained(wallet, KeychainKind::External, STOP_GAP)?;
        Self::ensure_stop_gap_maintained(wallet, KeychainKind::Internal, STOP_GAP)?;

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
        let _operation = self.acquire_operation().await;
        let total_start = Instant::now();

        // Populate the electrum client's transaction cache
        let cache_start = Instant::now();
        self.populate_caches(wallet);
        debug!(
            "[electrum] populate_tx_cache completed in {:.2?}",
            cache_start.elapsed()
        );

        // Start sync request (only checks known addresses)
        let request = wallet.start_sync_with_revealed_spks();

        // Perform the sync with timeout protection to avoid indefinite hangs
        let electrum_sync_start = Instant::now();
        let client = Arc::clone(&self.client);
        client
            .inner
            .start_work()
            .map_err(|error| anyhow!("Sync cannot start: {error}"))?;
        let cancellation_client = Arc::clone(&client);
        let mut sync_task = spawn_blocking(move || {
            client.inner.prepare_recurring_work();
            client.sync(request, BATCH_SIZE, false)
        });
        let sync_result = await_blocking_with_timeout(
            &mut sync_task,
            Duration::from_secs(PRIMARY_SYNC_TIMEOUT_SECS),
            "Sync operation",
            move |poison| cancellation_client.inner.update_timeout_state(poison),
        )
        .await?;
        let update = sync_result.map_err(|e| anyhow!("Sync failed: {}", e))?;
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
            client
                .inner
                .start_work()
                .map_err(|error| anyhow!("Additional sync cannot start: {error}"))?;
            let cancellation_client = Arc::clone(&client);
            let mut additional_task = spawn_blocking(move || {
                client.inner.prepare_recurring_work();
                client.sync(request, BATCH_SIZE, false)
            });
            let additional_result = await_blocking_with_timeout(
                &mut additional_task,
                Duration::from_secs(PRIMARY_SYNC_TIMEOUT_SECS),
                "Additional sync operation",
                move |poison| cancellation_client.inner.update_timeout_state(poison),
            )
            .await?;
            let update = additional_result.map_err(|e| anyhow!("Additional sync failed: {}", e))?;
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
        let _operation = self.acquire_operation().await;
        let client = Arc::clone(&self.client);
        client
            .inner
            .start_work()
            .map_err(|error| anyhow!("Block header lookup cannot start: {error}"))?;
        let cancellation_client = Arc::clone(&client);
        let mut header_task = spawn_blocking(move || client.inner.block_header(height as usize));
        let header = await_blocking_with_timeout(
            &mut header_task,
            Duration::from_secs(BLOCK_OP_TIMEOUT_SECS),
            "Get block header operation",
            move |poison| cancellation_client.inner.update_timeout_state(poison),
        )
        .await?
        .map_err(|e| anyhow!("Failed to get block header for height {}: {}", height, e))?;

        Ok(BlockHeader {
            height,
            timestamp: header.time as u64,
        })
    }

    /// Get transaction history for a specific script pubkey (for address watches)
    pub async fn script_get_history(&self, script: &ScriptBuf) -> Result<Vec<GetHistoryRes>> {
        let _operation = self.acquire_operation().await;
        let client = Arc::clone(&self.client);
        client
            .inner
            .start_work()
            .map_err(|error| anyhow!("script_get_history cannot start: {error}"))?;
        let cancellation_client = Arc::clone(&client);
        let script = script.clone();
        let mut history_task = spawn_blocking(move || {
            client.inner.prepare_recurring_work();
            client.inner.script_get_history(&script)
        });
        let history_result = await_blocking_with_timeout(
            &mut history_task,
            Duration::from_secs(BLOCK_OP_TIMEOUT_SECS),
            "script_get_history",
            move |poison| cancellation_client.inner.update_timeout_state(poison),
        )
        .await?;
        let history = history_result.map_err(|e| anyhow!("script_get_history failed: {}", e))?;

        Ok(history)
    }

    /// Get balance for a specific script pubkey (for address watches)
    pub async fn script_get_balance(&self, script: &ScriptBuf) -> Result<GetBalanceRes> {
        let _operation = self.acquire_operation().await;
        let client = Arc::clone(&self.client);
        client
            .inner
            .start_work()
            .map_err(|error| anyhow!("script_get_balance cannot start: {error}"))?;
        let cancellation_client = Arc::clone(&client);
        let script = script.clone();
        let mut balance_task = spawn_blocking(move || client.inner.script_get_balance(&script));
        let balance_result = await_blocking_with_timeout(
            &mut balance_task,
            Duration::from_secs(BLOCK_OP_TIMEOUT_SECS),
            "script_get_balance",
            move |poison| cancellation_client.inner.update_timeout_state(poison),
        )
        .await?;
        let balance = balance_result.map_err(|e| anyhow!("script_get_balance failed: {}", e))?;

        Ok(balance)
    }

    /// Get a full transaction by txid (for address watches)
    pub async fn transaction_get(&self, txid: &Txid) -> Result<Transaction> {
        let _operation = self.acquire_operation().await;
        let client = Arc::clone(&self.client);
        client
            .inner
            .start_work()
            .map_err(|error| anyhow!("transaction_get cannot start: {error}"))?;
        let cancellation_client = Arc::clone(&client);
        let txid = *txid;
        let mut transaction_task = spawn_blocking(move || client.inner.transaction_get(&txid));
        let transaction_result = await_blocking_with_timeout(
            &mut transaction_task,
            Duration::from_secs(BLOCK_OP_TIMEOUT_SECS),
            "transaction_get",
            move |poison| cancellation_client.inner.update_timeout_state(poison),
        )
        .await?;
        let tx = transaction_result.map_err(|e| anyhow!("transaction_get failed: {}", e))?;

        Ok(tx)
    }

    /// Remove recurring-history subscriptions and cached histories for scripts no longer owned by
    /// an active wallet. Callers serialize this with other self-hosted Electrum work.
    async fn forget_scripts(&self, scripts: Vec<ScriptBuf>) -> Result<()> {
        if scripts.is_empty() {
            return Ok(());
        }
        let _operation = self.acquire_operation().await;
        let client = Arc::clone(&self.client);
        spawn_blocking(move || client.inner.forget_scripts(&scripts))
            .await
            .map_err(|error| anyhow!("Electrum subscription cleanup task failed: {error}"))?;
        Ok(())
    }

    pub async fn get_current_block_height(&self) -> Result<u32> {
        let _operation = self.acquire_operation().await;
        let client = Arc::clone(&self.client);
        client
            .inner
            .start_work()
            .map_err(|error| anyhow!("Block height lookup cannot start: {error}"))?;
        let cancellation_client = Arc::clone(&client);
        let mut height_task = spawn_blocking(move || client.inner.block_headers_subscribe());
        let height = await_blocking_with_timeout(
            &mut height_task,
            Duration::from_secs(BLOCK_OP_TIMEOUT_SECS),
            "Get current block height operation",
            move |poison| cancellation_client.inner.update_timeout_state(poison),
        )
        .await?
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
    history_cache: SharedHistoryCache,
    subscriptions_enabled: bool,
}

impl ElectrumClientManager {
    /// Create a new manager with initial connection attempt
    pub fn new(url: &str) -> Result<Self> {
        Self::new_with_subscriptions(url, false)
    }

    pub(crate) fn new_with_subscriptions(url: &str, subscriptions_enabled: bool) -> Result<Self> {
        let history_cache = SharedHistoryCache::default();
        let client = ElectrumClient::new_with_history_cache(
            url,
            history_cache.clone(),
            subscriptions_enabled,
        )
        .ok();
        let has_client = client.is_some();

        let manager = Self {
            client: RwLock::new(client),
            url: url.to_string(),
            reconnecting: AtomicBool::new(false),
            consecutive_failures: AtomicU32::new(0),
            alert_sent: AtomicBool::new(false),
            last_error: RwLock::new(None),
            history_cache,
            subscriptions_enabled,
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

    /// Forget scripts after wallet deletion even if the socket is currently disconnected.
    pub async fn forget_scripts(&self, scripts: Vec<ScriptBuf>) -> Result<()> {
        if let Some(client) = self.get_client().await {
            if let Err(error) = client.forget_scripts(scripts.clone()).await {
                self.history_cache.forget_scripts(&scripts);
                return Err(error);
            }
        } else {
            self.history_cache.forget_scripts(&scripts);
        }
        Ok(())
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

        // Do not contend with active sync work. A client operation already holding the gate owns
        // the shared socket and cancellation state; its normal timeout/reconnect path is the
        // authoritative health signal.
        let _operation = match &client.operation_gate {
            Some(gate) => match gate.try_lock() {
                Ok(operation) => Some(operation),
                Err(_) => {
                    debug!("ElectrumClientManager: Skipping active verification while Electrum work is in progress");
                    return true;
                }
            },
            None => None,
        };

        // Try to get block height as a simple health check with a short timeout. This uses the
        // same cooperative cancellation path as every other operation so a timed-out blocking
        // worker cannot outlive the operation gate unnoticed.
        let client_arc = Arc::clone(&client.client);
        if let Err(error) = client_arc.inner.start_work() {
            debug!("ElectrumClientManager: Connection verification cannot start: {error}");
            return false;
        }
        let cancellation_client = Arc::clone(&client_arc);
        let mut task_handle = spawn_blocking(move || client_arc.inner.block_headers_subscribe());
        let result = await_blocking_with_timeout(
            &mut task_handle,
            Duration::from_secs(3),
            "Connection verification",
            move |poison| cancellation_client.inner.update_timeout_state(poison),
        )
        .await;

        match result {
            Ok(Ok(_)) => true,
            Ok(Err(e)) => {
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
            Err(error) => {
                let error_msg = error.to_string();
                debug!("ElectrumClientManager: Connection verification failed: {error_msg}");
                if Self::is_transport_error(&error_msg) {
                    self.mark_disconnected(&error_msg).await;
                }
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

        let result = match ElectrumClient::new_with_history_cache(
            &self.url,
            self.history_cache.clone(),
            self.subscriptions_enabled,
        ) {
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
            || msg_lower.contains("connection replacement")
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
            history_cache: SharedHistoryCache::default(),
            subscriptions_enabled: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn timeout_cancels_worker_and_allows_work_gate_release() {
        let gate = Arc::new(tokio::sync::Semaphore::new(1));
        let permit = gate.clone().acquire_owned().await.unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let mut task = spawn_blocking(move || {
            while !worker_cancelled.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(1));
            }
            42
        });
        let cancel_signal = Arc::clone(&cancelled);
        let started = Instant::now();

        let error = await_blocking_with_timeout(
            &mut task,
            Duration::from_millis(10),
            "test operation",
            move |_| cancel_signal.store(true, Ordering::Release),
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("timed out"));
        assert!(cancelled.load(Ordering::Acquire));
        assert!(started.elapsed() < Duration::from_secs(1));
        drop(permit);
        assert!(timeout(Duration::from_secs(1), gate.acquire())
            .await
            .is_ok());
    }

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
        assert!(ElectrumClientManager::is_transport_error(
            "connection replacement required"
        ));

        // Should NOT be detected as transport errors
        assert!(!ElectrumClientManager::is_transport_error("timeout"));
        assert!(!ElectrumClientManager::is_transport_error("server error"));
        assert!(!ElectrumClientManager::is_transport_error(
            "invalid response"
        ));
        assert!(!ElectrumClientManager::is_transport_error("parse error"));
    }

    #[test]
    fn operation_gate_is_scoped_to_subscription_enabled_self_hosted_clients() {
        assert!(self_hosted_operation_gate(true).is_some());
        assert!(self_hosted_operation_gate(false).is_none());
    }
}
