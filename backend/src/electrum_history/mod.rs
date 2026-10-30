//! Subscription-aware Electrum history for recurring wallet syncs.
//!
//! BDK asks for the history of every revealed script on each sync. This module wraps the
//! Electrum client so those requests are answered from a cache and refetched only when a
//! script subscription, reconnect or periodic safety check says the history may have changed.
//!
//! - `cache` owns the history cache that outlives individual connections.
//! - `client` owns the adapter, its subscription set and its timeout/cancellation state.
//! - `history` owns the cached, subscription-driven batch history lookup.
//! - `api` forwards the rest of `ElectrumApi` to the wrapped client.

mod api;
mod cache;
mod client;
mod history;
#[cfg(test)]
mod tests;

pub(crate) use cache::SharedHistoryCache;
pub(crate) use client::SubscriptionHistoryClient;

use bdk_electrum::electrum_client::Error;

/// True when the error is a direct `NotSubscribed` or that error wrapped by
/// electrum-client's retrying `Client` as `AllAttemptsErrored`.
fn is_not_subscribed(error: &Error) -> bool {
    match error {
        Error::NotSubscribed(_) => true,
        Error::AllAttemptsErrored(errors) => errors.iter().any(is_not_subscribed),
        _ => false,
    }
}
