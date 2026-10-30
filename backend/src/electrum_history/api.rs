//! `ElectrumApi` for the adapter: history calls go through the cache, every other call is
//! forwarded to the wrapped client after the cancellation check.

use super::client::SubscriptionHistoryClient;
use bdk_electrum::electrum_client::{
    Batch, BroadcastPackageRes, ElectrumApi, Error, EstimationMode, GetBalanceRes, GetHeadersRes,
    GetHistoryRes, GetMerkleRes, ListUnspentRes, MempoolInfoRes, Param, RawHeaderNotification,
    ScriptStatus, ServerFeaturesRes, TxidFromPosRes,
};
use bdk_wallet::bitcoin::{Script, ScriptBuf, Txid};
use std::borrow::Borrow;
use std::time::Instant;

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
