//! History cache shared by every connection the Electrum manager creates.

use bdk_electrum::electrum_client::{GetHistoryRes, ScriptStatus};
use bdk_wallet::bitcoin::ScriptBuf;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Clone, Debug)]
pub(super) struct HistoryCacheEntry {
    pub(super) status: Option<ScriptStatus>,
    pub(super) history: Vec<GetHistoryRes>,
    pub(super) last_revalidated: Instant,
    pub(super) dirty: bool,
}

#[derive(Debug, Default)]
pub(super) struct HistoryCacheState {
    pub(super) entries: HashMap<ScriptBuf, HistoryCacheEntry>,
    pub(super) hits: u64,
    pub(super) misses: u64,
    pub(super) reconnect_resubscriptions: u64,
}

/// History cache shared by replacement Electrum connections.
///
/// Keeping this outside the socket-backed client lets a reconnect resubscribe and compare the
/// server's current status with the history cached before the disconnect.
#[derive(Clone, Debug, Default)]
pub(crate) struct SharedHistoryCache(pub(super) Arc<Mutex<HistoryCacheState>>);

impl SharedHistoryCache {
    pub(crate) fn forget_scripts(&self, scripts: &[ScriptBuf]) -> usize {
        let mut cache = self.0.lock().expect("history cache lock poisoned");
        scripts
            .iter()
            .filter(|script| cache.entries.remove(*script).is_some())
            .count()
    }
}
