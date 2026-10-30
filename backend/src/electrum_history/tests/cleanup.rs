//! Unsubscribing and evicting scripts when wallets are removed.

use super::fake::{history, indexed_script, script, status, FakeApi};
use crate::electrum_history::cache::HistoryCacheEntry;
use crate::electrum_history::{SharedHistoryCache, SubscriptionHistoryClient};
use bdk_electrum::electrum_client::ElectrumApi;
use std::sync::{Arc, Barrier};
use std::time::Instant;

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

    adapter
        .forget_scripts(std::slice::from_ref(&removed))
        .unwrap();

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
fn wallet_cleanup_handles_large_subscription_sets() {
    let api = FakeApi::default();
    let cache = SharedHistoryCache::default();
    let adapter = SubscriptionHistoryClient::new(api.clone(), cache.clone(), true);
    let scripts: Vec<_> = (0..512).map(indexed_script).collect();

    adapter
        .subscribed
        .lock()
        .unwrap()
        .extend(scripts.iter().cloned());
    api.state().subscribed.extend(scripts.iter().cloned());
    cache
        .0
        .lock()
        .unwrap()
        .entries
        .extend(scripts.iter().cloned().map(|script| {
            (
                script,
                HistoryCacheEntry {
                    status: None,
                    history: Vec::new(),
                    last_revalidated: Instant::now(),
                    dirty: false,
                },
            )
        }));

    adapter.forget_scripts(&scripts).unwrap();

    assert_eq!(api.state().unsubscribe_calls, scripts.len());
    assert!(api.state().subscribed.is_empty());
    assert!(adapter.subscribed.lock().unwrap().is_empty());
    assert!(cache.0.lock().unwrap().entries.is_empty());
}

#[test]
fn wallet_cleanup_stops_between_rpcs_after_cancellation() {
    let api = FakeApi::default();
    let cache = SharedHistoryCache::default();
    let adapter = Arc::new(SubscriptionHistoryClient::new(
        api.clone(),
        cache.clone(),
        true,
    ));
    let scripts = vec![indexed_script(1), indexed_script(2)];
    adapter
        .subscribed
        .lock()
        .unwrap()
        .extend(scripts.iter().cloned());
    api.state().subscribed.extend(scripts.iter().cloned());
    cache
        .0
        .lock()
        .unwrap()
        .entries
        .extend(scripts.iter().cloned().map(|script| {
            (
                script,
                HistoryCacheEntry {
                    status: None,
                    history: Vec::new(),
                    last_revalidated: Instant::now(),
                    dirty: false,
                },
            )
        }));
    let started = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    {
        let mut state = api.state();
        state.unsubscribe_started = Some(Arc::clone(&started));
        state.unsubscribe_release = Some(Arc::clone(&release));
    }

    let worker_adapter = Arc::clone(&adapter);
    let worker_scripts = scripts.clone();
    let worker = std::thread::spawn(move || worker_adapter.forget_scripts(&worker_scripts));
    started.wait();
    adapter.cancel_work();
    release.wait();

    assert!(worker.join().unwrap().is_err());
    assert_eq!(api.state().unsubscribe_calls, 1);
    assert_eq!(api.state().subscribed.len(), 1);
    assert!(adapter.subscribed.lock().unwrap().is_empty());
    assert!(cache.0.lock().unwrap().entries.is_empty());
}

#[test]
fn wallet_cleanup_error_requires_connection_replacement() {
    let api = FakeApi::default();
    let cache = SharedHistoryCache::default();
    let adapter = SubscriptionHistoryClient::new(api.clone(), cache.clone(), true);
    let script = script(40);
    adapter.subscribed.lock().unwrap().insert(script.clone());
    {
        let mut state = api.state();
        state.subscribed.insert(script.clone());
        state.fail_unsubscribes = true;
    }
    cache.0.lock().unwrap().entries.insert(
        script.clone(),
        HistoryCacheEntry {
            status: None,
            history: Vec::new(),
            last_revalidated: Instant::now(),
            dirty: false,
        },
    );

    assert!(adapter
        .forget_scripts(std::slice::from_ref(&script))
        .is_err());
    assert!(api.state().subscribed.contains(&script));
    assert!(adapter.subscribed.lock().unwrap().is_empty());
    assert!(cache.0.lock().unwrap().entries.is_empty());
}

#[test]
fn wrapped_not_subscribed_unsubscribe_is_not_a_cleanup_failure() {
    let api = FakeApi::default();
    let cache = SharedHistoryCache::default();
    let adapter = SubscriptionHistoryClient::new(api.clone(), cache.clone(), true);
    let script = script(41);
    adapter.subscribed.lock().unwrap().insert(script.clone());
    api.state().wrap_not_subscribed = true;
    cache.0.lock().unwrap().entries.insert(
        script.clone(),
        HistoryCacheEntry {
            status: None,
            history: Vec::new(),
            last_revalidated: Instant::now(),
            dirty: false,
        },
    );

    adapter
        .forget_scripts(std::slice::from_ref(&script))
        .unwrap();

    assert!(adapter.subscribed.lock().unwrap().is_empty());
    assert!(cache.0.lock().unwrap().entries.is_empty());
    assert_eq!(api.state().unsubscribe_calls, 1);
}
