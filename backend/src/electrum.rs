//! Electrum connectivity: the client that syncs wallets and serves lookups, and the manager
//! that replaces the connection after transport failures.

mod error;

pub use error::{ElectrumError, ErrorKind};

use crate::electrum_history::{SharedHistoryCache, SubscriptionHistoryClient};
use bdk_electrum::BdkElectrumClient;
use bdk_electrum::electrum_client::{self, ElectrumApi, GetBalanceRes, GetHistoryRes};
use bdk_wallet::bitcoin::{ScriptBuf, Transaction, Txid};
use bdk_wallet::rusqlite::Connection;
use bdk_wallet::{KeychainKind, PersistedWallet};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};
use tokio::task::{JoinHandle, spawn_blocking};
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
const SUBSCRIPTION_CLEANUP_TIMEOUT_SECS: u64 = 60;
const CANCELLATION_DRAIN_TIMEOUT_SECS: u64 = 45;
const VERIFY_CONNECTION_TIMEOUT_SECS: u64 = 3;

/// BDK client over the subscription-aware adapter that owns cancellation state.
type AdapterClient = BdkElectrumClient<SubscriptionHistoryClient<Arc<electrum_client::Client>>>;

fn self_hosted_operation_gate(subscriptions_enabled: bool) -> Option<Arc<Mutex<()>>> {
    subscriptions_enabled.then(|| Arc::new(Mutex::new(())))
}

async fn await_blocking_with_timeout<T>(
    task: &mut JoinHandle<T>,
    deadline: Duration,
    operation: &'static str,
    update_cancellation: impl Fn(bool),
) -> Result<T, ElectrumError>
where
    T: Send + 'static,
{
    if let Ok(result) = timeout(deadline, &mut *task).await {
        result.map_err(|source| ElectrumError::TaskFailed { operation, source })
    } else {
        update_cancellation(false);
        warn!(
            "{operation} exceeded {:.2?}; cancellation requested, draining the blocking worker for at most {CANCELLATION_DRAIN_TIMEOUT_SECS}s",
            deadline
        );
        let secs = deadline.as_secs();
        if timeout(
            Duration::from_secs(CANCELLATION_DRAIN_TIMEOUT_SECS),
            &mut *task,
        )
        .await
        .is_ok()
        {
            Err(ElectrumError::Timeout { operation, secs })
        } else {
            // Tokio cannot abort a spawn_blocking closure once it is running. The adapter
            // checks cancellation between bounded socket RPCs; reaching this branch means
            // the connection must be replaced before more work is attempted.
            update_cancellation(true);
            task.abort();
            Err(ElectrumError::WorkerStuck { operation, secs })
        }
    }
}

/// Runs `work` on a blocking thread with cooperative cancellation after `deadline`.
async fn run_blocking<T>(
    client: &Arc<AdapterClient>,
    operation: &'static str,
    deadline: Duration,
    work: impl FnOnce(&AdapterClient) -> Result<T, electrum_client::Error> + Send + 'static,
) -> Result<T, ElectrumError>
where
    T: Send + 'static,
{
    client
        .inner
        .start_work()
        .map_err(|_| ElectrumError::ConnectionPoisoned { operation })?;
    let worker = Arc::clone(client);
    let mut task = spawn_blocking(move || work(&worker));
    let cancellation = Arc::clone(client);
    await_blocking_with_timeout(&mut task, deadline, operation, move |poison| {
        cancellation.inner.update_timeout_state(poison);
    })
    .await?
    .map_err(|source| ElectrumError::Client { operation, source })
}

#[derive(Clone)]
pub struct ElectrumClient {
    client: Arc<AdapterClient>,
    polling_client: Arc<AdapterClient>,
    // Subscription-enabled self-hosted clients share one socket and cancellation state between
    // recurring and polling adapters. Serialize those operations at the client boundary so a
    // health/header lookup cannot interfere with another worker's cancellation. Cloud clients
    // keep their existing parallelism and do not allocate this gate.
    operation_gate: Option<Arc<Mutex<()>>>,
}

impl ElectrumClient {
    pub fn new(url: &str) -> Result<Self, ElectrumError> {
        Self::new_with_history_cache(url, SharedHistoryCache::default(), false)
    }

    fn new_with_history_cache(
        url: &str,
        history_cache: SharedHistoryCache,
        subscriptions_enabled: bool,
    ) -> Result<Self, ElectrumError> {
        // electrum_client::Client::new() handles both tcp:// and ssl:// schemes automatically
        if !url.starts_with("tcp://") && !url.starts_with("ssl://") {
            return Err(ElectrumError::UnsupportedScheme);
        }

        // Set a TCP socket timeout so that blocking reads/writes fail fast when the
        // Electrum server becomes unresponsive.  Without this, spawn_blocking tasks hold
        // the internal RawClient mutexes indefinitely, causing all subsequent Electrum
        // calls to queue up and cascade-timeout.
        let config = electrum_client::Config::builder()
            .timeout(Some(Duration::from_secs(BLOCK_OP_TIMEOUT_SECS)))
            .build();
        let electrum_client = Arc::new(
            electrum_client::Client::from_config(url, config)
                .map_err(|source| ElectrumError::Connect { source })?,
        );
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
            .map(|(txid, anchors)| (*txid, anchors.iter().copied().collect::<Vec<_>>()))
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
    ) -> bool {
        let calc_start = Instant::now();
        let highest_used = Self::get_highest_used_index(wallet, keychain);
        let usage_scan_duration = calc_start.elapsed();
        let current_index = wallet.next_derivation_index(keychain);
        let required_index =
            highest_used.saturating_add(u32::try_from(stop_gap).unwrap_or(u32::MAX));

        if current_index <= required_index {
            let keychain_str = if keychain == KeychainKind::External {
                "external"
            } else {
                "internal"
            };
            debug!(
                "  Need more {} addresses: highest used={}, current revealed={}, need={} (scan {:.2?})",
                keychain_str, highest_used, current_index, required_index, usage_scan_duration
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

            true // Addresses were revealed
        } else {
            let keychain_str = if keychain == KeychainKind::External {
                "external"
            } else {
                "internal"
            };
            debug!(
                "  Stop gap already satisfied for {} keychain (highest={}, current={}, required={}) after {:.2?}",
                keychain_str, highest_used, current_index, required_index, usage_scan_duration
            );
            false // No new addresses needed
        }
    }

    pub async fn full_scan_wallet(
        &self,
        wallet: &mut PersistedWallet<Connection>,
        scan_gap: usize,
    ) -> Result<(), ElectrumError> {
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
        let update = run_blocking(
            &self.polling_client,
            "Full scan",
            Duration::from_secs(FULL_SCAN_TIMEOUT_SECS),
            move |client| client.full_scan(request, scan_gap, BATCH_SIZE, false),
        )
        .await?;

        wallet
            .apply_update(update)
            .map_err(|source| ElectrumError::ApplyUpdate { source })?;

        // Retain only Canary's normal lookahead beyond activity. This only grows reveal indices,
        // so legacy wallets that already persisted a deeper reveal are never shrunk.
        Self::ensure_stop_gap_maintained(wallet, KeychainKind::External, STOP_GAP);
        Self::ensure_stop_gap_maintained(wallet, KeychainKind::Internal, STOP_GAP);

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

    pub async fn sync_wallet(
        &self,
        wallet: &mut PersistedWallet<Connection>,
    ) -> Result<(), ElectrumError> {
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
        let update = run_blocking(
            &self.client,
            "Sync",
            Duration::from_secs(PRIMARY_SYNC_TIMEOUT_SECS),
            move |client| {
                client.inner.prepare_recurring_work();
                client.sync(request, BATCH_SIZE, false)
            },
        )
        .await?;
        debug!(
            "[electrum] primary sync completed in {:.2?}",
            electrum_sync_start.elapsed()
        );

        // Apply the update
        let apply_start = Instant::now();
        wallet
            .apply_update(update)
            .map_err(|source| ElectrumError::ApplyUpdate { source })?;
        debug!(
            "[electrum] apply_update completed in {:.2?}",
            apply_start.elapsed()
        );

        // After sync, check if we need to reveal more addresses to maintain stop gap
        let stop_gap_external =
            Self::ensure_stop_gap_maintained(wallet, KeychainKind::External, STOP_GAP);
        let stop_gap_internal =
            Self::ensure_stop_gap_maintained(wallet, KeychainKind::Internal, STOP_GAP);

        // If new addresses were revealed, we need to sync them too
        if stop_gap_external || stop_gap_internal {
            debug!("New addresses revealed, performing additional sync...");

            // Sync only the newly revealed addresses with timeout protection
            let request = wallet.start_sync_with_revealed_spks();
            let additional_sync_start = Instant::now();
            let update = run_blocking(
                &self.client,
                "Additional sync",
                Duration::from_secs(PRIMARY_SYNC_TIMEOUT_SECS),
                move |client| {
                    client.inner.prepare_recurring_work();
                    client.sync(request, BATCH_SIZE, false)
                },
            )
            .await?;
            debug!(
                "[electrum] additional sync completed in {:.2?}",
                additional_sync_start.elapsed()
            );

            let additional_apply_start = Instant::now();
            wallet
                .apply_update(update)
                .map_err(|source| ElectrumError::ApplyUpdate { source })?;
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

    pub async fn get_block_header(&self, height: u32) -> Result<BlockHeader, ElectrumError> {
        let _operation = self.acquire_operation().await;
        let header = run_blocking(
            &self.client,
            "Block header lookup",
            Duration::from_secs(BLOCK_OP_TIMEOUT_SECS),
            move |client| client.inner.block_header(height as usize),
        )
        .await?;

        Ok(BlockHeader {
            height,
            timestamp: u64::from(header.time),
        })
    }

    /// Get transaction history for a specific script pubkey (for address watches)
    pub async fn script_get_history(
        &self,
        script: &ScriptBuf,
    ) -> Result<Vec<GetHistoryRes>, ElectrumError> {
        let _operation = self.acquire_operation().await;
        let script = script.clone();
        run_blocking(
            &self.client,
            "script_get_history",
            Duration::from_secs(BLOCK_OP_TIMEOUT_SECS),
            move |client| {
                client.inner.prepare_recurring_work();
                client.inner.script_get_history(&script)
            },
        )
        .await
    }

    /// Get balance for a specific script pubkey (for address watches)
    pub async fn script_get_balance(
        &self,
        script: &ScriptBuf,
    ) -> Result<GetBalanceRes, ElectrumError> {
        let _operation = self.acquire_operation().await;
        let script = script.clone();
        run_blocking(
            &self.client,
            "script_get_balance",
            Duration::from_secs(BLOCK_OP_TIMEOUT_SECS),
            move |client| client.inner.script_get_balance(&script),
        )
        .await
    }

    /// Get a full transaction by txid (for address watches)
    pub async fn transaction_get(&self, txid: &Txid) -> Result<Transaction, ElectrumError> {
        let _operation = self.acquire_operation().await;
        let txid = *txid;
        run_blocking(
            &self.client,
            "transaction_get",
            Duration::from_secs(BLOCK_OP_TIMEOUT_SECS),
            move |client| client.inner.transaction_get(&txid),
        )
        .await
    }

    /// Remove recurring-history subscriptions and cached histories for scripts no longer owned by
    /// an active wallet. Callers serialize this with other self-hosted Electrum work.
    ///
    /// Any failure is a [`ElectrumError::SubscriptionCleanup`]: the server may still hold some
    /// of the subscriptions, so the connection has to be replaced.
    async fn forget_scripts(&self, scripts: Vec<ScriptBuf>) -> Result<(), ElectrumError> {
        if scripts.is_empty() {
            return Ok(());
        }
        let _operation = self.acquire_operation().await;
        run_blocking(
            &self.client,
            "Electrum subscription cleanup",
            Duration::from_secs(SUBSCRIPTION_CLEANUP_TIMEOUT_SECS),
            move |client| client.inner.forget_scripts(&scripts),
        )
        .await
        .map_err(|source| ElectrumError::SubscriptionCleanup {
            source: Box::new(source),
        })
    }

    pub async fn get_current_block_height(&self) -> Result<u32, ElectrumError> {
        let _operation = self.acquire_operation().await;
        let height = run_blocking(
            &self.client,
            "Block height lookup",
            Duration::from_secs(BLOCK_OP_TIMEOUT_SECS),
            |client| client.inner.block_headers_subscribe(),
        )
        .await?
        .height;
        u32::try_from(height).map_err(|_| ElectrumError::InvalidHeight(height))
    }
}

/// Clears `ElectrumClientManager::reconnecting` when dropped, so a cancelled or panicking
/// reconnect attempt cannot block every later one.
struct ReconnectingGuard<'a>(&'a AtomicBool);

impl Drop for ReconnectingGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// Manages Electrum client lifecycle with automatic reconnection support.
///
/// This manager wraps the `ElectrumClient` and provides:
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
    pub fn new(url: &str) -> Self {
        Self::new_with_subscriptions(url, false)
    }

    pub(crate) fn new_with_subscriptions(url: &str, subscriptions_enabled: bool) -> Self {
        let history_cache = SharedHistoryCache::default();
        // Stays blocking: it runs once at startup, and an async constructor would ripple to callers.
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

        manager
    }

    /// Get a clone of the current client for operations
    pub async fn get_client(&self) -> Option<ElectrumClient> {
        self.client.read().await.clone()
    }

    /// Returns the current client, reconnecting once if the connection was lost.
    pub async fn client_or_reconnect(&self) -> Option<ElectrumClient> {
        if let Some(client) = self.get_client().await {
            return Some(client);
        }
        match self.reconnect().await {
            Ok(true) => self.get_client().await,
            _ => None,
        }
    }

    /// Forget scripts after wallet deletion even if the socket is currently disconnected.
    pub async fn forget_scripts(&self, scripts: Vec<ScriptBuf>) -> Result<(), ElectrumError> {
        if let Some(client) = self.get_client().await {
            if let Err(error) = client.forget_scripts(scripts.clone()).await {
                self.history_cache.forget_scripts(&scripts);
                if error.is_transport() {
                    self.mark_disconnected(&error.to_string()).await;
                }
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

        let Some(client) = self.get_client().await else {
            return false;
        };

        // Do not contend with active sync work. A client operation already holding the gate owns
        // the shared socket and cancellation state; its normal timeout/reconnect path is the
        // authoritative health signal.
        let _operation = match &client.operation_gate {
            Some(gate) => {
                if let Ok(operation) = gate.try_lock() {
                    Some(operation)
                } else {
                    debug!(
                        "ElectrumClientManager: Skipping active verification while Electrum work is in progress"
                    );
                    return true;
                }
            }
            None => None,
        };

        // Try to get block height as a simple health check with a short timeout. This uses the
        // same cooperative cancellation path as every other operation so a timed-out blocking
        // worker cannot outlive the operation gate unnoticed.
        let result = run_blocking(
            &client.client,
            "Connection verification",
            Duration::from_secs(VERIFY_CONNECTION_TIMEOUT_SECS),
            |client| client.inner.block_headers_subscribe(),
        )
        .await;

        match result {
            Ok(_) => true,
            Err(error) => {
                debug!("ElectrumClientManager: Connection verification failed: {error}");
                if error.is_transport() {
                    self.mark_disconnected(&error.to_string()).await;
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
    pub async fn reconnect(&self) -> Result<bool, ElectrumError> {
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

        // Clears the flag however this attempt ends, including cancellation of this future.
        let _reconnecting = ReconnectingGuard(&self.reconnecting);
        info!(
            "ElectrumClientManager: Attempting reconnection to {}",
            self.url
        );

        // Connecting does a blocking TCP/TLS handshake and a `server.version` round trip.
        let url = self.url.clone();
        let history_cache = self.history_cache.clone();
        let subscriptions_enabled = self.subscriptions_enabled;
        let connected = spawn_blocking(move || {
            ElectrumClient::new_with_history_cache(&url, history_cache, subscriptions_enabled)
        })
        .await
        .map_err(|source| ElectrumError::TaskFailed {
            operation: "Reconnect",
            source,
        })
        .and_then(|connected| connected);

        match connected {
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
                *self.last_error.write().await = Some(e.to_string());

                // Increment failure counter
                let failures = self.consecutive_failures.fetch_add(1, Ordering::SeqCst) + 1;
                error!(
                    "ElectrumClientManager: Reconnection attempt {} failed for {}: {}",
                    failures, self.url, e
                );

                Err(e)
            }
        }
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

    /// Drops the connection and reconnects when `error` is a transport failure.
    pub async fn reconnect_after_transport_error(&self, error: &ElectrumError) {
        if error.is_transport() {
            self.mark_disconnected(&error.to_string()).await;
            let _ = self.reconnect().await;
        }
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

        assert!(matches!(error, ElectrumError::Timeout { .. }));
        assert!(cancelled.load(Ordering::Acquire));
        assert!(started.elapsed() < Duration::from_secs(1));
        drop(permit);
        assert!(
            timeout(Duration::from_secs(1), gate.acquire())
                .await
                .is_ok()
        );
    }

    fn disconnected_manager(url: String) -> ElectrumClientManager {
        ElectrumClientManager {
            url,
            ..ElectrumClientManager::new_mock_connected()
        }
    }

    /// A local TCP address with nothing listening on it, so connecting is refused at once.
    fn refused_url() -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        format!("tcp://{addr}")
    }

    #[tokio::test]
    async fn failed_reconnect_releases_reconnecting_flag() {
        let manager = disconnected_manager(refused_url());

        assert!(manager.reconnect().await.is_err());
        assert!(!manager.reconnecting.load(Ordering::SeqCst));

        // A held flag would make this attempt report Ok(false) instead of trying again.
        assert!(manager.reconnect().await.is_err());
        assert_eq!(manager.get_consecutive_failures(), 2);
        assert!(!manager.reconnecting.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn cancelled_reconnect_releases_reconnecting_flag() {
        // The server accepts the connection but never answers `server.version`, so the
        // blocking connect is still running when the reconnect future is dropped.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let manager = disconnected_manager(format!("tcp://{}", listener.local_addr().unwrap()));

        let attempt = timeout(Duration::from_millis(100), manager.reconnect()).await;

        assert!(attempt.is_err(), "reconnect should still be waiting");
        assert!(!manager.reconnecting.load(Ordering::SeqCst));

        // Close the silent connection so the orphaned blocking connect finishes promptly.
        let (stream, _) = listener.accept().unwrap();
        drop(stream);
        drop(listener);
    }

    #[test]
    fn operation_gate_is_scoped_to_subscription_enabled_self_hosted_clients() {
        assert!(self_hosted_operation_gate(true).is_some());
        assert!(self_hosted_operation_gate(false).is_none());
    }
}
