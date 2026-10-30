//! Cache hits, notification-driven refreshes, safety revalidation and batch validation.

use super::fake::{FakeApi, history, indexed_script, script, status};
use crate::electrum_history::history::{
    HISTORY_REVALIDATION_INTERVAL, MAX_QUEUED_NOTIFICATIONS_PER_SCRIPT,
};
use crate::electrum_history::{SharedHistoryCache, SubscriptionHistoryClient};
use bdk_electrum::electrum_client::ElectrumApi;
use bdk_wallet::bitcoin::ScriptBuf;
use std::time::Instant;

#[test]
fn electrs_warning_condition_is_avoided_and_warm_history_is_cached() {
    let api = FakeApi::default();
    let script = script(1);
    {
        let mut state = api.state();
        state.statuses.insert(script.clone(), Some(status(1)));
        state.histories.insert(script.clone(), history(1));
    }
    let adapter = SubscriptionHistoryClient::new(api.clone(), SharedHistoryCache::default(), true);

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
    assert_eq!(state.unsubscribed_history_calls, 0);
    assert_eq!(state.ping_calls, 2);
}

#[test]
fn large_cold_cache_subscribes_once_per_bdk_batch() {
    let api = FakeApi::default();
    let adapter = SubscriptionHistoryClient::new(api.clone(), SharedHistoryCache::default(), true);
    let scripts: Vec<_> = (0..512).map(indexed_script).collect();

    for batch in scripts.chunks(20) {
        adapter
            .batch_script_get_history(batch.iter().map(bdk_wallet::bitcoin::ScriptBuf::as_script))
            .unwrap();
    }
    for batch in scripts.chunks(20) {
        adapter
            .batch_script_get_history(batch.iter().map(bdk_wallet::bitcoin::ScriptBuf::as_script))
            .unwrap();
    }

    let state = api.state();
    assert_eq!(state.subscription_batch_calls, scripts.len().div_ceil(20));
    assert_eq!(state.subscription_calls, scripts.len());
    assert_eq!(state.history_batches.len(), scripts.len().div_ceil(20));
    assert_eq!(state.unsubscribed_history_calls, 0);
}

#[test]
fn partial_batch_subscription_is_reconciled_without_an_already_subscribed_loop() {
    let api = FakeApi::default();
    let scripts = [script(30), script(31), script(32)];
    {
        let mut state = api.state();
        state.partial_batch_failure_after = Some(1);
        for (seed, script) in [(30, &scripts[0]), (31, &scripts[1]), (32, &scripts[2])] {
            state.statuses.insert(script.clone(), Some(status(seed)));
            state.histories.insert(script.clone(), history(seed));
        }
    }
    let adapter = SubscriptionHistoryClient::new(api.clone(), SharedHistoryCache::default(), true);

    let cold = adapter
        .batch_script_get_history(scripts.iter().map(ScriptBuf::as_script))
        .unwrap();
    let warm = adapter
        .batch_script_get_history(scripts.iter().map(ScriptBuf::as_script))
        .unwrap();

    let cold_heights: Vec<_> = cold.iter().map(|history| history[0].height).collect();
    let warm_heights: Vec<_> = warm.iter().map(|history| history[0].height).collect();
    assert_eq!(cold_heights, [30, 31, 32]);
    assert_eq!(warm_heights, cold_heights);
    let state = api.state();
    assert_eq!(state.subscription_batch_calls, 1);
    assert_eq!(state.subscription_calls, 4);
    assert_eq!(state.unsubscribe_calls, scripts.len());
    assert_eq!(state.subscribed.len(), scripts.len());
    assert_eq!(state.history_batches.len(), 1);
    assert_eq!(state.unsubscribed_history_calls, 0);
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
    let adapter = SubscriptionHistoryClient::new(api.clone(), SharedHistoryCache::default(), true);
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
    assert!(
        adapter
            .batch_script_get_history([script.as_script()])
            .is_err()
    );
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
fn safety_revalidation_clears_non_empty_history_after_ambiguous_null() {
    let api = FakeApi::default();
    let script = script(25);
    {
        let mut state = api.state();
        state.statuses.insert(script.clone(), Some(status(25)));
        state.histories.insert(script.clone(), history(25));
    }
    let cache = SharedHistoryCache::default();
    let adapter = SubscriptionHistoryClient::new(api.clone(), cache.clone(), true);
    let start = Instant::now();
    adapter
        .histories_at(std::slice::from_ref(&script), start)
        .unwrap();

    {
        let mut state = api.state();
        state.statuses.insert(script.clone(), None);
        state.histories.insert(script.clone(), Vec::new());
    }
    let refreshed = adapter
        .histories_at(
            std::slice::from_ref(&script),
            start + HISTORY_REVALIDATION_INTERVAL,
        )
        .unwrap();

    assert!(refreshed[0].is_empty());
    assert_eq!(api.state().history_batches.len(), 2);
    assert_eq!(cache.0.lock().unwrap().entries[&script].status, None);
}

#[test]
fn sparse_batch_refresh_keeps_status_with_its_original_script() {
    let api = FakeApi::default();
    let scripts = [script(10), script(11), script(12)];
    {
        let mut state = api.state();
        for (offset, script) in scripts.iter().enumerate() {
            let seed = 10 + u8::try_from(offset).unwrap();
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
fn malformed_history_batch_lengths_return_errors() {
    for response_len in [1, 3] {
        let api = FakeApi::default();
        let scripts = [script(41), script(42)];
        {
            let mut state = api.state();
            state.history_response_len = Some(response_len);
            for (seed, script) in [(41, &scripts[0]), (42, &scripts[1])] {
                state.statuses.insert(script.clone(), Some(status(seed)));
                state.histories.insert(script.clone(), history(seed));
            }
        }
        let adapter = SubscriptionHistoryClient::new(api, SharedHistoryCache::default(), true);

        let error = adapter
            .batch_script_get_history(scripts.iter().map(ScriptBuf::as_script))
            .unwrap_err();

        assert!(error.to_string().contains(&format!(
            "returned {response_len} results for {} scripts",
            scripts.len()
        )));
    }
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
    let adapter = SubscriptionHistoryClient::new(api.clone(), SharedHistoryCache::default(), true);
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
    let adapter = SubscriptionHistoryClient::new(api.clone(), SharedHistoryCache::default(), true);
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
    let fallback =
        SubscriptionHistoryClient::new(fallback_api.clone(), SharedHistoryCache::default(), true);
    fallback
        .batch_script_get_history([first_script.as_script()])
        .unwrap();
    fallback
        .batch_script_get_history([first_script.as_script()])
        .unwrap();
    assert_eq!(fallback_api.state().history_batches.len(), 2);
}
