//! In-memory Electrum server that records subscriptions, notifications and history batches
//! and can inject the failure modes the adapter must survive.

use bdk_electrum::electrum_client::{
    Batch, BroadcastPackageRes, ElectrumApi, Error, EstimationMode, GetBalanceRes, GetHeadersRes,
    GetHistoryRes, GetMerkleRes, ListUnspentRes, MempoolInfoRes, Param, RawHeaderNotification,
    ScriptStatus, ServerFeaturesRes, ToElectrumScriptHash, TxidFromPosRes,
};
use bdk_wallet::bitcoin::{Script, ScriptBuf, Txid};
use std::borrow::Borrow;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Barrier, Mutex};

#[derive(Default)]
pub(super) struct FakeState {
    pub(super) histories: HashMap<ScriptBuf, Vec<GetHistoryRes>>,
    pub(super) statuses: HashMap<ScriptBuf, Option<ScriptStatus>>,
    pub(super) notifications: HashMap<ScriptBuf, VecDeque<ScriptStatus>>,
    pub(super) subscribed: HashSet<ScriptBuf>,
    pub(super) subscription_calls: usize,
    pub(super) subscription_batch_calls: usize,
    pub(super) partial_batch_failure_after: Option<usize>,
    pub(super) history_batches: Vec<Vec<ScriptBuf>>,
    pub(super) unsubscribed_history_calls: usize,
    pub(super) unsubscribe_calls: usize,
    pub(super) unsubscribe_started: Option<Arc<Barrier>>,
    pub(super) unsubscribe_release: Option<Arc<Barrier>>,
    pub(super) fail_unsubscribes: bool,
    pub(super) ping_calls: usize,
    pub(super) fail_ping: bool,
    pub(super) fail_subscriptions: bool,
    /// Models `electrum-client::Client`: `NotSubscribed` replaces the raw client
    /// and surfaces as `AllAttemptsErrored`.
    pub(super) wrap_not_subscribed: bool,
    pub(super) raw_client_replacements: usize,
    pub(super) fail_pops: bool,
    pub(super) fail_history_batches: usize,
    pub(super) history_response_len: Option<usize>,
}

#[derive(Clone, Default)]
pub(super) struct FakeApi(Arc<Mutex<FakeState>>);

impl FakeApi {
    pub(super) fn state(&self) -> std::sync::MutexGuard<'_, FakeState> {
        self.0.lock().unwrap()
    }
}

pub(super) fn script(seed: u8) -> ScriptBuf {
    ScriptBuf::from_bytes(vec![0x51, seed])
}

pub(super) fn indexed_script(index: u32) -> ScriptBuf {
    let mut bytes = vec![0x51];
    bytes.extend_from_slice(&index.to_le_bytes());
    ScriptBuf::from_bytes(bytes)
}

pub(super) fn status(seed: u8) -> ScriptStatus {
    serde_json::from_value(serde_json::Value::String(format!("{seed:02x}").repeat(32))).unwrap()
}

pub(super) fn history(seed: u8) -> Vec<GetHistoryRes> {
    vec![GetHistoryRes {
        height: i32::from(seed),
        tx_hash: format!("{seed:02x}").repeat(32).parse().unwrap(),
        fee: None,
    }]
}

fn wrap_not_subscribed_after_replace(state: &mut FakeState, script: &ScriptBuf) -> Error {
    state.subscribed.clear();
    state.raw_client_replacements += 1;
    Error::AllAttemptsErrored(vec![Error::NotSubscribed(
        script.as_script().to_electrum_scripthash(),
    )])
}

impl ElectrumApi for FakeApi {
    fn raw_call(
        &self,
        _: &str,
        _: impl IntoIterator<Item = Param>,
    ) -> Result<serde_json::Value, Error> {
        unimplemented!()
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

    fn batch_script_subscribe<'s, I>(&self, scripts: I) -> Result<Vec<Option<ScriptStatus>>, Error>
    where
        I: IntoIterator + Clone,
        I::Item: Borrow<&'s Script>,
    {
        self.state().subscription_batch_calls += 1;
        let partial_batch_failure_after = { self.state().partial_batch_failure_after.take() };
        if let Some(successful) = partial_batch_failure_after {
            for script in scripts.into_iter().take(successful) {
                self.script_subscribe(script.borrow())?;
            }
            return Err(Error::Message("partial batch failure".to_string()));
        }
        scripts
            .into_iter()
            .map(|script| self.script_subscribe(script.borrow()))
            .collect()
    }

    fn script_unsubscribe(&self, script: &Script) -> Result<bool, Error> {
        let script = ScriptBuf::from_bytes(script.as_bytes().to_vec());
        let (started, release) = {
            let mut state = self.state();
            state.unsubscribe_calls += 1;
            (
                state.unsubscribe_started.take(),
                state.unsubscribe_release.take(),
            )
        };
        if let Some(started) = started {
            started.wait();
        }
        if let Some(release) = release {
            release.wait();
        }
        if self.state().fail_unsubscribes {
            return Err(Error::Message("unsubscribe failed".to_string()));
        }
        let mut state = self.state();
        let removed = state.subscribed.remove(&script);
        if !removed && state.wrap_not_subscribed {
            return Err(wrap_not_subscribed_after_replace(&mut state, &script));
        }
        Ok(removed)
    }

    fn script_pop(&self, script: &Script) -> Result<Option<ScriptStatus>, Error> {
        let script = ScriptBuf::from_bytes(script.as_bytes().to_vec());
        let mut state = self.state();
        if state.fail_pops || !state.subscribed.contains(&script) {
            if state.wrap_not_subscribed {
                return Err(wrap_not_subscribed_after_replace(&mut state, &script));
            }
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

    fn batch_script_get_history<'s, I>(&self, scripts: I) -> Result<Vec<Vec<GetHistoryRes>>, Error>
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
        let response_len = state.history_response_len.take();
        let mut histories: Vec<_> = scripts
            .iter()
            .map(|script| state.histories.get(script).cloned().unwrap_or_default())
            .collect();
        if let Some(response_len) = response_len {
            histories.resize_with(response_len, Vec::new);
        }
        Ok(histories)
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
        let mut state = self.state();
        state.ping_calls += 1;
        if state.fail_ping {
            return Err(Error::Message("ping failed".to_string()));
        }
        Ok(())
    }

    fn calls_made(&self) -> Result<usize, Error> {
        unimplemented!()
    }
}
