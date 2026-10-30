//! The subscription history adapter: construction, subscription ownership, wallet cleanup and
//! the cancellation state shared with the timeout wrapper in `electrum`.

use super::cache::SharedHistoryCache;
use super::is_not_subscribed;
use bdk_electrum::electrum_client::{ElectrumApi, Error};
use bdk_wallet::bitcoin::ScriptBuf;
use std::collections::HashSet;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tracing::{debug, info, warn};

/// Returned by [`SubscriptionHistoryClient::start_work`] once a timed-out worker failed to stop:
/// the shared socket is in an unknown state until the connection is replaced.
#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("Electrum client requires connection replacement after an unresponsive worker")]
pub(crate) struct ConnectionPoisoned;

/// Electrum API adapter that turns script subscriptions into a cache invalidation signal.
///
/// BDK still asks for every revealed script on each recurring sync. This adapter preserves the
/// ordered batch contract while fetching history only for new, changed, reconnected, or safety-
/// revalidated scripts. Full scans intentionally use the underlying polling client directly.
#[derive(Debug)]
pub(crate) struct SubscriptionHistoryClient<E> {
    pub(super) inner: E,
    pub(super) cache: SharedHistoryCache,
    pub(super) subscribed: Mutex<HashSet<ScriptBuf>>,
    pub(super) enabled: bool,
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

    pub(super) fn cache_diagnostics(&self) -> (usize, u64, u64, u64) {
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
    pub(crate) fn start_work(&self) -> Result<(), ConnectionPoisoned> {
        if self.cancellation_enabled {
            if self.work_state.poisoned.load(Ordering::Acquire) {
                return Err(ConnectionPoisoned);
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

    pub(super) fn ensure_work_active(&self) -> Result<(), Error> {
        if self.cancellation_enabled && self.work_state.cancelled.load(Ordering::Acquire) {
            // A `TimedOut` I/O error keeps the cancellation classifiable as a timeout after
            // it passes through BDK, which only forwards `electrum_client::Error`.
            return Err(Error::IOError(io::Error::new(
                io::ErrorKind::TimedOut,
                "Electrum work cancelled after operation timeout",
            )));
        }
        Ok(())
    }

    pub(super) fn call<T>(
        &self,
        operation: impl FnOnce(&E) -> Result<T, Error>,
    ) -> Result<T, Error> {
        self.ensure_work_active()?;
        operation(&self.inner)
    }

    pub(crate) fn prepare_recurring_work(&self) {
        if self.enabled && self.ensure_work_active().is_ok() {
            // electrum-client only processes queued notifications while an RPC reads the socket.
            // The following sync call will surface any real transport failure.
            if let Err(error) = self.inner.ping() {
                debug!("Electrum notification ping failed before recurring work: {error}");
                // Client's retry wrapper may already have replaced the raw socket.
                // Drop local subscription ownership so the next work item
                // batch-resubscribes instead of popping stale entries.
                self.subscribed
                    .lock()
                    .expect("subscription set lock poisoned")
                    .clear();
            }
        }
    }

    pub(crate) fn forget_scripts(&self, scripts: &[ScriptBuf]) -> Result<(), Error> {
        // Drop all local ownership up front. If cancellation interrupts server cleanup, the
        // caller replaces this connection, which removes any remaining server-side subscriptions
        // with the old socket; retaining stale local/cache entries would be less safe.
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

        let mut server_unsubscribe_attempts = 0;
        let mut cancellation_error = None;
        let mut unsubscribe_error = None;
        for script in &subscribed_scripts {
            if let Err(error) = self.ensure_work_active() {
                cancellation_error = Some(error);
                break;
            }
            server_unsubscribe_attempts += 1;
            match self.inner.script_unsubscribe(script.as_script()) {
                Ok(_) => {}
                Err(error) if is_not_subscribed(&error) => {}
                Err(error) => {
                    warn!(
                        "Electrum script unsubscribe failed during wallet cleanup; the local subscription was still removed and the connection must be replaced: {error}"
                    );
                    unsubscribe_error.get_or_insert(error);
                }
            }
        }
        let evicted = self.cache.forget_scripts(scripts);
        info!(
            "Electrum subscription cleanup: requested_scripts={}, locally_removed_subscriptions={}, server_unsubscribe_attempts={server_unsubscribe_attempts}, evicted_history_entries={evicted}",
            scripts.len(),
            subscribed_scripts.len()
        );
        if let Some(error) = cancellation_error {
            return Err(error);
        }
        if let Some(error) = unsubscribe_error {
            return Err(error);
        }
        Ok(())
    }
}
