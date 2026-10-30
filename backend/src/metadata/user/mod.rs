//! User-scoped `MetadataDb` methods, split by responsibility.
//!
//! Every submodule adds an `impl MetadataDb` block; none of them define public
//! types, so nothing is re-exported from here.

mod accounts;
mod btcpay;
mod checkout;
mod lockout;
mod preferences;
mod rate_limit;
mod seed;
mod sessions;
mod stripe_webhooks;
mod subscription;
mod tokens;
