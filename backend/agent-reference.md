# Backend Reference

## Project Overview

Backend-specific reference for the Rust service. For the product overview,
frontend commands and cross-cutting conventions see the
[root agent-reference.md](../agent-reference.md); for the architecture see
[ARCHITECTURE.md](ARCHITECTURE.md).

- **Stack**: Axum and tokio, BDK wallet with Electrum sync, SQLite (BDK wallet files plus an r2d2-pooled metadata database)
- **Authentication**: Optional JWT-based multi-user system with email verification (cloud mode)
- **Billing**: Stripe integration with subscription management and webhook processing
- **Notifications**: Plugin-based system supporting ntfy.sh, Twilio SMS, Resend email, Nostr DMs, JSON webhooks, and Telegram bots

## Development Commands

```bash
# REQUIRED FIRST (cloud mode): Start Stripe CLI webhook forwarding for billing
stripe listen --forward-to localhost:3000/api/stripe/webhook

# Run backend (network comes from CANARY_NETWORK in .env)
cd backend && cargo run

# Override the network or Electrum server
cargo run -- --network testnet
cargo run -- --network mainnet --electrum-url ssl://electrum.blockstream.info:50002
CANARY_NETWORK=regtest CANARY_ELECTRUM_URL=tcp://127.0.0.1:50001 cargo run

# Checks (same as `.agent-loop/checks.sh backend` from the repo root)
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test -- --test-threads=1
# CI additionally builds the docs with warnings denied
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps

# Docker regtest system tests
./run-system-tests.sh
```

The toolchain is pinned in [rust-toolchain.toml](../rust-toolchain.toml). Local regtest services
(`scripts/dev.sh`) are described in the root reference and
[scripts/README.md](../scripts/README.md).

## Configuration

### Environment Setup
All configuration is explicit via environment variables - no hardcoded defaults.

**Quick Start:**
- **self-hosted mode**: `cp .env.example.self-hosted .env`
- **cloud mode**: `cp .env.example.cloud .env`

**Configuration Files:**
- `.env.example.self-hosted` - Self-hosted single-user configuration
- `.env.example.cloud` - Hosted service with billing configuration  

The backend will fail fast with clear error messages if required variables are missing.

## Code Architecture

### Backend Structure
See the module map in [ARCHITECTURE.md](ARCHITECTURE.md#module-map).

**Key Patterns:**
- **Domain-driven Handler Organization**: Handlers grouped by domain (auth, wallet, billing, etc.)
- **Custom Axum Extractors**: `AuthenticatedUser` extractor for type-safe authentication
- **Unified AppState**: Single state struct with `FromRef` implementations for handler access
- **Non-blocking Web Architecture**: `AppServices` struct provides fast metadata access without wallet mutex locks
- **Dual State Management**: Web endpoints use `AppServices`, sync operations use `WalletManager` with Arc<Mutex<T>>
- **Background Sync Tasks**: Heavy wallet operations run in separate async tasks to avoid blocking web serving
- **Performance Monitoring**: Comprehensive timing logs for mutex wait times and sync operations
- Plugin-based notification architecture with trait objects
- Tier-based wallet sync intervals (Personal: 10min mainnet, Team: 2min mainnet; 30s regtest Personal, 15s regtest Team)
- Automatic wallet cleanup during sync cycles (soft delete → hard delete)
- Network-specific SQLite databases in `{CANARY_DATA_DIR}/{network}/`; the `.env` templates set `CANARY_DATA_DIR` to `./database/cloud` or `./database/self-hosted`

### Database Schema
- **Migrations**: numbered SQL files in `migrations/` (`NNN_description.sql`), applied in order by `MigrationRunner` and recorded in `schema_migrations`
- **UUID Primary Keys**: Used for security-critical tables (users, transaction_events)
- **Subscription Management**: Built-in user tiers, Stripe integration fields
- **Security Tables**: Login rate limiting (`login_attempts`), OTP brute-force protection (`otp_verification_attempts`)
- **Network Isolation**: Separate databases per Bitcoin network (regtest/testnet/mainnet)

### Notification System Architecture
- **Provider Registration**: Runtime registration based on environment variables
- **Trait-based**: Common `NotificationProvider` trait for extensibility
- **Multi-method Support**: Contacts can have multiple notification methods
- **Auto-detection**: Phone numbers → SMS, email addresses → email, topics → ntfy
- **Delivery Tracking**: Database logging with success/failure status
- **Failure Tracking**: Consecutive failure monitoring for SMS/email with throttled admin alerts after 3 failures
- **Email Queue**: Background batching (up to 100 per batch) with retry logic and rate limiting

## Environment Configuration

### Backend Environment Variables
```bash
# Network configuration
CANARY_NETWORK=regtest|testnet|mainnet
CANARY_ELECTRUM_URL=ssl://electrum.blockstream.info:50002
CANARY_SYNC_INTERVAL=60

# Authentication (optional)
CANARY_MODE=cloud
JWT_SECRET=your_secure_jwt_secret

# Stripe billing
STRIPE_SECRET_KEY=sk_test_...
STRIPE_WEBHOOK_SECRET=whsec_...

# Notification providers
# Twilio will be auto-enabled in cloud mode if configured
TWILIO_ACCOUNT_SID=your_account_sid
TWILIO_AUTH_TOKEN=your_auth_token
# Sender ID: alphanumeric name (e.g., "Canary"), phone number, or Messaging Service SID
TWILIO_SENDER_ID=Canary

# Email provider (Resend)
RESEND_API_KEY=re_...
RESEND_FROM_EMAIL=notifications@canarybitcoin.com

# Telegram Bot (self-hosted only; Settings is preferred, env is an optional fallback)
# TELEGRAM_BOT_TOKEN=123456:ABC-DEF

# Transaction explorers (self-hosted only)
# CANARY_MEMPOOL_URL=http://umbrel.local:3006            # Full URL to custom instance
# CANARY_BITFEED_URL=http://umbrel.local:8314            # Full URL to custom instance
# CANARY_BTC_RPC_EXPLORER_URL=http://umbrel.local:3002   # Full URL to custom instance
# CANARY_MEMPOOL_PORT=3006                               # Auto-detected on Umbrel via exports.sh
# CANARY_BITFEED_PORT=8314                               # Auto-detected on Umbrel via exports.sh
# CANARY_BTC_RPC_EXPLORER_PORT=3002                      # Auto-detected on Umbrel via exports.sh
# CANARY_TX_EXPLORER_PLATFORM=umbrel                     # Optional platform label for local explorers

# Optional SOCKS5 proxy for NIP-17 .onion inbox relays (self-hosted).
# Requires system Tor or equivalent. Clearnet relays stay direct.
# CANARY_NOSTR_SOCKS_PROXY=127.0.0.1:9050
```

Configuration precedence: CLI arguments (`--network`, `--electrum-url`, `--bind-address`, `--data-dir`), then environment variables, then `.env` in `backend/`.

Transaction explorers: without a configured URL or port the default is `https://mempool.space`. Cloud mode always uses `mempool.space`.

## Network Defaults
- **Regtest**: tcp://127.0.0.1:50001 (Docker development environment)
- **Testnet**: ssl://electrum.blockstream.info:60002
- **Mainnet**: ssl://electrum.blockstream.info:50002
- **Sync intervals**: self-hosted uses `CANARY_SYNC_INTERVAL` per wallet (defaults 30s regtest, 60s testnet, 300s mainnet); cloud uses tier intervals (Personal: 10min mainnet, Team: 2min mainnet; 30s regtest Personal, 15s regtest Team)

## Testing

- Run with `cargo test -- --test-threads=1` (tests share process environment and SQLite files)
- **Unit tests**: inline `#[cfg(test)] mod tests` blocks, plus cross-module suites in `src/tests/`
- **Integration tests**: HTTP handler and database tests in `tests/`, for example
  - `tests/balance_alerts_system_tests.rs` - Balance alert CRUD operations, alert types, edge cases
  - `tests/stripe_integration_tests.rs` - Stripe webhook processing and billing flows
  - `tests/contact_duplicates_test.rs` - Contact management and duplicate prevention
- **System tests**: Docker regtest scenarios in `system_tests/`, run with `./run-system-tests.sh`; see [system_tests/README.md](system_tests/README.md)

```bash
# Run one integration test file
cargo test --test balance_alerts_system_tests -- --test-threads=1

# Run one system test with output
cargo test --test balance_alert_scenarios test_balance_alert_below_threshold -- --ignored --nocapture
```

## Important Development Notes

### Stripe Webhook Requirement
The Stripe CLI webhook forwarding is **required** for user registration to work properly:
- Users register → Stripe creates trial subscription → Webhook updates user status
- Without webhook forwarding, users remain in "pending" status

### Database Management
- **Network Isolation**: Each network has separate database directory
- **Reset Command**: `./scripts/dev.sh reset` clears all databases
- **Connection Pooling**: r2d2 SQLite pool with 16 max connections

### Subscription Limits
- **Proactive Enforcement**: Limits checked before form display, not after submission  
- **Tier-based Sync**: Individual wallet sync intervals based on user tier
- **Admin Bypass**: Admin users have unlimited access regardless of subscription
- **Contact Priority**: Oldest contacts remain active when limits exceeded

### Address Management & Deep Scanning
- **Custom Stop Gap**: Wallet creation accepts `stop_gap` values `auto`, 250, 500, 750 or 1000
- **Fast Wallet Creation**: POST responses in ~1.5s using prefix-based script type detection for immediate UX
- **Background Processing**: Async deep scanning after wallet creation with 'pending' → 'ready' state management
- **Script Type Detection**: Intelligent P2WPKH, P2SH, P2TR, P2PKH detection from XPUB prefixes
- **High Index Support**: `system_tests/high_index_scanning.rs` covers funds at index 249 with a 250-script stop gap
- **Dynamic Revelation**: BDK automatically reveals addresses to maintain 20 unused addresses
- **Stop Gap**: Always ensures 20 (`electrum::STOP_GAP`) consecutive unused addresses are revealed
- **No Index Limits**: Handles transactions at any address index (150, 200+)

### File Structure Conventions
- **Backend**: Modular architecture with `handlers/`, `extractors/`, and `models/` directories
- **Handlers**: Domain-specific handler modules (auth, wallet, billing, etc.)
- **Extractors**: Custom Axum extractors for authentication and authorization
- **Models**: Request/response DTOs and validators
- **Migrations**: SQL files in `migrations/`
- **Database**: `{CANARY_DATA_DIR}/{network}/metadata.sqlite` and `{CANARY_DATA_DIR}/{network}/wallets/{checksum}.sqlite`
