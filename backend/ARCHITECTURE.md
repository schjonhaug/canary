# Backend architecture

This document describes how the Canary backend is put together: what runs
when the process starts, how a transaction on the chain becomes a notification,
where data is stored and how errors are handled. It is a map, not a reference;
the module docs and the code are the source of truth.

## Overview

The backend is a single Rust binary built on axum and tokio. It watches
Bitcoin wallets (output descriptors, xpubs and single addresses) through an
Electrum server, records their transactions in SQLite and notifies each
wallet's contacts when funds move or a balance threshold is crossed. It never
holds private keys.

The process runs in one of two operating modes, selected by the required
`CANARY_MODE` variable (`OperatingMode` in `src/config.rs`):

| | `self-hosted` | `cloud` |
|---|---|---|
| Users | One built-in admin user | Registered users with email/password and JWT sessions |
| Providers | ntfy, JSON webhook, Telegram, Nostr DM | Email (Resend), SMS (Twilio), ntfy |
| Billing | None | Stripe or BTCPay Server, subscription tiers |
| Sync schedule | One fair queue, `CANARY_SYNC_INTERVAL` per wallet | One loop per subscription tier |

`CANARY_RESTORE_DRILL` is an additional switch for restore testing that
disables billing and customer notifications.

## Process layout

`src/main.rs` is deliberately thin. It installs the rustls crypto provider,
loads `.env` with `dotenvy`, initialises tracing, parses `AppConfig` from CLI
arguments and the environment, and calls `canary::app::run`.

`app::run` (`src/app/mod.rs`) assembles the application:

1. Validates the configuration for the selected mode.
2. Creates the `TransactionNotification` broadcast channel (capacity 100).
3. Builds `WalletManager`, which connects to Electrum, opens the metadata
   database (running migrations), and loads active wallets into memory.
4. Builds `AppServices`, the bundle the HTTP layer uses: the `MetadataDb` and
   a `WalletCreationService`. Handlers do not need the wallet manager's
   in-memory wallets, so they do not get it.
5. Starts the exchange rate refresh task, registers notification providers
   (`app/providers.rs`), initialises Stripe in cloud mode and seeds demo
   wallets (`app/demo.rs`) when the demo user exists.
6. Spawns the background tasks below, then binds `CANARY_BIND_ADDRESS` and
   serves the router.

Background tasks (`src/app/tasks.rs` unless noted):

| Task | What it does |
|---|---|
| Wallet sync | Self-hosted: `WalletManager::run_self_hosted_sync_queue`, an oldest-first due-time queue. Cloud: one `interval` loop per `SubscriptionTier` calling `sync_tier_parallel` (up to 10 wallets at once). |
| Block header sync | Polls the Electrum chain tip and stores it in the metadata DB. |
| Session cleanup | Deletes expired login sessions every hour. |
| Notification dispatcher | `app/dispatcher.rs`; consumes the broadcast channel. |
| Subscription limits | Cloud only; re-applies tier limits for every account once at startup. |
| Startup checks | `MetadataDb::run_startup_checks`; logs database integrity problems. |
| Email queue worker | Cloud only; `email_queue.rs`, a rate-limited batching queue that the email provider writes to. |

### HTTP layer

`api::create_router_with_services` (`src/api.rs`) builds one `AppState` and
implements `FromRef` for each part, so handlers and extractors take only what
they need (`State<AppServicesState>`, `State<ConfigState>` and so on). All
routes are nested under `/api`. Stripe, BTCPay webhook and donation routes are
only mounted when that integration is configured.

Two layers wrap the router: a CORS layer restricted to the configured frontend
origins, and `validate_browser_origin`, which rejects unsafe-method requests
whose `Origin`/`Referer` does not match a configured frontend origin. In
self-hosted mode it also accepts same-origin requests that arrive through the
frontend proxy. Requests without an origin and without a session cookie, such
as provider webhooks and bearer-token clients, are not affected.

Handlers live in `src/handlers/`, grouped by domain. The `AuthenticatedUser`
extractor (`src/extractors/auth.rs`) validates the JWT from the `HttpOnly`
cookie or the `Authorization` header. Request and response DTOs are in
`src/models/`.

## From a block to a notification

```
 Electrum server
       |
       |  BDK full_scan / sync (descriptor wallets)
       |  script_get_history / script_get_balance (address watches)
       v
 WalletSyncService (sync.rs)
   - diffs chain state against the transactions table
   - classifies receive / send, pending / confirmed, RBF, CPFP
   - updates the balance, then checks balance alerts
       |
       |  TransactionNotification::{Pending, Confirmed, BalanceAlert}
       v
 tokio::sync::broadcast channel
       |
       v
 NotificationDispatcher (app/dispatcher.rs)
   - loads the wallet and its contacts
   - filters contacts by their per-event settings
   - calls every registered NotificationProvider
       |
       +--> ntfy, webhook, Telegram, Nostr   (self-hosted)
       +--> email queue, Twilio SMS, ntfy    (cloud)
       |
       v
 notification_logs table (one row per delivery attempt)
```

`WalletSyncService` lives in `src/sync.rs`. For descriptor wallets it syncs the
BDK wallet through `ElectrumClient`, then compares BDK's canonical
transactions with the stored rows. New transactions are inserted and
broadcast as `Pending` (or directly as confirmed if they were mined before the
first sync); a stored pending transaction that gains a confirmation is
broadcast as `Confirmed`; conflicted transactions in BDK's graph that share
inputs with a newer canonical one are marked as replaced (RBF). The wallet's
balance is updated before the events are processed, so notifications show
the current balance. Balance alerts are then evaluated and fire as
`BalanceAlert` events.

Publishing to the channel never fails a sync. If the dispatcher falls behind,
the channel drops the oldest events and the dispatcher logs how many it lost.

The dispatcher builds the ntfy provider per user, because self-hosted users
can point Canary at their own ntfy server and credentials. It writes every
delivery attempt, successful or not, to `notification_logs`, and tracks
consecutive SMS and email failures (`notification_failure_tracker.rs`) to
alert the operator through `admin_notifications.rs`.

`src/electrum.rs` wraps `bdk_electrum`. `ElectrumClientManager` owns the
current connection, reconnects after transport errors, coordinates reconnects
across parallel syncs and counts consecutive failures for alerting.
`src/electrum_history.rs` adds a `SubscriptionHistoryClient` adapter used in
self-hosted mode: it subscribes to script hashes and only refetches history for
scripts whose status changed, with a periodic revalidation, so recurring syncs
against a home node do not re-query every revealed script.

## Storage

There are two kinds of SQLite file, both under
`{CANARY_DATA_DIR}/{network}/`:

- **BDK wallet files**, `wallets/{checksum}.sqlite`, one per descriptor
  wallet. Each holds BDK's own persisted state (`PersistedWallet<Connection>`).
  `WalletManager` keeps the loaded wallets in a map with a mutex per wallet,
  so different wallets sync in parallel.
- **The metadata database**, `metadata.sqlite`. Everything Canary owns:
  users, sessions, wallets, contacts and their notification methods,
  transactions, balance alerts, notification logs, billing state and
  settings. It is accessed only through `MetadataDb` (`src/metadata/`), which
  wraps an r2d2 pool (16 connections, WAL, foreign keys on, 5 s busy timeout).

rusqlite and r2d2 are synchronous, and a query can block on disk I/O or on
the busy timeout. Running them on a tokio worker would stall every other task
on that worker, so `MetadataDb` methods run their queries inside
`tokio::task::spawn_blocking` and callers simply `.await` them. BDK wallet
loading and the blocking `electrum-client` calls are handled the same way.

### Migrations

Schema migrations are plain SQL files in `backend/migrations/`, named
`NNN_description.sql`. `MigrationRunner` (`src/migrations.rs`) runs when
`MetadataDb` is created: it applies files in name order, records each version
in `schema_migrations`, and skips versions already recorded. The migration
tests are in `tests/migrations_tests.rs`.

## Error handling

- **HTTP.** Handlers return `ApiResult<T>`, an alias for
  `Result<T, ApiError>` (`src/handlers/error.rs`). `ApiError` implements
  `IntoResponse` and renders the JSON `ErrorResponse` the frontend expects:
  `Client` carries a status, an optional machine-readable `error_code` that the
  frontend translates, and a message; `RateLimited` adds `Retry-After`;
  `Internal` logs its source and returns only a fixed message; `Opaque` is a
  500 whose cause is never logged because it may contain customer or billing
  identifiers. `OrInternal::or_internal` converts any error into `Internal`,
  so database and provider details never reach the client.
- **Electrum.** `src/electrum/error.rs` defines `ElectrumError` with
  `thiserror`. `ElectrumError::kind()` classifies a failure (`Transport`,
  `Timeout`, `Server` and so on) from the typed `electrum_client::Error` and
  `io::ErrorKind`, and the sync loop uses that to decide whether to replace
  the connection.
- **Elsewhere.** Many library modules, including `metadata/`, `sync.rs` and
  `wallet.rs`, still return `anyhow::Result`. The direction is typed errors per
  domain with `anyhow` limited to startup (`main`, `app::run`, config
  loading); at the HTTP boundary `From<anyhow::Error> for ApiError` turns a
  remaining `anyhow` error into a generic 500.

## Design decisions

**`async_trait` for `NotificationProvider`.** The dispatcher holds providers
as `Arc<dyn NotificationProvider>` in `NotificationManager` and picks them at
runtime from configuration. Native `async fn` in traits is not
dyn-compatible, so the trait uses `#[async_trait]`, which boxes the returned
future. Traits that are not used as `dyn` objects do not need it.

**Address watches bypass BDK.** A watched address is stored as a standard
BIP-385 `addr()` descriptor (or `pk()` for bare public keys). `rust-miniscript`,
and therefore BDK, does not support `addr()`, so no BDK `Wallet` can be built
for it. `WalletSyncService` instead queries Electrum directly with
`script_get_history` and `script_get_balance` and diffs the result against the
database. The doc comment on `script_from_watch_descriptor` in `src/sync.rs`
records why `bdk_chain`'s `SpkTxOutIndex`/`IndexedTxGraph` was not used. Since
the stored descriptors are already standard, a move to BDK wallets later would
need no data migration. Several users watching the same address share one
Electrum query per sync.

**Vendored `postcard`.** `phonenumber` enables `postcard`'s default
`heapless-cas` feature, which pulls in the unmaintained `atomic-polyfill`
crate (RUSTSEC-2023-0089). Cargo cannot disable another crate's additive
features, so `Cargo.toml` patches `postcard` to `vendor/postcard`, a copy of
the crates.io release with only the default feature list changed.
[`vendor/postcard/CANARY-PATCH.md`](vendor/postcard/CANARY-PATCH.md) explains
how to refresh or remove it.

**Hand-rolled migration runner.** Migrations run one statement at a time
rather than through `execute_batch`. That lets the runner skip a
`duplicate column name` error from an `ALTER TABLE ... ADD COLUMN`, which is
how a migration interrupted halfway recovers on the next start. Splitting on
`;` is not enough, because trigger bodies and string literals contain
semicolons. The runner instead accumulates text up to each `;` and asks
SQLite's own tokenizer, via `sqlite3_complete`, whether it is a complete
statement. That FFI call is the only `unsafe` block in the crate
(`unsafe_code` is denied crate-wide and allowed there with `#[expect]`).

## Module map

| Path | Responsibility |
|---|---|
| `main.rs` | Process setup; calls `app::run` |
| `app/` | Assembly (`mod.rs`), background tasks, dispatcher, provider registration, demo seeding, logging |
| `config.rs` | `AppConfig`: CLI arguments and environment, mode and network settings |
| `api.rs` | Router, `AppState`, `AppServices`, CORS and origin checks |
| `handlers/` | HTTP handlers by domain, `ApiError` |
| `extractors/` | `AuthenticatedUser` extractor |
| `models/` | Request/response DTOs and validators |
| `wallet.rs` | `WalletManager` (loaded wallets, sync scheduling, subscription limits) and `WalletCreationService` |
| `sync.rs` | `WalletSyncService`: transaction detection, RBF/CPFP, balance alerts, address watches |
| `electrum.rs`, `electrum_history.rs` | Electrum client, reconnection, subscription-based history cache |
| `metadata/` | `MetadataDb` and its queries, split by table group; integrity checks |
| `migrations.rs` | SQL migration runner |
| `notifications.rs` | `NotificationProvider` trait, `NotificationManager`, contact filtering |
| `*_provider.rs` | ntfy, webhook, Telegram, Nostr, email, Twilio providers |
| `message_formatter.rs`, `locales/` | Localised notification text (`rust-i18n`) |
| `email_queue.rs`, `email_service.rs` | Email delivery queue and transactional email |
| `notification_failure_tracker.rs`, `admin_notifications.rs` | Provider failure tracking and operator alerts |
| `auth.rs`, `admin_mfa.rs`, `rate_limit.rs` | Sessions and JWTs, admin second factor, in-memory rate-limit fallback |
| `subscription.rs`, `stripe_billing.rs`, `stripe_client_service.rs`, `btcpay_client.rs` | Tiers and limits, billing integrations |
| `outbound_target.rs` | SSRF policy for outbound HTTP (ntfy, webhooks) |
| `exchange_rates.rs` | Fiat exchange rate refresh |
| `xpub_converter.rs`, `utils/` | Key format conversion, descriptor parsing, amounts, time |

## Testing

There are three layers. `cargo test` runs the first two; the third needs
Docker.

1. **Unit tests** sit in `#[cfg(test)] mod tests` blocks next to the code,
   plus a few cross-module suites in `src/tests/`.
2. **Integration tests** in `tests/` exercise the HTTP handlers (by calling
   the router in-process) and the database layer against temporary SQLite
   files, for example `wallet_handlers_tests.rs`, `auth_flow_tests.rs`,
   `migrations_tests.rs` and `database_integrity_tests.rs`. Each test builds its
   own `AppConfig` and temporary database, so they run in parallel.
3. **System tests** in `system_tests/` start Bitcoin Core and Fulcrum in Docker
   on regtest, send real transactions and assert on the events Canary records.
   Each file is a `[[test]]` target in `Cargo.toml` and every test is
   `#[ignore]`d. Run them with `./run-system-tests.sh`; see
   [`system_tests/README.md`](system_tests/README.md).

CI (`.github/workflows/backend.yml`) runs `cargo fmt --check`,
`cargo clippy --all-targets -- -D warnings`, `cargo doc --no-deps` with
`RUSTDOCFLAGS="-D warnings"` and `cargo test`. It does not
run the system tests.
