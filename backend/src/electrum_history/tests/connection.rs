//! Cancellation, shared timeout state, reconnects and lost server-side subscriptions.

use super::fake::{history, script, status, FakeApi};
use crate::electrum_history::{is_not_subscribed, SharedHistoryCache, SubscriptionHistoryClient};
use bdk_electrum::electrum_client::{ElectrumApi, Error, ToElectrumScriptHash};

#[test]
fn cancelled_work_stops_before_the_next_electrum_rpc() {
    let api = FakeApi::default();
    let adapter = SubscriptionHistoryClient::new(api.clone(), SharedHistoryCache::default(), true);

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
fn not_subscribed_helper_peels_all_attempts_errored() {
    let hash = script(1).as_script().to_electrum_scripthash();
    assert!(is_not_subscribed(&Error::NotSubscribed(hash)));
    assert!(is_not_subscribed(&Error::AllAttemptsErrored(vec![
        Error::NotSubscribed(hash)
    ])));
    assert!(is_not_subscribed(&Error::AllAttemptsErrored(vec![
        Error::Message("other".to_string()),
        Error::AllAttemptsErrored(vec![Error::NotSubscribed(hash)]),
    ])));
    assert!(!is_not_subscribed(&Error::Message("other".to_string())));
    assert!(!is_not_subscribed(&Error::AllAttemptsErrored(vec![
        Error::Message("other".to_string())
    ])));
}

#[test]
fn wrapped_not_subscribed_after_hidden_reconnect_resubscribes_batch_once() {
    let cache = SharedHistoryCache::default();
    let api = FakeApi::default();
    let first = script(7);
    let second = script(8);
    {
        let mut state = api.state();
        state.wrap_not_subscribed = true;
        state.statuses.insert(first.clone(), Some(status(7)));
        state.statuses.insert(second.clone(), Some(status(8)));
        state.histories.insert(first.clone(), history(7));
        state.histories.insert(second.clone(), history(8));
    }
    let adapter = SubscriptionHistoryClient::new(api.clone(), cache.clone(), true);
    adapter
        .batch_script_get_history([first.as_script(), second.as_script()])
        .unwrap();

    {
        let mut state = api.state();
        state.subscribed.clear();
        state.statuses.insert(second.clone(), Some(status(9)));
        state.histories.insert(second.clone(), history(9));
    }

    let histories = adapter
        .batch_script_get_history([first.as_script(), second.as_script()])
        .unwrap();

    assert_eq!(histories[0][0].height, 7);
    assert_eq!(histories[1][0].height, 9);
    let state = api.state();
    assert_eq!(state.raw_client_replacements, 1);
    assert!(state.subscribed.contains(&first));
    assert!(state.subscribed.contains(&second));
    assert_eq!(state.history_batches.len(), 2);
    assert_eq!(state.history_batches[1], vec![second.clone()]);
    assert_eq!(cache.0.lock().unwrap().reconnect_resubscriptions, 2);
}

#[test]
fn wrapped_not_subscribed_after_retry_falls_back_to_polling() {
    let api = FakeApi::default();
    let script = script(11);
    {
        let mut state = api.state();
        state.wrap_not_subscribed = true;
        state.statuses.insert(script.clone(), Some(status(11)));
        state.histories.insert(script.clone(), history(11));
    }
    let adapter = SubscriptionHistoryClient::new(api.clone(), SharedHistoryCache::default(), true);
    adapter
        .batch_script_get_history([script.as_script()])
        .unwrap();

    {
        let mut state = api.state();
        state.subscribed.clear();
        state.fail_pops = true;
    }

    let histories = adapter
        .batch_script_get_history([script.as_script()])
        .unwrap();

    assert_eq!(histories[0][0].height, 11);
    let state = api.state();
    assert_eq!(state.raw_client_replacements, 2);
    assert_eq!(state.history_batches.len(), 2);
    assert_eq!(state.history_batches[1], vec![script.clone()]);
}

#[test]
fn ping_failure_clears_local_subscriptions() {
    let api = FakeApi::default();
    let script = script(10);
    {
        let mut state = api.state();
        state.statuses.insert(script.clone(), Some(status(10)));
        state.histories.insert(script.clone(), history(10));
    }
    let adapter = SubscriptionHistoryClient::new(api.clone(), SharedHistoryCache::default(), true);
    adapter
        .batch_script_get_history([script.as_script()])
        .unwrap();
    assert!(adapter.subscribed.lock().unwrap().contains(&script));

    api.state().fail_ping = true;
    adapter.prepare_recurring_work();

    assert!(adapter.subscribed.lock().unwrap().is_empty());
    assert_eq!(api.state().ping_calls, 1);
}
