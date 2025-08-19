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

### Docker Development Environment
```bash
cd regtest-env && docker-compose up -d
./docker-utils.sh reset        # Reset all databases
./docker-utils.sh mine 6       # Mine blocks
./docker-utils.sh fund <addr> 1.0  # Fund address
```

## Configuration

### Environment Setup
All configuration is explicit via environment variables - no hardcoded defaults.

**Quick Start:**
- **FOSS mode**: `cp .env.example.foss .env`
- **SAAS mode**: `cp .env.example.saas .env`

**Configuration Files:**
- `.env.example.foss` - Self-hosted single-user configuration
- `.env.example.saas` - Hosted service with billing configuration  

The backend will fail fast with clear error messages if required variables are missing.

## Code Architecture

### Backend Structure
- **Core Entry Point**: `src/main.rs` - Application initialization with async task spawning
- **API Routes**: `src/api.rs` - RESTful endpoints with OpenAPI documentation
- **Wallet Management**: `src/wallet.rs` - BDK wallet operations, sync logic, address revelation
- **Database Layer**: `src/metadata.rs` - SQLite with r2d2 connection pooling
- **Authentication**: `src/auth.rs` - JWT session management, email verification
- **Billing**: `src/stripe_billing.rs` + `src/stripe_client_service.rs` - Subscription management
- **Notifications**: Plugin-based system with provider modules (`src/ntfy_provider.rs`, etc.)

**Key Patterns:**
- Plugin-based notification architecture with trait objects
- Arc<Mutex<T>> for shared state management across async tasks
- Tier-based wallet sync intervals (Personal: 10min, Team: 1min)
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
- **Single Migration**: `migrations/001_initial_schema.sql` with complete normalized schema
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
CANARY_MODE=saas
JWT_SECRET=your_secure_jwt_secret

# Stripe billing
STRIPE_SECRET_KEY=sk_test_...
STRIPE_WEBHOOK_SECRET=whsec_...

# Notification providers
# Twilio will be auto-enabled in SAAS mode if configured
TWILIO_ACCOUNT_SID=your_account_sid
TWILIO_AUTH_TOKEN=your_auth_token
TWILIO_MESSAGING_SERVICE_SID=your_service_sid
TWILIO_ACCOUNT_SID=...
TWILIO_AUTH_TOKEN=...
TWILIO_MESSAGING_SERVICE_SID=...

# Email provider (Resend)
RESEND_API_KEY=re_...
RESEND_FROM_EMAIL=notifications@canarybitcoin.com
```

### Frontend Environment Variables
```bash
NEXT_PUBLIC_STRIPE_PUBLISHABLE_KEY=pk_test_...
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
- **No Migrations**: Single migration file `001_initial_schema.sql`
- **Network Isolation**: Each network has separate database directory
- **Reset Command**: `./regtest-env/docker-utils.sh reset` clears all databases
- **Connection Pooling**: r2d2 SQLite pool with 10 max connections

### Subscription Limits
- **Proactive Enforcement**: Limits checked before form display, not after submission  
- **Tier-based Sync**: Individual wallet sync intervals based on user tier
- **Admin Bypass**: Admin users have unlimited access regardless of subscription
- **Contact Priority**: Oldest contacts remain active when limits exceeded

### Address Management
- **Dynamic Revelation**: BDK automatically reveals addresses to maintain 20 unused addresses
- **Stop Gap**: Always ensures 20 consecutive unused addresses are revealed
- **No Index Limits**: Handles transactions at any address index (150, 200+)

### File Structure Conventions
- **Backend**: All source in `src/` with single-file modules  
- **Frontend**: App router structure with co-located component tests
- **Migrations**: Single migration file approach (no backwards compatibility needed)
- **Database**: Network-specific directories under `database/{network}/`