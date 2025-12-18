# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Canary is a Bitcoin wallet management service built in Rust that provides REST API endpoints for Bitcoin wallet operations using BDK (Bitcoin Development Kit). The project consists of a Rust backend service, Next.js frontend, and Docker-based development environment.

**Key Architecture Components:**
- **Backend**: Rust service with Axum web framework, BDK wallet management, SQLite databases per network
- **Frontend**: Next.js 15 with React 19, TypeScript, Tailwind CSS 4, shadcn/ui components
- **Development Environment**: Docker-based Bitcoin regtest + Fulcrum Electrum server setup
- **Authentication**: Optional JWT-based multi-user system with email verification
- **Billing**: Stripe integration with subscription management and webhook processing
- **Notifications**: Plugin-based system supporting ntfy.sh, Twilio SMS, and Resend email

## Development Commands

### Backend (Rust)
```bash
# REQUIRED FIRST: Start Stripe CLI webhook forwarding for billing
stripe listen --forward-to localhost:3000/api/stripe/webhook

# Run backend (regtest is default network)
cd backend && cargo run

# Other networks
cargo run -- --network testnet
cargo run -- --network mainnet

# Build and test
cargo build
cargo test -- --test-threads=1
cargo fmt && cargo clippy
```

### Frontend (Next.js)
```bash
cd frontend && npm run dev     # http://localhost:3001
npm run build
npm run lint
npm test
```

### Local Development Scripts
```bash
cd scripts && docker-compose up -d
./dev.sh reset        # Reset all databases
./dev.sh mine 6       # Mine blocks
./dev.sh fund <addr> 1.0  # Fund address
```

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
- **Core Entry Point**: `src/main.rs` - Application initialization with async task spawning
- **API Routes**: `src/api.rs` - RESTful endpoints with OpenAPI documentation
- **Wallet Management**: `src/wallet.rs` - BDK wallet operations, sync logic, address revelation
- **Database Layer**: `src/metadata.rs` - SQLite with r2d2 connection pooling
- **Authentication**: `src/auth.rs` - JWT session management, email verification
- **Billing**: `src/stripe_billing.rs` + `src/stripe_client_service.rs` - Subscription management
- **Notifications**: Plugin-based system with provider modules (`src/ntfy_provider.rs`, `src/twilio_provider.rs`, `src/email_provider.rs`, etc.)

**Key Patterns:**
- **Non-blocking Web Architecture**: `AppServices` struct provides fast metadata access without wallet mutex locks
- **Dual State Management**: Web endpoints use `AppServices`, sync operations use `WalletManager` with Arc<Mutex<T>>
- **Background Sync Tasks**: Heavy wallet operations run in separate async tasks to avoid blocking web serving
- **Performance Monitoring**: Comprehensive timing logs for mutex wait times and sync operations
- Plugin-based notification architecture with trait objects
- Tier-based wallet sync intervals (Personal: 10min mainnet, Team: 2min mainnet; 30s regtest Personal, 15s regtest Team)
- Automatic wallet cleanup during sync cycles (soft delete → hard delete)
- Network-specific SQLite databases in `database/{network}/`

### Frontend Structure
- **App Router**: Next.js 15 app directory structure with TypeScript
- **Authentication**: `src/contexts/auth-context.tsx` - JWT token management, billing status
- **State Management**: React Context + custom hooks pattern
- **UI Components**: `src/components/ui/` - shadcn/ui design system
- **API Layer**: `src/lib/api.ts` - Type-safe backend communication
- **Subscription UI**: Shared pricing components for upgrade flows

**Key Patterns:**
- Server and client component separation
- Context providers for global state (auth, wallets)
- Custom hooks for data fetching with polling
- Shared pricing data for consistent billing UI

### Database Schema
- **Eight Migration Files**: migrations/001_initial_schema.sql through migrations/008_transaction_based_refactor.sql with complete normalized schema and transaction-based architecture
- **UUID Primary Keys**: Used for security-critical tables (users, transaction_events)
- **Subscription Management**: Built-in user tiers, Stripe integration fields
- **Network Isolation**: Separate databases per Bitcoin network (regtest/testnet/mainnet)

### Notification System Architecture
- **Provider Registration**: Runtime registration based on environment variables
- **Trait-based**: Common `NotificationProvider` trait for extensibility  
- **Multi-method Support**: Contacts can have multiple notification methods
- **Auto-detection**: Phone numbers → SMS, email addresses → email, topics → ntfy
- **Delivery Tracking**: Database logging with success/failure status

## Environment Configuration

### Required for Development
```bash
# Stripe CLI is REQUIRED for local development
stripe listen --forward-to localhost:3000/api/stripe/webhook
```

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
```

### Frontend Environment Variables
```bash
NEXT_PUBLIC_SYNC_INTERVAL=60000
```

## Network Defaults
- **Regtest**: tcp://127.0.0.1:50001 (Docker development environment)
- **Testnet**: ssl://electrum.blockstream.info:60002
- **Mainnet**: ssl://electrum.blockstream.info:50002

## Testing

### Backend
- Run with `cargo test -- --test-threads=1` (SQLite requires single-threaded)
- Integration tests in `tests/` directory
- Unit tests in `src/tests/` modules

#### Test Categories
- **Unit Tests**: Individual component testing in `src/tests/` modules
- **Integration Tests**: Full database and API testing in `tests/` directory
  - `tests/balance_alerts_system_tests.rs` - Balance alert CRUD operations, alert types, edge cases, performance
  - `tests/stripe_integration_tests.rs` - Stripe webhook processing and billing flows
  - `tests/contact_duplicates_test.rs` - Contact management and duplicate prevention
- **System Tests**: End-to-end Docker-based testing in `system_tests/` directory (see `system_tests/README.md`)
  - `system_tests/balance_alert_scenarios.rs` - Real Bitcoin transaction balance alert testing with Docker Bitcoin Core
  - `system_tests/mined_directly_scenarios.rs` - Transaction detection when mined before sync
  - `system_tests/two_stage_send_scenarios.rs` - Unconfirmed → confirmed transaction flow testing

#### Running Specific Tests
```bash
# Run all integration tests
cargo test -- --test-threads=1

# Run specific test file
cargo test --test balance_alerts_system_tests -- --test-threads=1

# Run system tests (requires Docker)
cargo test --test mined_directly_scenarios -- --ignored
cargo test --test balance_alert_scenarios -- --ignored

# Run with debug output
cargo test balance_alerts_system_tests -- --test-threads=1 --nocapture
cargo test test_balance_alert_below_threshold --test balance_alert_scenarios -- --ignored --nocapture
```

### Frontend  
- Jest with React Testing Library: `npm test`
- Component tests focus on subscription limits, modals, contact management
- Mock API responses for integration testing

## Important Development Notes

### Stripe Webhook Requirement
The Stripe CLI webhook forwarding is **required** for user registration to work properly:
- Users register → Stripe creates trial subscription → Webhook updates user status
- Without webhook forwarding, users remain in "pending" status

### Database Management
- **Eight Migration Files**: Complete schema through migration 008 with transaction-based architecture
- **Network Isolation**: Each network has separate database directory
- **Reset Command**: `./scripts/dev.sh reset` clears all databases
- **Connection Pooling**: r2d2 SQLite pool with 10 max connections

### Subscription Limits
- **Proactive Enforcement**: Limits checked before form display, not after submission  
- **Tier-based Sync**: Individual wallet sync intervals based on user tier
- **Admin Bypass**: Admin users have unlimited access regardless of subscription
- **Contact Priority**: Oldest contacts remain active when limits exceeded

### Address Management & Deep Scanning
- **Deep Scanning System**: Progressive address revelation up to 500 addresses in batches (100, 200, 300, 400, 500)
- **Fast Wallet Creation**: POST responses in ~1.5s using prefix-based script type detection for immediate UX
- **Background Processing**: Async deep scanning after wallet creation with 'pending' → 'ready' state management
- **Script Type Detection**: Intelligent P2WPKH, P2SH, P2TR, P2PKH detection from XPUB prefixes
- **High Index Support**: Successfully detects funds at high address indexes (tested to 250+)
- **Dynamic Revelation**: BDK automatically reveals addresses to maintain 20 unused addresses
- **Stop Gap**: Always ensures 20 consecutive unused addresses are revealed
- **No Index Limits**: Handles transactions at any address index (150, 200+)

### File Structure Conventions
- **Backend**: All source in `src/` with single-file modules  
- **Frontend**: App router structure with co-located component tests
- **Migrations**: Eight migration files for schema evolution
- **Database**: Network-specific directories under `database/{network}/`