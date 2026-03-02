# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview
Canary is a Bitcoin wallet management service built in Rust that provides REST API endpoints for creating and managing Bitcoin wallets using BDK (Bitcoin Development Kit). Features include multipath descriptors, Electrum sync, transaction analysis, background sync, multi-language support (9 languages: English, Norwegian, Spanish, Portuguese, German, French, Japanese, Danish, Swedish) for both UI and notifications via configurable providers, optional email/password authentication with email verification, Stripe subscription billing, and balance alert notifications.

## Architecture
Built with a **non-blocking web architecture** that separates wallet sync operations from web serving to ensure fast API responses. Features dual-state design with `AppServices` for immediate metadata access and `WalletManager` for background sync operations. Implements plugin-based notification system that allows extensible notification providers. Supports both ntfy.sh push notifications and Twilio SMS, configurable via environment variables. All providers share message formatting and notification logging functionality. Features optional JWT-based authentication with email/password and email verification for multi-user support. SMS verification via Twilio Verify is still used for contact verification when adding SMS contacts. Uses polling-based frontend updates rather than server-sent events. **Tier-based serial sync** processes wallets by subscription tier with automatic cleanup of deleted wallets during sync cycles.

**Performance Architecture:**
- **Fast Web Responses**: API endpoints respond in <1ms by avoiding wallet mutex locks
- **Background Sync**: Heavy wallet operations run in separate async tasks without blocking web serving
- **Dual State Design**: `AppServices` for immediate metadata access, `WalletManager` for sync operations
- **Non-blocking Startup**: Server starts immediately, wallet loading deferred to background sync task

## Development Commands

### Backend (Rust)
```bash
# REQUIRED: Start Stripe CLI webhook forwarding first
stripe listen --forward-to localhost:3000/api/stripe/webhook

# Run backend (regtest default) - in separate terminal
cd backend && cargo run

# Other networks
cd backend && cargo run -- --network testnet
cd backend && cargo run -- --network mainnet

# Alternative: Override environment directly (useful for deep scan testing)
cd backend && CANARY_NETWORK=mainnet CANARY_ELECTRUM_URL=ssl://electrum.blockstream.info:50002 cargo run
cd backend && CANARY_NETWORK=testnet CANARY_ELECTRUM_URL=ssl://electrum.blockstream.info:60002 cargo run
cd backend && CANARY_NETWORK=regtest CANARY_ELECTRUM_URL=tcp://127.0.0.1:50001 cargo run

# Build, test, lint
cd backend && cargo build
cd backend && cargo test -- --test-threads=1  # Unit + integration tests
cd backend && cargo test --test balance_alert_scenarios -- --ignored --nocapture  # System tests (requires Docker)
cd backend && cargo test --test mined_directly_scenarios -- --ignored --nocapture  # System tests
cd backend && cargo fmt && cargo clippy
```

### Frontend (Next.js)
```bash
cd frontend && pnpm dev        # http://localhost:3001
cd frontend && pnpm build
cd frontend && pnpm lint
cd frontend && pnpm test       # Run all tests
cd frontend && pnpm test:watch # Run tests in watch mode
```

### Local Development Scripts
```bash
cd scripts && docker-compose up -d
cd scripts && ./dev.sh reset
cd scripts && ./dev.sh run-tests <wallet_address>
```

## Project Structure
```
canary/
├── backend/          # Rust service with BDK wallet management
│   ├── src/
│   │   ├── main.rs           # Application entry point
│   │   ├── api.rs            # Router configuration, AppServices, AppState
│   │   ├── extractors/       # Custom Axum extractors
│   │   │   ├── mod.rs
│   │   │   └── auth.rs       # AuthenticatedUser extractor
│   │   ├── handlers/         # Domain-specific API handlers
│   │   │   ├── mod.rs
│   │   │   ├── auth.rs       # Authentication (register, login, etc.)
│   │   │   ├── wallet.rs     # Wallet CRUD operations
│   │   │   ├── contact.rs    # Contact management
│   │   │   ├── billing.rs    # Stripe integration
│   │   │   ├── config.rs     # Public app config (mempool URL)
│   │   │   └── ...           # Other domain handlers
│   │   ├── models/           # Request/response DTOs
│   │   │   ├── mod.rs
│   │   │   ├── requests.rs   # API request types
│   │   │   ├── responses.rs  # API response types
│   │   │   └── validators.rs # Input validation
│   │   ├── wallet.rs         # BDK wallet operations
│   │   ├── metadata/         # SQLite database layer (modular)
│   │   │   ├── mod.rs        # Module re-exports
│   │   │   ├── db.rs         # Database operations
│   │   │   ├── pool.rs       # Connection pooling
│   │   │   ├── types.rs      # Database types
│   │   │   ├── user.rs       # User operations (auth, rate limiting)
│   │   │   ├── wallet.rs     # Wallet metadata operations
│   │   │   ├── contact.rs    # Contact operations
│   │   │   ├── transaction.rs # Transaction operations
│   │   │   └── alert.rs      # Balance alert operations
│   │   ├── sync.rs           # Background sync operations
│   │   ├── email_queue.rs    # Background email queue with batching
│   │   ├── email_service.rs  # Email service abstraction
│   │   ├── notification_failure_tracker.rs # Provider failure tracking
│   │   ├── admin_notifications.rs # Admin alert system
│   │   ├── exchange_rates.rs # Exchange rate functionality
│   │   ├── message_formatter.rs # Notification message formatting
│   │   ├── electrum.rs       # Electrum client management
│   │   ├── xpub_converter.rs # XPUB format conversion
│   │   └── ...               # Notification providers, config, etc.
│   ├── database/             # SQLite databases (gitignored)
│   │   ├── cloud/            # cloud mode databases
│   │   └── self-hosted/      # self-hosted mode databases
│   ├── migrations/           # 22 database schema migrations (001-022)
│   ├── tests/                # Integration tests
│   ├── system_tests/         # End-to-end Docker-based tests
│   └── tasks/                # Development tasks and documentation
├── frontend/        # Next.js app with React components
│   ├── messages/       # i18n translation files (en-US.json, nb.json, es-419.json, pt-BR.json, de-DE.json, fr-FR.json, ja.json, da.json, sv.json)
│   ├── src/
│   │   ├── app/        # Next.js App Router (pages, layouts, API routes)
│   │   ├── components/ # UI components (plan-comparison.tsx, plans-modal.tsx, contact-modal.tsx)
│   │   ├── lib/        # Shared utilities (pricing-data.ts, utils.ts, api.ts)
│   │   ├── contexts/   # React contexts (auth-context.tsx, wallets-context.tsx)
│   │   ├── hooks/      # Custom React hooks (useWalletDetail.ts, usePricing.ts, useRelativeTime.ts)
│   │   ├── i18n/       # Internationalization config (config.ts, request.ts)
│   │   └── types/      # TypeScript type definitions
├── scripts/        # Development scripts and Docker setup
└── CLAUDE.md       # This file
```

## Key Dependencies
- **Backend**: BDK wallet v2, SQLite with r2d2 pooling, Axum web framework, ntfy.sh + Twilio + Resend email notifications
- **Frontend**: Next.js 16, React 19, Tailwind CSS 4, Radix UI with shadcn/ui components, next-intl for i18n, date-fns for localized dates, JWT authentication support

## API Endpoints

### Authentication (Optional)
- `POST /api/auth/register` - Register with email/password (sends verification email)
- `POST /api/auth/login` - Login with email/password
- `GET /api/auth/verify-email/{token}` - Verify email address
- `POST /api/auth/forgot-password` - Send password reset email
- `POST /api/auth/reset-password/{token}` - Reset password with token
- `POST /api/auth/logout` - Logout current user
- `GET /api/auth/me` - Get current user info

### Wallet Management
- `POST /api/wallets` - Create wallet (name + descriptor)
- `GET /api/wallets` - List all wallets with timestamp (returns `WalletsListResponse`)
- `GET /api/wallets/{checksum}` - Get wallet metadata by checksum
- `GET /api/wallets/{checksum}/detail` - Get wallet with transaction events (returns `WalletDetailResponse`)
- `PUT /api/wallets/{checksum}` - Update wallet name
- `DELETE /api/wallets/{checksum}` - Delete wallet

### Contact Management (Wallet-specific)
- `POST /api/wallets/{checksum}/contacts` - Add contact with automatic provider detection (name + contact_address + language)
- `GET /api/wallets/{checksum}/contacts` - List contacts with notification methods  
- `PUT /api/wallets/{checksum}/contacts/{contact_id}` - Update contact with atomic transaction support
- `DELETE /api/wallets/{wallet_checksum}/contacts/{contact_id}` - Remove contact and all notification methods

### Contact Verification System
- `POST /api/wallets/{checksum}/contacts/send-verification` - Send SMS/email verification codes
- `POST /api/wallets/{checksum}/contacts/verify` - Verify SMS/email codes for contact creation/updates
- **Security**: All SMS and email contacts require OTP verification within 30-minute window
- **Atomic Updates**: PUT operations use database transactions to prevent data loss
- **Verification Persistence**: Completed verifications are marked with timestamps, not deleted

### Application Config
- `GET /api/config` - Get public application configuration (mempool URL/port for self-hosted mode)

### Blockchain Data
- `GET /api/block-headers/current` - Get current block header from database

### Balance Alerts
- `POST /api/wallets/{checksum}/balance-alerts` - Create balance alert (threshold_sats + alert_type: above/below/equals)
- `GET /api/wallets/{checksum}/balance-alerts` - List balance alerts for wallet
- `PUT /api/wallets/{checksum}/balance-alerts/{alert_id}` - Update balance alert configuration
- `POST /api/wallets/{checksum}/balance-alerts/{alert_id}/reactivate` - Manually reactivate a fired alert
- `DELETE /api/wallets/{checksum}/balance-alerts/{alert_id}` - Delete balance alert
- **Auto-disable**: Alerts automatically deactivate after firing, requiring manual reactivation
- **Notification Integration**: Balance alerts use existing contact notification system
- **Audit Trail**: Complete history of triggered alerts in `balance_alert_notifications` table

### Billing & Subscription Management  
- `POST /api/stripe/checkout` - Create Stripe Checkout session for plan upgrades
- `POST /api/stripe/portal` - Create Stripe Customer Portal session for subscription management
- `GET /api/billing/status` - Get current subscription status and billing information
- `POST /api/stripe/webhook` - Process Stripe webhook events (subscription lifecycle)
- `GET /api/billing/pricing` - Get current subscription pricing information
- `GET /api/billing/session/{session_id}` - Get Stripe Checkout session details

### Notification System
- `GET /api/providers` - List available and configured notification providers
- **Multiple Notification Methods**: Contacts can receive notifications through multiple providers simultaneously
- **Auto-detection**: Phone numbers (starting with +) → SMS, topics → ntfy, email addresses → email
- **Normalized Database**: Separate tables for contacts and notification methods for extensibility
- Generic notification logs with delivery status tracking for all providers

## Network Configuration
Supports regtest (default), testnet, mainnet with configurable Electrum servers.

**Configuration methods:**
- CLI: `cargo run -- --network mainnet --electrum-url ssl://electrum.blockstream.info:50002`
- Environment: `CANARY_NETWORK=mainnet`, `CANARY_ELECTRUM_URL=...`
- `.env` file in backend directory

**Configuration templates:**
- **self-hosted mode**: Copy `backend/.env.example.self-hosted` → `backend/.env` and `frontend/.env.example.self-hosted` → `frontend/.env.local`
- **cloud mode**: Copy `backend/.env.example.cloud` → `backend/.env` and `frontend/.env.example.cloud` → `frontend/.env.local`

**Defaults:**
- Regtest: tcp://127.0.0.1:50001
- Testnet: ssl://electrum.blockstream.info:60002  
- Mainnet: ssl://electrum.blockstream.info:50002
- Sync intervals: Tier-based (Personal: 10min mainnet, Team: 2min mainnet; 30s regtest Personal, 15s regtest Team)
- Frontend polling: 60 seconds (configurable via NEXT_PUBLIC_SYNC_INTERVAL)

**Mempool Explorer (self-hosted only):**
- `CANARY_MEMPOOL_URL` - Full URL to custom Mempool instance (e.g., `http://umbrel.local:3006`)
- `CANARY_MEMPOOL_PORT` - Auto-detected by Umbrel `exports.sh` when Mempool app is installed
- Default: `https://mempool.space` when neither is set
- Cloud mode always uses `mempool.space` regardless of configuration

## Key Features

### Authentication & User Management
- **Optional Authentication**: Email/password authentication with email verification
- **Multi-user Support**: JWT-based sessions with user isolation when auth is enabled
- **Development Mode**: Pre-configured test accounts (delivered+admin@resend.dev, delivered+alice@resend.dev, delivered+bob@resend.dev) with password `password123`
- **Clean Registration Flow**: Dedicated success page (/sign-up/success) with clear email verification instructions

### Subscription & Billing
- **Two-tier System**: Personal ($9/month) and Team ($29/month) with capacity-based limits
- **Stripe Integration**: Native trial subscriptions, webhook-driven status updates, Customer Portal for management
- **Proactive Limit Enforcement**: Smart upgrade modals prevent users from hitting limits after form completion
- **Trial Management**: 30-day Team tier trial with automatic Stripe webhook transitions
- **Never Downgrade Policy**: Users keep tier after expiration but lose wallet syncing
- See `backend/CLAUDE.md` for Stripe integration details, webhook events, and billing configuration

### Wallet Management
- **Deep Scanning & Script Detection**: Detects funds at high address indexes (200+) with fast API responses
- **Async Wallet Creation**: Fast POST responses (~1.5s) with background deep scanning and skeleton UI states
- **Dynamic Address Revelation**: Automatically reveals addresses to maintain 20 unused stop gap
- **Script Type Detection**: Intelligent detection of P2WPKH, P2SH, P2TR, P2PKH from XPUBs
- **Tier-based Sync**: Individual wallet sync intervals based on subscription tier (Personal: 10min mainnet, Team: 2min mainnet; 30s regtest Personal, 15s regtest Team)
- **Wallet Deletion**: Soft delete with automatic cleanup during sync cycles
- **Transaction Analysis**: RBF/CPFP detection, accurate timestamps with blockchain confirmation tracking
- **Network Isolation**: Separate databases per Bitcoin network
- See `backend/CLAUDE.md` for deep scanning implementation and address revelation details

### Notifications & Alerts
- **Balance Alerts**: User-configurable alerts for above/below/equals threshold monitoring with auto-disable and manual reactivation
- **Plugin-based Notifications**: Extensible provider system supporting ntfy.sh, Twilio SMS, and Resend email
- **Multiple Notification Methods**: Each contact can have multiple notification methods (SMS + email + ntfy)
- **Auto-detection**: Automatically detects provider type from contact address format
- **Multi-language Support**: 9 languages (English, Norwegian, Spanish, Portuguese, German, French, Japanese, Danish, Swedish) with proper Bitcoin amount formatting
- **Notification Tracking**: Delivery status tracking with ✅/❌ UI indicators for all providers
- **Secure Verification System**: OTP verification for SMS/email with 30-minute validity windows
- **Admin Notifications**: Infrastructure alerts for trial expirations, non-syncing wallets, and system issues

### Security Features
- **Login Rate Limiting**: 5 failed attempts trigger 15-minute account lockout; email-based tracking in `login_attempts` table
- **OTP Verification Rate Limiting**: 5 failed OTP attempts trigger 30-minute block per notification target
- **Email Enumeration Prevention**: Registration endpoint returns consistent responses regardless of whether email exists
- **HTML Escaping**: `html_escape()` applied consistently across all email templates to prevent XSS
- **Machine-readable Error Codes**: `error_code` field in API error responses for frontend i18n translation (45+ error codes)

### Technical Features
- **Non-blocking Web Architecture**: Fast API responses (<1ms) avoiding wallet mutex locks
- **Dual State Design**: `AppServices` for immediate metadata access, `WalletManager` for sync operations
- **Performance**: Async SQLite with r2d2 connection pooling
- **Normalized Database**: 22 migration files with clean schema design supporting extensibility
- **Environment Configuration**: Provider selection and network config via .env variables
- **Atomic Updates**: Database transactions prevent data loss during modifications
- **Email Queue**: Background email queue with batching (up to 100 per batch), retry logic, and rate limiting
- **Notification Failure Tracking**: Consecutive failure monitoring for SMS/email providers with throttled admin alerts

### Internationalization (i18n)
- **9 Supported Languages**: English (US), Norwegian (Bokmål), Spanish (Latin America), Portuguese (Brazil), German, French, Japanese, Danish, Swedish
- **Frontend**: next-intl library with JSON translation files in `frontend/messages/{locale}.json`
- **Backend**: Notification messages translated via rust-i18n in `backend/locales/{locale}.yml`
- **Language Selection**: User preference stored in database and cookie, configurable in Settings page
- **Browser Detection**: Auto-detects browser locale on first visit, falls back to English (US)
- **Localized Dates**: date-fns locales for relative time formatting (e.g., "hace 5 horas" in Spanish)
- **Translation Pattern**: Components use `useTranslations('namespace')` hook from next-intl

**Locale Code Choices:**
| Code | Language | Rationale |
|------|----------|-----------|
| `en-US` | English (US) | American English spelling and date formats |
| `nb` | Norwegian (Bokmål) | Bokmål is the most common written standard (~85% of Norwegians) |
| `es-419` | Spanish (Latin America) | UN M.49 code for Latin America; uses "ustedes" form, Latin American vocabulary |
| `pt-BR` | Portuguese (Brazil) | Brazilian Portuguese; different spelling and vocabulary from European Portuguese |
| `de-DE` | German | Standard German (Germany) |
| `fr-FR` | French | Standard French (France) |
| `ja` | Japanese | No regional variants needed |
| `da` | Danish | No regional variants needed |
| `sv` | Swedish | No regional variants needed |

**Frontend Translation Structure:**
```
frontend/messages/
├── en-US.json  # English US (source/default)
├── nb.json     # Norwegian Bokmål
├── es-419.json # Spanish (Latin America)
├── pt-BR.json  # Portuguese (Brazil)
├── de-DE.json  # German
├── fr-FR.json  # French
├── ja.json     # Japanese
├── da.json     # Danish
└── sv.json     # Swedish
```

**Adding New Translations:**
1. Add keys to `frontend/messages/en-US.json` first
2. Copy to all other locale files with translated values
3. Use in components: `const t = useTranslations('namespace'); t('key')`
4. For variables: `t('greeting', { name: 'John' })` with `"greeting": "Hello, {name}"`

## Subscription Tiers & Limits

### Tier Structure
- **Personal ($9/month)**: For individual Bitcoin holders - 1 wallet, 1 contact per wallet, 10-minute sync, all features enabled
- **Team ($29/month)**: For Uncle Jims & family guardians - 5 wallets, 5 contacts per wallet, 2-minute sync, all features enabled

**Note**: All features (email/SMS/push notifications, transaction analysis) are available on both tiers. The difference is only in capacity limits and sync frequency.

### Limit Enforcement
- **Proactive Checking**: Limits checked before form display (not after submission)
- **Smart Upgrade Modals**: Professional plan comparison with monthly pricing and yearly savings display
- **Tier-based Sync**: Individual wallet sync intervals based on subscription tier
- **Contact Priority**: When subscription limits are exceeded, oldest contacts remain active (first-in-first-out principle)
- **Admin Bypass**: Admin users have unlimited access regardless of subscription tier

### Frontend Components
- **Shared Pricing Data**: Single source of truth in `/src/lib/pricing-data.ts`
- **Reusable Components**: `PlanComparison` and `PlansModal` components
- **Upgrade Modal**: Wide modal (85vw) with comprehensive feature comparison
- **Current Plan Highlighting**: Blue highlighting with "CURRENT PLAN" badge

### Database Schema
SQLite with 22 migrations (`backend/migrations/001-022`). Key tables: `users`, `wallets`, `contacts`, `contact_notification_methods`, `notification_logs`, `transaction_events`, `balance_alerts`, `balance_alert_notifications`. See `backend/CLAUDE.md` for full schema details.

## Development Workflow
- **Testing**: `./scripts/dev.sh` provides complete Bitcoin regtest environment
- **Docker Environment**: Complete Bitcoin Core + Fulcrum Electrum server for local development

## Testing
- **Frontend**: `pnpm test` — see `frontend/CLAUDE.md` for detailed test suite documentation
- **Backend unit/integration**: `cargo test -- --test-threads=1` (SQLite requires single-threaded)
- **Backend system tests**: Docker-based end-to-end tests — see `backend/system_tests/README.md`

## Code Standards
- No commented-out code (use git history)  
- Clean, maintainable codebase
- Plugin architecture for extensibility
- Generic database design supporting multiple notification providers
- Proper error handling with user-friendly messages
- Type-safe API contracts between frontend and backend

## Committing code to git

Always build both the frontend and backend and run and verify all tests before committing. In case of errors, they need to be fixed.

Never amend existing commits. Always create new commits for follow-up changes, review feedback fixes, etc. Only amend if explicitly asked to.

---
*Last updated: February 2026*