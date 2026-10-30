//! Cached batch history lookup driven by Electrum script subscriptions, notifications and a
//! periodic safety revalidation.

use super::cache::HistoryCacheEntry;
use super::client::SubscriptionHistoryClient;
use super::is_not_subscribed;
use bdk_electrum::electrum_client::{ElectrumApi, Error, GetHistoryRes, ScriptStatus};
use bdk_wallet::bitcoin::ScriptBuf;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

pub(super) const HISTORY_REVALIDATION_INTERVAL: Duration = Duration::from_hours(6);
pub(super) const MAX_QUEUED_NOTIFICATIONS_PER_SCRIPT: usize = 1_024;

impl<E: ElectrumApi> SubscriptionHistoryClient<E> {
    pub(super) fn histories_at(
        &self,
        scripts: &[ScriptBuf],
        now: Instant,
    ) -> Result<Vec<Vec<GetHistoryRes>>, Error> {
        self.histories_at_inner(scripts, now, false)
    }

    fn histories_at_inner(
        &self,
        scripts: &[ScriptBuf],
        now: Instant,
        after_hidden_reconnect: bool,
    ) -> Result<Vec<Vec<GetHistoryRes>>, Error> {
        self.ensure_work_active()?;
        if !self.enabled {
            return self.inner.batch_script_get_history(
                scripts
                    .iter()
                    .map(bdk_wallet::bitcoin::ScriptBuf::as_script),
            );
        }

        let mut results = vec![None; scripts.len()];
        let mut fetch_indices = Vec::new();
        let mut statuses = vec![None; scripts.len()];
        let mut status_observed = vec![false; scripts.len()];
        let mut subscription_failed = vec![false; scripts.len()];
        let mut safety_revalidation = false;

        let cold_indices: Vec<_> = {
            let subscribed = self
                .subscribed
                .lock()
                .expect("subscription set lock poisoned");
            scripts
                .iter()
                .enumerate()
                .filter_map(|(index, script)| (!subscribed.contains(script)).then_some(index))
                .collect()
        };
        if !cold_indices.is_empty() {
            self.subscribe_cold_scripts(
                scripts,
                &cold_indices,
                &mut statuses,
                &mut status_observed,
                &mut subscription_failed,
            )?;
        }

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
            if status_observed[index]
                && let Some(entry) = &cached
            {
                self.cache
                    .0
                    .lock()
                    .expect("history cache lock poisoned")
                    .reconnect_resubscriptions += 1;
                should_fetch |= entry.status != statuses[index];
            }
            should_fetch |= subscription_failed[index];

            if !subscription_failed[index]
                && self
                    .subscribed
                    .lock()
                    .expect("subscription set lock poisoned")
                    .contains(script)
            {
                let subscriptions_lost = self.drain_notifications(
                    script,
                    cached.as_ref(),
                    after_hidden_reconnect,
                    &mut statuses[index],
                    &mut status_observed[index],
                    &mut should_fetch,
                );
                if subscriptions_lost {
                    return self.histories_at_inner(scripts, now, true);
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
            self.refresh_histories(
                scripts,
                fetch_indices,
                &statuses,
                &status_observed,
                now,
                &mut results,
            )?;
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

    /// Pops every queued status notification for a subscribed script, recording the newest status
    /// and whether the history must be refetched. Returns `true` when the raw client lost all
    /// subscriptions and the caller must resubscribe the whole work item.
    fn drain_notifications(
        &self,
        script: &ScriptBuf,
        cached: Option<&HistoryCacheEntry>,
        after_hidden_reconnect: bool,
        status_slot: &mut Option<ScriptStatus>,
        status_observed: &mut bool,
        should_fetch: &mut bool,
    ) -> bool {
        let mut notifications_seen = 0;
        loop {
            match self.inner.script_pop(script.as_script()) {
                Ok(Some(status)) => {
                    notifications_seen += 1;
                    *status_slot = Some(status);
                    *status_observed = true;
                    if cached.is_none_or(|entry| entry.status != Some(status)) {
                        *should_fetch = true;
                    }
                    if notifications_seen >= MAX_QUEUED_NOTIFICATIONS_PER_SCRIPT {
                        warn!(
                            "Electrum notification drain reached its per-script safety limit; refreshing history and continuing on the next work item"
                        );
                        *should_fetch = true;
                        break;
                    }
                }
                Ok(None) => {
                    // No queued notification means the warm history remains valid. A
                    // backend-specific ambiguous null is bounded by the six-hour
                    // authoritative history reconciliation in the caller, without adding one RPC
                    // per non-empty script to every recurring sync.
                    break;
                }
                Err(error) if is_not_subscribed(&error) => {
                    if after_hidden_reconnect {
                        warn!(
                            "Electrum notification read failed after resubscription; polling history for this work item: {error}"
                        );
                        *should_fetch = true;
                        break;
                    }
                    // electrum-client's retrying Client treats NotSubscribed as a
                    // transport failure: it replaces the raw socket, wiping every
                    // connection-scoped subscription. Per-script resubscribe would
                    // keep paying that reconnect for the rest of this work item.
                    self.subscribed
                        .lock()
                        .expect("subscription set lock poisoned")
                        .clear();
                    debug!(
                        "Electrum raw client lost subscription state; resubscribing the current work item"
                    );
                    return true;
                }
                Err(error) => {
                    warn!(
                        "Electrum notification read failed; polling history for this work item: {error}"
                    );
                    *should_fetch = true;
                    break;
                }
            }
        }
        false
    }

    /// Batch-subscribes the scripts this connection does not own yet and records the statuses
    /// the server returned. Scripts whose subscription cannot be recovered after a failed batch
    /// are marked so the caller polls their history for this work item.
    fn subscribe_cold_scripts(
        &self,
        scripts: &[ScriptBuf],
        cold_indices: &[usize],
        statuses: &mut [Option<ScriptStatus>],
        status_observed: &mut [bool],
        subscription_failed: &mut [bool],
    ) -> Result<(), Error> {
        self.ensure_work_active()?;
        match self
            .inner
            .batch_script_subscribe(cold_indices.iter().map(|&index| scripts[index].as_script()))
        {
            Ok(batch_statuses) if batch_statuses.len() == cold_indices.len() => {
                let mut subscribed = self
                    .subscribed
                    .lock()
                    .expect("subscription set lock poisoned");
                for (index, status) in cold_indices.iter().copied().zip(batch_statuses) {
                    subscribed.insert(scripts[index].clone());
                    statuses[index] = status;
                    status_observed[index] = true;
                }
            }
            Ok(batch_statuses) => {
                return Err(Error::Message(format!(
                    "Electrum batch subscription returned {} statuses for {} scripts",
                    batch_statuses.len(),
                    cold_indices.len()
                )));
            }
            Err(error) => {
                warn!(
                    "Electrum batch script subscription failed; reconciling partial subscriptions before polling fallback: {error}"
                );
                let recovered = self.recover_partial_subscription(
                    scripts,
                    cold_indices,
                    statuses,
                    status_observed,
                )?;
                if !recovered {
                    for &index in cold_indices {
                        subscription_failed[index] = true;
                    }
                }
            }
        }
        Ok(())
    }

    /// Unsubscribes whatever part of a failed batch subscription the server accepted, then
    /// resubscribes each script individually. Returns `false` when either step fails.
    fn recover_partial_subscription(
        &self,
        scripts: &[ScriptBuf],
        cold_indices: &[usize],
        statuses: &mut [Option<ScriptStatus>],
        status_observed: &mut [bool],
    ) -> Result<bool, Error> {
        let mut recovered = true;
        for &index in cold_indices {
            self.ensure_work_active()?;
            match self.inner.script_unsubscribe(scripts[index].as_script()) {
                Ok(_) => {}
                Err(error) if is_not_subscribed(&error) => {}
                Err(error) => {
                    warn!(
                        "Electrum partial batch subscription cleanup failed; polling history for this work item: {error}"
                    );
                    recovered = false;
                    break;
                }
            }
        }
        if recovered {
            for &index in cold_indices {
                self.ensure_work_active()?;
                match self.inner.script_subscribe(scripts[index].as_script()) {
                    Ok(status) => {
                        self.subscribed
                            .lock()
                            .expect("subscription set lock poisoned")
                            .insert(scripts[index].clone());
                        statuses[index] = status;
                        status_observed[index] = true;
                    }
                    Err(Error::AlreadySubscribed(_)) => {
                        self.subscribed
                            .lock()
                            .expect("subscription set lock poisoned")
                            .insert(scripts[index].clone());
                    }
                    Err(error) => {
                        warn!(
                            "Electrum individual subscription recovery failed; polling history for this work item: {error}"
                        );
                        recovered = false;
                        break;
                    }
                }
            }
        }
        Ok(recovered)
    }

    /// Fetches history for `fetch_indices` in one batch, stores it in the shared cache and fills
    /// the matching `results` slots.
    fn refresh_histories(
        &self,
        scripts: &[ScriptBuf],
        fetch_indices: Vec<usize>,
        statuses: &[Option<ScriptStatus>],
        status_observed: &[bool],
        now: Instant,
        results: &mut [Option<Vec<GetHistoryRes>>],
    ) -> Result<(), Error> {
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
        if fetched.len() != fetch_indices.len() {
            return Err(Error::Message(format!(
                "Electrum history batch returned {} results for {} scripts",
                fetched.len(),
                fetch_indices.len()
            )));
        }
        let mut cache = self.cache.0.lock().expect("history cache lock poisoned");
        for (index, history) in fetch_indices.into_iter().zip(fetched) {
            let effective_status = if status_observed[index] {
                statuses[index]
            } else if history.is_empty() {
                // An authoritative empty history has Electrum's null status. This also
                // resolves backends where a null notification is indistinguishable from an
                // empty local notification queue during the periodic reconciliation.
                None
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
        Ok(())
    }
}
