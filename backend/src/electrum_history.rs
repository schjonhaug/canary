use bdk_electrum::electrum_client::{
    Batch, BroadcastPackageRes, ElectrumApi, Error, EstimationMode, GetBalanceRes, GetHeadersRes,
    GetHistoryRes, GetMerkleRes, ListUnspentRes, MempoolInfoRes, Param, RawHeaderNotification,
    ScriptStatus, ServerFeaturesRes, ToElectrumScriptHash, TxidFromPosRes,
};
use bdk_wallet::bitcoin::{Script, ScriptBuf, Txid};
use std::borrow::Borrow;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

pub(crate) const HISTORY_REVALIDATION_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
const MAX_QUEUED_NOTIFICATIONS_PER_SCRIPT: usize = 1_024;

#[derive(Clone, Debug)]
struct HistoryCacheEntry {
    status: Option<ScriptStatus>,
    history: Vec<GetHistoryRes>,
    last_revalidated: Instant,
    dirty: bool,
}

#[derive(Debug, Default)]
struct HistoryCacheState {
    entries: HashMap<ScriptBuf, HistoryCacheEntry>,
    hits: u64,
    misses: u64,
    reconnect_resubscriptions: u64,
}

/// History cache shared by replacement Electrum connections.
///
/// Keeping this outside the socket-backed client lets a reconnect resubscribe and compare the
/// server's current status with the history cached before the disconnect.
#[derive(Clone, Debug, Default)]
pub(crate) struct SharedHistoryCache(Arc<Mutex<HistoryCacheState>>);

impl SharedHistoryCache {
    pub(crate) fn forget_scripts(&self, scripts: &[ScriptBuf]) -> usize {
        let mut cache = self.0.lock().expect("history cache lock poisoned");
        scripts
            .iter()
            .filter(|script| cache.entries.remove(*script).is_some())
            .count()
    }
}

/// Electrum API adapter that turns script subscriptions into a cache invalidation signal.
///
/// BDK still asks for every revealed script on each recurring sync. This adapter preserves the
/// ordered batch contract while fetching history only for new, changed, reconnected, or safety-
/// revalidated scripts. Full scans intentionally use the underlying polling client directly.
#[derive(Debug)]
pub(crate) struct SubscriptionHistoryClient<E> {
    inner: E,
    cache: SharedHistoryCache,
    subscribed: Mutex<HashSet<ScriptBuf>>,
    enabled: bool,
    cancellation_enabled: bool,
    work_state: Arc<WorkState>,
}

#[derive(Debug, Default)]
struct WorkState {
    cancelled: AtomicBool,
    poisoned: AtomicBool,
}

impl<E> SubscriptionHistoryClient<E> {
    pub(crate) fn new(inner: E, cache: SharedHistoryCache, enabled: bool) -> Self {
        Self {
            inner,
            cache,
            subscribed: Mutex::new(HashSet::new()),
            enabled,
            cancellation_enabled: enabled,
            work_state: Arc::new(WorkState::default()),
        }
    }

    pub(crate) fn polling(inner: E, recurring: &Self) -> Self {
        Self {
            inner,
            cache: SharedHistoryCache::default(),
            subscribed: Mutex::new(HashSet::new()),
            enabled: false,
            cancellation_enabled: recurring.cancellation_enabled,
            work_state: Arc::clone(&recurring.work_state),
        }
    }

    fn cache_diagnostics(&self) -> (usize, u64, u64, u64) {
        let cache = self.cache.0.lock().expect("history cache lock poisoned");
        let subscribed = self
            .subscribed
            .lock()
            .expect("subscription set lock poisoned")
            .len();
        (
            subscribed,
            cache.hits,
            cache.misses,
            cache.reconnect_resubscriptions,
        )
    }
}

impl<E: ElectrumApi> SubscriptionHistoryClient<E> {
    fn probe_script_status(&self, script: &Script) -> Result<Option<ScriptStatus>, Error> {
        let serialized_hash = serde_json::to_value(script.to_electrum_scripthash())?;
        let script_hash = serialized_hash
            .as_str()
            .ok_or_else(|| Error::InvalidResponse(serialized_hash.clone()))?
            .to_string();
        let status = self.call(|inner| {
            inner.raw_call(
                "blockchain.scripthash.subscribe",
                [Param::String(script_hash)],
            )
        })?;
        serde_json::from_value(status).map_err(Error::JSON)
    }

    pub(crate) fn start_work(&self) -> Result<(), Error> {
        if self.cancellation_enabled {
            if self.work_state.poisoned.load(Ordering::Acquire) {
                return Err(Error::Message(
                    "Electrum client requires connection replacement after an unresponsive worker"
                        .to_string(),
                ));
            }
            self.work_state.cancelled.store(false, Ordering::Release);
        }
        Ok(())
    }

    pub(crate) fn cancel_work(&self) {
        if self.cancellation_enabled {
            self.work_state.cancelled.store(true, Ordering::Release);
        }
    }

    pub(crate) fn poison_work(&self) {
        if self.cancellation_enabled {
            self.work_state.cancelled.store(true, Ordering::Release);
            self.work_state.poisoned.store(true, Ordering::Release);
        }
    }

    pub(crate) fn update_timeout_state(&self, poison: bool) {
        if poison {
            self.poison_work();
        } else {
            self.cancel_work();
        }
    }

    fn ensure_work_active(&self) -> Result<(), Error> {
        if self.cancellation_enabled && self.work_state.cancelled.load(Ordering::Acquire) {
            return Err(Error::Message(
                "Electrum work cancelled after operation timeout".to_string(),
            ));
        }
        Ok(())
    }

    fn call<T>(&self, operation: impl FnOnce(&E) -> Result<T, Error>) -> Result<T, Error> {
        self.ensure_work_active()?;
        operation(&self.inner)
    }

    pub(crate) fn prepare_recurring_work(&self) {
        if self.enabled && self.ensure_work_active().is_ok() {
            // electrum-client only processes queued notifications while an RPC reads the socket.
            // The following sync call will surface any real transport failure.
            if let Err(error) = self.inner.ping() {
                debug!("Electrum notification ping failed before recurring work: {error}");
            }
        }
    }

    pub(crate) fn forget_scripts(&self, scripts: &[ScriptBuf]) {
        let subscribed_scripts: Vec<_> = {
            let mut subscribed = self
                .subscribed
                .lock()
                .expect("subscription set lock poisoned");
            scripts
                .iter()
                .filter(|script| subscribed.remove(*script))
                .cloned()
                .collect()
        };

        for script in &subscribed_scripts {
            match self.inner.script_unsubscribe(script.as_script()) {
                Ok(_) | Err(Error::NotSubscribed(_)) => {}
                Err(error) => warn!(
                    "Electrum script unsubscribe failed during wallet cleanup; the local subscription was still removed: {error}"
                ),
            }
        }
        let evicted = self.cache.forget_scripts(scripts);
        info!(
            "Electrum subscription cleanup: requested_scripts={}, unsubscribed_scripts={}, evicted_history_entries={evicted}",
            scripts.len(),
            subscribed_scripts.len()
        );
    }

    fn histories_at(
        &self,
        scripts: &[ScriptBuf],
        now: Instant,
    ) -> Result<Vec<Vec<GetHistoryRes>>, Error> {
        self.ensure_work_active()?;
        if !self.enabled {
            return self
                .inner
                .batch_script_get_history(scripts.iter().map(|script| script.as_script()));
        }

        let mut results = vec![None; scripts.len()];
        let mut fetch_indices = Vec::new();
        let mut statuses = vec![None; scripts.len()];
        let mut status_observed = vec![false; scripts.len()];
        let mut safety_revalidation = false;

        for (index, script) in scripts.iter().enumerate() {
            self.ensure_work_active()?;
            let cached = self
                .cache
                .0
                .lock()
                .expect("history cache lock poisoned")
                .entries
                .get(script)
                .cloned();
            let mut should_fetch = cached.as_ref().is_none_or(|entry| entry.dirty);
            let mut subscription_failed = false;
            let known_subscribed = self
                .subscribed
                .lock()
                .expect("subscription set lock poisoned")
                .contains(script);

            if !known_subscribed {
                match self.inner.script_subscribe(script.as_script()) {
                    Ok(status) => {
                        self.subscribed
                            .lock()
                            .expect("subscription set lock poisoned")
                            .insert(script.clone());
                        statuses[index] = status;
                        status_observed[index] = true;
                        if let Some(entry) = &cached {
                            self.cache
                                .0
                                .lock()
                                .expect("history cache lock poisoned")
                                .reconnect_resubscriptions += 1;
                            should_fetch |= entry.status != status;
                        }
                    }
                    Err(Error::AlreadySubscribed(_)) => {
                        self.subscribed
                            .lock()
                            .expect("subscription set lock poisoned")
                            .insert(script.clone());
                    }
                    Err(error) => {
                        warn!(
                            "Electrum script subscription failed; polling history for this work item: {error}"
                        );
                        subscription_failed = true;
                        should_fetch = true;
                    }
                }
            }

            if !subscription_failed
                && self
                    .subscribed
                    .lock()
                    .expect("subscription set lock poisoned")
                    .contains(script)
            {
                let mut notifications_seen = 0;
                loop {
                    match self.inner.script_pop(script.as_script()) {
                        Ok(Some(status)) => {
                            notifications_seen += 1;
                            statuses[index] = Some(status);
                            status_observed[index] = true;
                            if cached
                                .as_ref()
                                .is_none_or(|entry| entry.status != Some(status))
                            {
                                should_fetch = true;
                            }
                            if notifications_seen >= MAX_QUEUED_NOTIFICATIONS_PER_SCRIPT {
                                warn!(
                                    "Electrum notification drain reached its per-script safety limit; refreshing history and continuing on the next work item"
                                );
                                should_fetch = true;
                                break;
                            }
                        }
                        Ok(None) => {
                            // The trait cannot distinguish an empty local queue from a backend
                            // representing Electrum's null status as None. Probe only scripts with
                            // actual history: an unchanged status remains a history-cache hit,
                            // while a null/changed status triggers the authoritative refresh.
                            if !status_observed[index]
                                && cached
                                    .as_ref()
                                    .is_some_and(|entry| !entry.dirty && !entry.history.is_empty())
                            {
                                match self.probe_script_status(script.as_script()) {
                                    Ok(status) => {
                                        statuses[index] = status;
                                        status_observed[index] = true;
                                        should_fetch |= cached
                                            .as_ref()
                                            .is_none_or(|entry| entry.status != status);
                                    }
                                    Err(error) => {
                                        warn!(
                                            "Electrum status probe failed; polling history for this work item: {error}"
                                        );
                                        should_fetch = true;
                                    }
                                }
                            }
                            break;
                        }
                        Err(Error::NotSubscribed(_)) => {
                            self.subscribed
                                .lock()
                                .expect("subscription set lock poisoned")
                                .remove(script);
                            match self.inner.script_subscribe(script.as_script()) {
                                Ok(status) => {
                                    self.subscribed
                                        .lock()
                                        .expect("subscription set lock poisoned")
                                        .insert(script.clone());
                                    statuses[index] = status;
                                    status_observed[index] = true;
                                    self.cache
                                        .0
                                        .lock()
                                        .expect("history cache lock poisoned")
                                        .reconnect_resubscriptions += 1;
                                    should_fetch |=
                                        cached.as_ref().is_none_or(|entry| entry.status != status);
                                }
                                Err(error) => {
                                    warn!(
                                        "Electrum resubscription failed; polling history for this work item: {error}"
                                    );
                                    should_fetch = true;
                                }
                            }
                            break;
                        }
                        Err(error) => {
                            warn!(
                                "Electrum notification read failed; polling history for this work item: {error}"
                            );
                            should_fetch = true;
                            break;
                        }
                    }
                }
            }

            if cached.as_ref().is_some_and(|entry| {
                now.saturating_duration_since(entry.last_revalidated)
                    >= HISTORY_REVALIDATION_INTERVAL
            }) {
                should_fetch = true;
                safety_revalidation = true;
            }

            if should_fetch {
                fetch_indices.push(index);
            } else if let Some(entry) = cached {
                results[index] = Some(entry.history);
                self.cache
                    .0
                    .lock()
                    .expect("history cache lock poisoned")
                    .hits += 1;
            }
        }

        let reconciliation_start = Instant::now();
        if !fetch_indices.is_empty() {
            // Notifications are consumed by script_pop before the history RPC. Persist a dirty
            // marker (and the newest status) first so a transient history failure is retried on
            // the next sync instead of serving the old cache until safety reconciliation.
            {
                let mut cache = self.cache.0.lock().expect("history cache lock poisoned");
                for &index in &fetch_indices {
                    if let Some(entry) = cache.entries.get_mut(&scripts[index]) {
                        entry.dirty = true;
                        if status_observed[index] {
                            entry.status = statuses[index];
                        }
                    }
                }
            }
            self.ensure_work_active()?;
            let fetched = self.inner.batch_script_get_history(
                fetch_indices
                    .iter()
                    .map(|&index| scripts[index].as_script()),
            )?;
            let mut cache = self.cache.0.lock().expect("history cache lock poisoned");
            for (index, history) in fetch_indices.into_iter().zip(fetched) {
                let effective_status = if status_observed[index] {
                    statuses[index]
                } else {
                    cache
                        .entries
                        .get(&scripts[index])
                        .and_then(|entry| entry.status)
                };
                cache.entries.insert(
                    scripts[index].clone(),
                    HistoryCacheEntry {
                        status: effective_status,
                        history: history.clone(),
                        last_revalidated: now,
                        dirty: false,
                    },
                );
                cache.misses += 1;
                results[index] = Some(history);
            }
        }

        if safety_revalidation {
            info!(
                "Electrum history safety reconciliation completed in {:.2?}",
                reconciliation_start.elapsed()
            );
        }
        let (subscribed, hits, misses, reconnects) = self.cache_diagnostics();
        debug!(
            "Electrum history cache: subscribed_scripts={subscribed}, hits={hits}, misses={misses}, reconnect_resubscriptions={reconnects}"
        );

        Ok(results
            .into_iter()
            .map(|history| history.expect("every history result is populated"))
            .collect())
    }
}

impl<E: ElectrumApi> ElectrumApi for SubscriptionHistoryClient<E> {
    fn raw_call(
        &self,
        method_name: &str,
        params: impl IntoIterator<Item = Param>,
    ) -> Result<serde_json::Value, Error> {
        self.call(|inner| inner.raw_call(method_name, params))
    }

    fn batch_call(&self, batch: &Batch) -> Result<Vec<serde_json::Value>, Error> {
        self.call(|inner| inner.batch_call(batch))
    }

    fn block_headers_subscribe_raw(&self) -> Result<RawHeaderNotification, Error> {
        self.call(ElectrumApi::block_headers_subscribe_raw)
    }

    fn block_headers_pop_raw(&self) -> Result<Option<RawHeaderNotification>, Error> {
        self.call(ElectrumApi::block_headers_pop_raw)
    }

    fn block_header_raw(&self, height: usize) -> Result<Vec<u8>, Error> {
        self.call(|inner| inner.block_header_raw(height))
    }

    fn block_headers(&self, start_height: usize, count: usize) -> Result<GetHeadersRes, Error> {
        self.call(|inner| inner.block_headers(start_height, count))
    }

    fn estimate_fee(&self, number: usize, mode: Option<EstimationMode>) -> Result<f64, Error> {
        self.call(|inner| inner.estimate_fee(number, mode))
    }

    fn relay_fee(&self) -> Result<f64, Error> {
        self.call(ElectrumApi::relay_fee)
    }

    fn script_subscribe(&self, script: &Script) -> Result<Option<ScriptStatus>, Error> {
        self.call(|inner| inner.script_subscribe(script))
    }

    fn batch_script_subscribe<'s, I>(&self, scripts: I) -> Result<Vec<Option<ScriptStatus>>, Error>
    where
        I: IntoIterator + Clone,
        I::Item: Borrow<&'s Script>,
    {
        self.call(|inner| inner.batch_script_subscribe(scripts))
    }

    fn script_unsubscribe(&self, script: &Script) -> Result<bool, Error> {
        self.call(|inner| inner.script_unsubscribe(script))
    }

    fn script_pop(&self, script: &Script) -> Result<Option<ScriptStatus>, Error> {
        self.call(|inner| inner.script_pop(script))
    }

    fn script_get_balance(&self, script: &Script) -> Result<GetBalanceRes, Error> {
        self.call(|inner| inner.script_get_balance(script))
    }

    fn batch_script_get_balance<'s, I>(&self, scripts: I) -> Result<Vec<GetBalanceRes>, Error>
    where
        I: IntoIterator + Clone,
        I::Item: Borrow<&'s Script>,
    {
        self.call(|inner| inner.batch_script_get_balance(scripts))
    }

    fn script_get_history(&self, script: &Script) -> Result<Vec<GetHistoryRes>, Error> {
        self.histories_at(&[script.to_owned()], Instant::now())
            .map(|mut histories| histories.remove(0))
    }

    fn batch_script_get_history<'s, I>(&self, scripts: I) -> Result<Vec<Vec<GetHistoryRes>>, Error>
    where
        I: IntoIterator + Clone,
        I::Item: Borrow<&'s Script>,
    {
        let scripts: Vec<ScriptBuf> = scripts
            .into_iter()
            .map(|script| ScriptBuf::from_bytes(script.borrow().as_bytes().to_vec()))
            .collect();
        self.histories_at(&scripts, Instant::now())
    }

    fn script_list_unspent(&self, script: &Script) -> Result<Vec<ListUnspentRes>, Error> {
        self.call(|inner| inner.script_list_unspent(script))
    }

    fn batch_script_list_unspent<'s, I>(
        &self,
        scripts: I,
    ) -> Result<Vec<Vec<ListUnspentRes>>, Error>
    where
        I: IntoIterator + Clone,
        I::Item: Borrow<&'s Script>,
    {
        self.call(|inner| inner.batch_script_list_unspent(scripts))
    }

    fn transaction_get_raw(&self, txid: &Txid) -> Result<Vec<u8>, Error> {
        self.call(|inner| inner.transaction_get_raw(txid))
    }

    fn batch_transaction_get_raw<'t, I>(&self, txids: I) -> Result<Vec<Vec<u8>>, Error>
    where
        I: IntoIterator + Clone,
        I::Item: Borrow<&'t Txid>,
    {
        self.call(|inner| inner.batch_transaction_get_raw(txids))
    }

    fn batch_block_header_raw<I>(&self, heights: I) -> Result<Vec<Vec<u8>>, Error>
    where
        I: IntoIterator + Clone,
        I::Item: Borrow<u32>,
    {
        self.call(|inner| inner.batch_block_header_raw(heights))
    }

    fn batch_estimate_fee<I>(&self, numbers: I) -> Result<Vec<f64>, Error>
    where
        I: IntoIterator + Clone,
        I::Item: Borrow<usize>,
    {
        self.call(|inner| inner.batch_estimate_fee(numbers))
    }

    fn transaction_broadcast_raw(&self, raw_tx: &[u8]) -> Result<Txid, Error> {
        self.call(|inner| inner.transaction_broadcast_raw(raw_tx))
    }

    fn transaction_broadcast_package_raw<T: AsRef<[u8]>>(
        &self,
        raw_txs: &[T],
    ) -> Result<BroadcastPackageRes, Error> {
        self.call(|inner| inner.transaction_broadcast_package_raw(raw_txs))
    }

    fn transaction_get_merkle(&self, txid: &Txid, height: usize) -> Result<GetMerkleRes, Error> {
        self.call(|inner| inner.transaction_get_merkle(txid, height))
    }

    fn batch_transaction_get_merkle<I>(
        &self,
        txids_and_heights: I,
    ) -> Result<Vec<GetMerkleRes>, Error>
    where
        I: IntoIterator + Clone,
        I::Item: Borrow<(Txid, usize)>,
    {
        self.call(|inner| inner.batch_transaction_get_merkle(txids_and_heights))
    }

    fn txid_from_pos(&self, height: usize, tx_pos: usize) -> Result<Txid, Error> {
        self.call(|inner| inner.txid_from_pos(height, tx_pos))
    }

    fn txid_from_pos_with_merkle(
        &self,
        height: usize,
        tx_pos: usize,
    ) -> Result<TxidFromPosRes, Error> {
        self.call(|inner| inner.txid_from_pos_with_merkle(height, tx_pos))
    }

    fn server_features(&self) -> Result<ServerFeaturesRes, Error> {
        self.call(ElectrumApi::server_features)
    }

    fn mempool_get_info(&self) -> Result<MempoolInfoRes, Error> {
        self.call(ElectrumApi::mempool_get_info)
    }

    fn ping(&self) -> Result<(), Error> {
        self.call(ElectrumApi::ping)
    }

    fn calls_made(&self) -> Result<usize, Error> {
        self.call(ElectrumApi::calls_made)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bdk_electrum::electrum_client::ToElectrumScriptHash;
    use std::collections::VecDeque;

    #[derive(Default)]
    struct FakeState {
        histories: HashMap<ScriptBuf, Vec<GetHistoryRes>>,
        statuses: HashMap<ScriptBuf, Option<ScriptStatus>>,
        notifications: HashMap<ScriptBuf, VecDeque<ScriptStatus>>,
        subscribed: HashSet<ScriptBuf>,
        subscription_calls: usize,
        status_probe_calls: usize,
        history_batches: Vec<Vec<ScriptBuf>>,
        unsubscribed_history_calls: usize,
        unsubscribe_calls: usize,
        ping_calls: usize,
        fail_subscriptions: bool,
        fail_history_batches: usize,
    }

    #[derive(Clone, Default)]
    struct FakeApi(Arc<Mutex<FakeState>>);

    impl FakeApi {
        fn state(&self) -> std::sync::MutexGuard<'_, FakeState> {
            self.0.lock().unwrap()
        }
    }

    fn script(seed: u8) -> ScriptBuf {
        ScriptBuf::from_bytes(vec![0x51, seed])
    }

    fn status(seed: u8) -> ScriptStatus {
        serde_json::from_value(serde_json::Value::String(format!("{seed:02x}").repeat(32))).unwrap()
    }

    fn history(seed: u8) -> Vec<GetHistoryRes> {
        vec![GetHistoryRes {
            height: seed as i32,
            tx_hash: format!("{seed:02x}").repeat(32).parse().unwrap(),
            fee: None,
        }]
    }

    impl ElectrumApi for FakeApi {
        fn raw_call(
            &self,
            method: &str,
            params: impl IntoIterator<Item = Param>,
        ) -> Result<serde_json::Value, Error> {
            assert_eq!(method, "blockchain.scripthash.subscribe");
            let script_hash = match params.into_iter().next() {
                Some(Param::String(script_hash)) => script_hash,
                _ => panic!("status probe requires a script hash"),
            };
            let mut state = self.state();
            state.status_probe_calls += 1;
            let status = state
                .statuses
                .iter()
                .find(|(script, _)| {
                    serde_json::to_value(script.as_script().to_electrum_scripthash())
                        .unwrap()
                        .as_str()
                        == Some(script_hash.as_str())
                })
                .and_then(|(_, status)| *status);
            serde_json::to_value(status).map_err(Error::JSON)
        }

        fn batch_call(&self, _: &Batch) -> Result<Vec<serde_json::Value>, Error> {
            unimplemented!()
        }

        fn block_headers_subscribe_raw(&self) -> Result<RawHeaderNotification, Error> {
            unimplemented!()
        }

        fn block_headers_pop_raw(&self) -> Result<Option<RawHeaderNotification>, Error> {
            unimplemented!()
        }

        fn block_header_raw(&self, _: usize) -> Result<Vec<u8>, Error> {
            unimplemented!()
        }

        fn block_headers(&self, _: usize, _: usize) -> Result<GetHeadersRes, Error> {
            unimplemented!()
        }

        fn estimate_fee(&self, _: usize, _: Option<EstimationMode>) -> Result<f64, Error> {
            unimplemented!()
        }

        fn relay_fee(&self) -> Result<f64, Error> {
            unimplemented!()
        }

        fn script_subscribe(&self, script: &Script) -> Result<Option<ScriptStatus>, Error> {
            let script = ScriptBuf::from_bytes(script.as_bytes().to_vec());
            let mut state = self.state();
            state.subscription_calls += 1;
            if state.fail_subscriptions {
                return Err(Error::Message("subscriptions unavailable".to_string()));
            }
            if !state.subscribed.insert(script.clone()) {
                return Err(Error::AlreadySubscribed(
                    script.as_script().to_electrum_scripthash(),
                ));
            }
            Ok(state.statuses.get(&script).copied().flatten())
        }

        fn batch_script_subscribe<'s, I>(
            &self,
            scripts: I,
        ) -> Result<Vec<Option<ScriptStatus>>, Error>
        where
            I: IntoIterator + Clone,
            I::Item: Borrow<&'s Script>,
        {
            scripts
                .into_iter()
                .map(|script| self.script_subscribe(script.borrow()))
                .collect()
        }

        fn script_unsubscribe(&self, script: &Script) -> Result<bool, Error> {
            let script = ScriptBuf::from_bytes(script.as_bytes().to_vec());
            let mut state = self.state();
            state.unsubscribe_calls += 1;
            Ok(state.subscribed.remove(&script))
        }

        fn script_pop(&self, script: &Script) -> Result<Option<ScriptStatus>, Error> {
            let script = ScriptBuf::from_bytes(script.as_bytes().to_vec());
            let mut state = self.state();
            if !state.subscribed.contains(&script) {
                return Err(Error::NotSubscribed(
                    script.as_script().to_electrum_scripthash(),
                ));
            }
            Ok(state.notifications.entry(script).or_default().pop_front())
        }

        fn script_get_balance(&self, _: &Script) -> Result<GetBalanceRes, Error> {
            unimplemented!()
        }

        fn batch_script_get_balance<'s, I>(&self, _: I) -> Result<Vec<GetBalanceRes>, Error>
        where
            I: IntoIterator + Clone,
            I::Item: Borrow<&'s Script>,
        {
            unimplemented!()
        }

        fn script_get_history(&self, script: &Script) -> Result<Vec<GetHistoryRes>, Error> {
            self.batch_script_get_history([script])
                .map(|mut result| result.remove(0))
        }

        fn batch_script_get_history<'s, I>(
            &self,
            scripts: I,
        ) -> Result<Vec<Vec<GetHistoryRes>>, Error>
        where
            I: IntoIterator + Clone,
            I::Item: Borrow<&'s Script>,
        {
            let scripts: Vec<_> = scripts
                .into_iter()
                .map(|script| ScriptBuf::from_bytes(script.borrow().as_bytes().to_vec()))
                .collect();
            let mut state = self.state();
            if state.fail_history_batches > 0 {
                state.fail_history_batches -= 1;
                return Err(Error::Message("transient history failure".to_string()));
            }
            state.unsubscribed_history_calls += scripts
                .iter()
                .filter(|script| !state.subscribed.contains(*script))
                .count();
            state.history_batches.push(scripts.clone());
            Ok(scripts
                .iter()
                .map(|script| state.histories.get(script).cloned().unwrap_or_default())
                .collect())
        }

        fn script_list_unspent(&self, _: &Script) -> Result<Vec<ListUnspentRes>, Error> {
            unimplemented!()
        }

        fn batch_script_list_unspent<'s, I>(&self, _: I) -> Result<Vec<Vec<ListUnspentRes>>, Error>
        where
            I: IntoIterator + Clone,
            I::Item: Borrow<&'s Script>,
        {
            unimplemented!()
        }

        fn transaction_get_raw(&self, _: &Txid) -> Result<Vec<u8>, Error> {
            unimplemented!()
        }

        fn batch_transaction_get_raw<'t, I>(&self, _: I) -> Result<Vec<Vec<u8>>, Error>
        where
            I: IntoIterator + Clone,
            I::Item: Borrow<&'t Txid>,
        {
            unimplemented!()
        }

        fn batch_block_header_raw<I>(&self, _: I) -> Result<Vec<Vec<u8>>, Error>
        where
            I: IntoIterator + Clone,
            I::Item: Borrow<u32>,
        {
            unimplemented!()
        }

        fn batch_estimate_fee<I>(&self, _: I) -> Result<Vec<f64>, Error>
        where
            I: IntoIterator + Clone,
            I::Item: Borrow<usize>,
        {
            unimplemented!()
        }

        fn transaction_broadcast_raw(&self, _: &[u8]) -> Result<Txid, Error> {
            unimplemented!()
        }

        fn transaction_broadcast_package_raw<T: AsRef<[u8]>>(
            &self,
            _: &[T],
        ) -> Result<BroadcastPackageRes, Error> {
            unimplemented!()
        }

        fn transaction_get_merkle(&self, _: &Txid, _: usize) -> Result<GetMerkleRes, Error> {
            unimplemented!()
        }

        fn batch_transaction_get_merkle<I>(&self, _: I) -> Result<Vec<GetMerkleRes>, Error>
        where
            I: IntoIterator + Clone,
            I::Item: Borrow<(Txid, usize)>,
        {
            unimplemented!()
        }

        fn txid_from_pos(&self, _: usize, _: usize) -> Result<Txid, Error> {
            unimplemented!()
        }

        fn txid_from_pos_with_merkle(&self, _: usize, _: usize) -> Result<TxidFromPosRes, Error> {
            unimplemented!()
        }

        fn server_features(&self) -> Result<ServerFeaturesRes, Error> {
            unimplemented!()
        }

        fn mempool_get_info(&self) -> Result<MempoolInfoRes, Error> {
            unimplemented!()
        }

        fn ping(&self) -> Result<(), Error> {
            self.state().ping_calls += 1;
            Ok(())
        }

        fn calls_made(&self) -> Result<usize, Error> {
            unimplemented!()
        }
    }

    #[test]
    fn electrs_warning_condition_is_avoided_and_warm_history_is_cached() {
        let api = FakeApi::default();
        let script = script(1);
        {
            let mut state = api.state();
            state.statuses.insert(script.clone(), Some(status(1)));
            state.histories.insert(script.clone(), history(1));
        }
        let adapter =
            SubscriptionHistoryClient::new(api.clone(), SharedHistoryCache::default(), true);

        adapter.prepare_recurring_work();
        let first = adapter
            .batch_script_get_history([script.as_script()])
            .unwrap();
        adapter.prepare_recurring_work();
        let second = adapter
            .batch_script_get_history([script.as_script()])
            .unwrap();

        assert_eq!(first[0][0].height, 1);
        assert_eq!(second[0][0].height, 1);
        let state = api.state();
        assert_eq!(state.subscription_calls, 1);
        assert_eq!(state.history_batches.len(), 1);
        assert_eq!(state.status_probe_calls, 1);
        assert_eq!(state.unsubscribed_history_calls, 0);
        assert_eq!(state.ping_calls, 2);
    }

    #[test]
    fn cancelled_work_stops_before_the_next_electrum_rpc() {
        let api = FakeApi::default();
        let adapter =
            SubscriptionHistoryClient::new(api.clone(), SharedHistoryCache::default(), true);

        adapter.cancel_work();
        assert!(adapter.ping().is_err());
        assert_eq!(api.state().ping_calls, 0);

        adapter.start_work().unwrap();
        adapter.ping().unwrap();
        assert_eq!(api.state().ping_calls, 1);

        adapter.poison_work();
        assert!(adapter.start_work().is_err());
    }

    #[test]
    fn polling_and_recurring_clients_share_timeout_state() {
        let api = FakeApi::default();
        let recurring =
            SubscriptionHistoryClient::new(api.clone(), SharedHistoryCache::default(), true);
        let polling = SubscriptionHistoryClient::polling(api, &recurring);

        polling.poison_work();
        assert!(recurring.start_work().is_err());
    }

    #[test]
    fn changed_and_multiple_queued_notifications_refresh_once() {
        let api = FakeApi::default();
        let script = script(2);
        {
            let mut state = api.state();
            state.statuses.insert(script.clone(), Some(status(1)));
            state.histories.insert(script.clone(), history(1));
        }
        let adapter =
            SubscriptionHistoryClient::new(api.clone(), SharedHistoryCache::default(), true);
        adapter
            .batch_script_get_history([script.as_script()])
            .unwrap();
        {
            let mut state = api.state();
            state.histories.insert(script.clone(), history(3));
            state.statuses.insert(script.clone(), Some(status(3)));
            state
                .notifications
                .entry(script.clone())
                .or_default()
                .extend([status(2), status(3)]);
        }

        let refreshed = adapter
            .batch_script_get_history([script.as_script()])
            .unwrap();
        assert_eq!(refreshed[0][0].height, 3);
        assert_eq!(api.state().history_batches.len(), 2);
    }

    #[test]
    fn consumed_notification_remains_dirty_after_failed_history_refresh() {
        let api = FakeApi::default();
        let script = script(23);
        {
            let mut state = api.state();
            state.statuses.insert(script.clone(), Some(status(1)));
            state.histories.insert(script.clone(), history(1));
        }
        let cache = SharedHistoryCache::default();
        let adapter = SubscriptionHistoryClient::new(api.clone(), cache.clone(), true);
        adapter
            .batch_script_get_history([script.as_script()])
            .unwrap();

        {
            let mut state = api.state();
            state.histories.insert(script.clone(), history(2));
            state
                .notifications
                .entry(script.clone())
                .or_default()
                .push_back(status(2));
            state.fail_history_batches = 1;
        }
        assert!(adapter
            .batch_script_get_history([script.as_script()])
            .is_err());
        assert!(cache.0.lock().unwrap().entries[&script].dirty);

        let retried = adapter
            .batch_script_get_history([script.as_script()])
            .unwrap();
        assert_eq!(retried[0][0].height, 2);
        let cache = cache.0.lock().unwrap();
        assert!(!cache.entries[&script].dirty);
        assert_eq!(cache.entries[&script].status, Some(status(2)));
    }

    #[test]
    fn null_status_probe_revalidates_non_empty_history_to_empty() {
        let api = FakeApi::default();
        let script = script(25);
        {
            let mut state = api.state();
            state.statuses.insert(script.clone(), Some(status(25)));
            state.histories.insert(script.clone(), history(25));
        }
        let cache = SharedHistoryCache::default();
        let adapter = SubscriptionHistoryClient::new(api.clone(), cache.clone(), true);
        adapter
            .batch_script_get_history([script.as_script()])
            .unwrap();

        {
            let mut state = api.state();
            state.statuses.insert(script.clone(), None);
            state.histories.insert(script.clone(), Vec::new());
        }
        let refreshed = adapter
            .batch_script_get_history([script.as_script()])
            .unwrap();

        assert!(refreshed[0].is_empty());
        assert_eq!(api.state().history_batches.len(), 2);
        assert_eq!(api.state().status_probe_calls, 1);
        assert_eq!(cache.0.lock().unwrap().entries[&script].status, None);
    }

    #[test]
    fn sparse_batch_refresh_keeps_status_with_its_original_script() {
        let api = FakeApi::default();
        let scripts = [script(10), script(11), script(12)];
        {
            let mut state = api.state();
            for (offset, script) in scripts.iter().enumerate() {
                let seed = 10 + offset as u8;
                state.statuses.insert(script.clone(), Some(status(seed)));
                state.histories.insert(script.clone(), history(seed));
            }
        }
        let cache = SharedHistoryCache::default();
        let adapter = SubscriptionHistoryClient::new(api.clone(), cache.clone(), true);
        adapter
            .batch_script_get_history(scripts.iter().map(ScriptBuf::as_script))
            .unwrap();

        {
            let mut state = api.state();
            state.histories.insert(scripts[2].clone(), history(13));
            state
                .notifications
                .entry(scripts[2].clone())
                .or_default()
                .push_back(status(13));
        }
        let refreshed = adapter
            .batch_script_get_history(scripts.iter().map(ScriptBuf::as_script))
            .unwrap();

        assert_eq!(refreshed[2][0].height, 13);
        assert_eq!(
            cache.0.lock().unwrap().entries[&scripts[2]].status,
            Some(status(13))
        );
        assert_eq!(api.state().history_batches[1], vec![scripts[2].clone()]);
    }

    #[test]
    fn wallet_cleanup_unsubscribes_and_evicts_only_removed_scripts() {
        let api = FakeApi::default();
        let removed = script(20);
        let retained = script(21);
        {
            let mut state = api.state();
            for (seed, script) in [(20, &removed), (21, &retained)] {
                state.statuses.insert(script.clone(), Some(status(seed)));
                state.histories.insert(script.clone(), history(seed));
            }
        }
        let cache = SharedHistoryCache::default();
        let adapter = SubscriptionHistoryClient::new(api.clone(), cache.clone(), true);
        adapter
            .batch_script_get_history([removed.as_script(), retained.as_script()])
            .unwrap();

        adapter.forget_scripts(std::slice::from_ref(&removed));

        let state = api.state();
        assert_eq!(state.unsubscribe_calls, 1);
        assert!(!state.subscribed.contains(&removed));
        assert!(state.subscribed.contains(&retained));
        drop(state);
        let cache = cache.0.lock().unwrap();
        assert!(!cache.entries.contains_key(&removed));
        assert!(cache.entries.contains_key(&retained));
    }

    #[test]
    fn notification_drain_has_a_safety_bound() {
        let api = FakeApi::default();
        let script = script(22);
        {
            let mut state = api.state();
            state.statuses.insert(script.clone(), Some(status(22)));
            state.histories.insert(script.clone(), history(22));
        }
        let adapter =
            SubscriptionHistoryClient::new(api.clone(), SharedHistoryCache::default(), true);
        adapter
            .batch_script_get_history([script.as_script()])
            .unwrap();
        api.state()
            .notifications
            .entry(script.clone())
            .or_default()
            .extend(std::iter::repeat_n(
                status(22),
                MAX_QUEUED_NOTIFICATIONS_PER_SCRIPT + 1,
            ));

        adapter
            .batch_script_get_history([script.as_script()])
            .unwrap();

        let state = api.state();
        assert_eq!(state.history_batches.len(), 2);
        assert_eq!(state.notifications[&script].len(), 1);
    }

    #[test]
    fn reconnect_resubscribes_without_refetching_unchanged_history() {
        let cache = SharedHistoryCache::default();
        let script = script(3);
        let first_api = FakeApi::default();
        {
            let mut state = first_api.state();
            state.statuses.insert(script.clone(), Some(status(3)));
            state.histories.insert(script.clone(), history(3));
        }
        SubscriptionHistoryClient::new(first_api, cache.clone(), true)
            .batch_script_get_history([script.as_script()])
            .unwrap();

        let reconnected_api = FakeApi::default();
        reconnected_api
            .state()
            .statuses
            .insert(script.clone(), Some(status(3)));
        let history = SubscriptionHistoryClient::new(reconnected_api.clone(), cache.clone(), true)
            .batch_script_get_history([script.as_script()])
            .unwrap();

        assert_eq!(history[0][0].height, 3);
        assert_eq!(reconnected_api.state().subscription_calls, 1);
        assert!(reconnected_api.state().history_batches.is_empty());
        assert_eq!(cache.0.lock().unwrap().reconnect_resubscriptions, 1);
    }

    #[test]
    fn not_subscribed_after_hidden_reconnect_resubscribes_and_compares_status() {
        let cache = SharedHistoryCache::default();
        let api = FakeApi::default();
        let script = script(6);
        {
            let mut state = api.state();
            state.statuses.insert(script.clone(), Some(status(6)));
            state.histories.insert(script.clone(), history(6));
        }
        let adapter = SubscriptionHistoryClient::new(api.clone(), cache.clone(), true);
        adapter
            .batch_script_get_history([script.as_script()])
            .unwrap();

        // Simulate electrum-client replacing its RawClient internally while the adapter still
        // remembers the script as subscribed.
        api.state().subscribed.clear();
        let cached = adapter
            .batch_script_get_history([script.as_script()])
            .unwrap();

        assert_eq!(cached[0][0].height, 6);
        let state = api.state();
        assert_eq!(state.subscription_calls, 2);
        assert_eq!(state.history_batches.len(), 1);
        assert_eq!(cache.0.lock().unwrap().reconnect_resubscriptions, 1);
    }

    #[test]
    fn null_status_after_hidden_reconnect_clears_non_empty_history() {
        let cache = SharedHistoryCache::default();
        let api = FakeApi::default();
        let script = script(24);
        {
            let mut state = api.state();
            state.statuses.insert(script.clone(), Some(status(24)));
            state.histories.insert(script.clone(), history(24));
        }
        let adapter = SubscriptionHistoryClient::new(api.clone(), cache.clone(), true);
        adapter
            .batch_script_get_history([script.as_script()])
            .unwrap();

        // electrum-client cannot represent a null-status notification in its ScriptStatus queue.
        // It replaces the raw client after the decode failure, which surfaces here as
        // NotSubscribed; resubscribing then returns the current null status.
        {
            let mut state = api.state();
            state.subscribed.clear();
            state.statuses.insert(script.clone(), None);
            state.histories.insert(script.clone(), Vec::new());
        }
        let refreshed = adapter
            .batch_script_get_history([script.as_script()])
            .unwrap();

        assert!(refreshed[0].is_empty());
        assert_eq!(api.state().history_batches.len(), 2);
        assert_eq!(cache.0.lock().unwrap().entries[&script].status, None);
    }

    #[test]
    fn six_hour_revalidation_and_polling_fallback_preserve_order() {
        let api = FakeApi::default();
        let first_script = script(4);
        let second_script = script(5);
        {
            let mut state = api.state();
            state.statuses.insert(first_script.clone(), Some(status(4)));
            state
                .statuses
                .insert(second_script.clone(), Some(status(5)));
            state.histories.insert(first_script.clone(), history(4));
            state.histories.insert(second_script.clone(), history(5));
        }
        let adapter =
            SubscriptionHistoryClient::new(api.clone(), SharedHistoryCache::default(), true);
        let start = Instant::now();
        adapter
            .histories_at(&[first_script.clone(), second_script.clone()], start)
            .unwrap();
        let reconciled = adapter
            .histories_at(
                &[first_script.clone(), second_script.clone()],
                start + HISTORY_REVALIDATION_INTERVAL,
            )
            .unwrap();
        assert_eq!(reconciled[0][0].height, 4);
        assert_eq!(reconciled[1][0].height, 5);
        assert_eq!(api.state().history_batches.len(), 2);

        let fallback_api = FakeApi::default();
        {
            let mut state = fallback_api.state();
            state.fail_subscriptions = true;
            state.histories.insert(first_script.clone(), history(4));
        }
        let fallback = SubscriptionHistoryClient::new(
            fallback_api.clone(),
            SharedHistoryCache::default(),
            true,
        );
        fallback
            .batch_script_get_history([first_script.as_script()])
            .unwrap();
        fallback
            .batch_script_get_history([first_script.as_script()])
            .unwrap();
        assert_eq!(fallback_api.state().history_batches.len(), 2);
    }
}
