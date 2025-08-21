# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview
Canary is a Bitcoin wallet management service built in Rust that provides REST API endpoints for creating and managing Bitcoin wallets using BDK (Bitcoin Development Kit). Features include multipath descriptors, Electrum sync, transaction analysis, background sync, multi-language notifications (Norwegian and English) via configurable providers, and optional email/password authentication with email verification.

## Architecture
Built with a plugin-based notification system that allows extensible notification providers. Supports both ntfy.sh push notifications and Twilio SMS, configurable via environment variables. All providers share message formatting and notification logging functionality. Features optional JWT-based authentication with email/password and email verification for multi-user support. SMS verification via Twilio Verify is still used for contact verification when adding SMS contacts. Uses polling-based frontend updates rather than server-sent events.

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
cd backend && cargo test -- --test-threads=1
cd backend && cargo fmt && cargo clippy
```

### Frontend (Next.js)
```bash
cd frontend && npm run dev     # http://localhost:3001
cd frontend && npm run build
cd frontend && npm run lint
cd frontend && npm test        # Run all tests
cd frontend && npm run test:watch  # Run tests in watch mode
```

### Docker (Regtest Environment)
```bash
cd regtest-env && docker-compose up -d
cd regtest-env && ./docker-utils.sh reset
cd regtest-env && ./docker-utils.sh run-tests <wallet_address>
```

## Project Structure
```
canary/
├── backend/          # Rust service with BDK wallet management
│   ├── src/         # All source code (api.rs, main.rs, wallet management, notifications, subscription.rs)
│   ├── database/    # Network-specific SQLite databases (database/{network}/)
│   └── migrations/  # Database schema migrations (001_initial_schema.sql)
├── frontend/        # Next.js app with React components
│   ├── src/
│   │   ├── components/  # UI components (plan-comparison.tsx, plans-modal.tsx)
│   │   ├── lib/        # Shared utilities (pricing-data.ts, utils.ts)
│   │   └── contexts/   # React contexts (auth-context.tsx, wallets-context.tsx)
├── regtest-env/    # Docker Bitcoin + Fulcrum setup
└── CLAUDE.md       # This file
```

## Key Dependencies
- **Backend**: BDK wallet v2, SQLite with r2d2 pooling, Axum web framework, ntfy.sh + Twilio + Resend email notifications
- **Frontend**: Next.js 15, React 19, Tailwind CSS 4, shadcn/ui components, JWT authentication support

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
- `DELETE /api/wallets/{wallet_checksum}/contacts/{contact_id}` - Remove contact and all notification methods

### Blockchain Data
- `GET /api/block-headers/current` - Get current block header from database

### Billing & Subscription Management  
- `POST /api/checkout/create-session` - Create Stripe Checkout session for plan upgrades
- `POST /api/billing/customer-portal` - Create Stripe Customer Portal session for subscription management
- `GET /api/billing/status` - Get current subscription status and billing information
- `POST /api/stripe/webhook` - Process Stripe webhook events (subscription lifecycle)

### Notification System
- `GET /api/providers` - List available and configured notification providers
- **Multiple Notification Methods**: Contacts can receive notifications through multiple providers simultaneously
- **Auto-detection**: Phone numbers (starting with +) → SMS, topics → ntfy, email addresses → email
- **Normalized Database**: Separate tables for contacts and notification methods for extensibility
- Generic notification logs with delivery status tracking for all providers
- `/swagger-ui` - API documentation

## Network Configuration
Supports regtest (default), testnet, mainnet with configurable Electrum servers.

**Configuration methods:**
- CLI: `cargo run -- --network mainnet --electrum-url ssl://electrum.blockstream.info:50002`
- Environment: `CANARY_NETWORK=mainnet`, `CANARY_ELECTRUM_URL=...`
- `.env` file in backend directory

**Configuration templates:**
- **FOSS mode**: Copy `backend/.env.example.foss` → `backend/.env` and `frontend/.env.example.foss` → `frontend/.env.local`
- **SAAS mode**: Copy `backend/.env.example.saas` → `backend/.env` and `frontend/.env.example.saas` → `frontend/.env.local`

**Defaults:**
- Regtest: tcp://127.0.0.1:50001
- Testnet: ssl://electrum.blockstream.info:60002  
- Mainnet: ssl://electrum.blockstream.info:50002
- Sync intervals: Tier-based (Personal: 10min, Team: 1min)
- Frontend polling: 60 seconds (configurable via NEXT_PUBLIC_SYNC_INTERVAL)

## Key Features
- **Optional Authentication**: Email/password authentication with email verification
- **Multi-user Support**: JWT-based sessions with user isolation when auth is enabled
- **Subscription Tiers**: Two-tier system (Personal, Team) with capacity-based limits and all features enabled for both tiers
- **Proactive Limit Enforcement**: Smart upgrade modals prevent users from hitting limits after form completion
- **Plugin-based Notifications**: Extensible provider system supporting ntfy.sh, Twilio SMS, and Resend email
- **Multiple Notification Methods**: Each contact can have multiple notification methods (SMS + email + ntfy + future: telegram + webhooks)
- **Multi-language Support**: Norwegian and English with proper Bitcoin amount formatting  
- **Auto-detection**: Automatically detects provider type from contact address format (phone numbers → SMS, email addresses → email, topics → ntfy)
- **Normalized Database**: Clean separation of contacts and notification methods for future extensibility
- **Notification Tracking**: Delivery status tracking with ✅/❌ UI indicators for all providers
- **Environment Configuration**: Provider selection via .env variables, no database config needed
- **Performance**: Async SQLite with r2d2 connection pooling
- **Tier-based Sync**: Individual wallet sync intervals based on user's subscription tier (Personal: 10min, Team: 1min)
- **Transaction Analysis**: RBF/CPFP detection, accurate timestamps
- **Network Isolation**: Separate databases per Bitcoin network
- **Deep Scanning & Script Detection**: Advanced wallet analysis that detects funds at high address indexes (200+) with fast API responses
- **Dynamic Address Revelation**: Automatically reveals addresses to maintain stop gap, ensuring transactions at any index are detected
- **Script Type Detection**: Intelligent detection of P2WPKH, P2SH, P2TR, P2PKH from XPUBs with defaults for fresh wallets
- **Async Wallet Creation**: Fast POST responses (~1.5s) with background deep scanning and skeleton UI states
- **User Onboarding**: Guided wallet creation with BIP39 mnemonic generation
- **Professional UX**: Comprehensive plan comparison modals showing monthly prices with yearly savings
- **Development Mode**: Quick login options for testing with pre-configured email accounts
- **Clean Registration Flow**: Dedicated success page (/sign-up/success) after registration with clear email verification instructions

## Subscription Tiers & Limits

### Tier Structure
- **Personal ($9/month)**: For individual Bitcoin holders - 1 wallet, 1 contact per wallet, 10-minute sync, all features enabled
- **Team ($29/month)**: For Uncle Jims & family guardians - 5 wallets, 5 contacts per wallet, 1-minute sync, all features enabled

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
```sql
-- Users table includes subscription_tier and admin flag
CREATE TABLE users (
    id TEXT PRIMARY KEY,
    subscription_tier TEXT NOT NULL DEFAULT 'team' CHECK (subscription_tier IN ('personal', 'team')),
    is_admin BOOLEAN NOT NULL DEFAULT 0
);

-- Wallets table includes sync management fields
CREATE TABLE wallets (
    last_synced_at DATETIME,
    sync_status TEXT DEFAULT 'pending' CHECK (sync_status IN ('pending', 'ready')),
    user_id TEXT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users (id)
);

-- Contacts table includes created_at for priority ordering
CREATE TABLE contacts (
    id TEXT PRIMARY KEY,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    is_active BOOLEAN NOT NULL DEFAULT 1
);
```

## Notification Setup

### ntfy.sh (Default, always enabled)
1. Add contacts with ntfy topics: `POST /api/wallets/{checksum}/contacts` (name + language + topic)
2. Auto-generated topics: `contactname-language-checksum` (e.g., `john-en-8nt3y08q`)
3. Subscribe to topics at https://ntfy.sh/your-topic
4. Automatic push notifications for all transactions

### Twilio SMS (Optional)
1. Set environment variables in `.env`:
   ```
   CANARY_ENABLE_TWILIO=true
   TWILIO_ACCOUNT_SID=your_account_sid
   TWILIO_AUTH_TOKEN=your_auth_token
   TWILIO_MESSAGING_SERVICE_SID=your_service_sid_or_phone
   ```
2. Add contacts with phone numbers: `POST /api/wallets/{checksum}/contacts` (name + language + phone)
3. Phone numbers must include country code (e.g., `+4712345678`) and are validated/normalized to E.164 format
4. Automatic SMS notifications for all transactions

### Resend Email (Optional)
1. Set environment variables in `.env`:
   ```
   CANARY_MODE=saas
   RESEND_API_KEY=re_your-resend-api-key
   RESEND_FROM_EMAIL=notifications@canarybitcoin.com
   RESEND_FROM_NAME=Canary Wallet
   ```
2. Add contacts with email addresses: `POST /api/wallets/{checksum}/contacts` (name + language + email)
3. Email addresses are validated and used for both transaction notifications and auth verification
4. Automatic email notifications for all transactions sent from notifications@canarybitcoin.com

### Multiple Notification Methods (Current Architecture)
- **Current Implementation**: Contacts have single notification method per contact (auto-detected from address format)
- **Database Schema**: Normalized design with separate `contacts` and `contact_notification_methods` tables
- **Future Extensibility**: Architecture supports multiple methods per contact (same person can have SMS + ntfy + email)
- **Auto-routing**: System automatically routes to appropriate provider(s) based on available methods
- **Provider Independence**: All providers process all contacts, notification methods determine delivery targets

### Authentication (Optional)
Enable email/password authentication for multi-user support:
1. Set environment variables in `.env`:
   ```
   CANARY_MODE=saas
   JWT_SECRET=your_secure_jwt_secret_here
   # Email service for verification emails (optional in dev mode)
   SMTP_HOST=smtp.gmail.com
   SMTP_PORT=587
   SMTP_USERNAME=your-email@gmail.com
   SMTP_PASSWORD=your-app-password
   RESEND_FROM_EMAIL=your-email@gmail.com
   RESEND_FROM_NAME="Canary Wallet"
   FRONTEND_URL=http://localhost:3001
   ```
2. Frontend automatically shows login page when auth is enabled
3. Users register with email/password and receive verification email
4. Email verification required before login (except in dev mode)
5. Password reset functionality via email
6. JWT tokens stored in localStorage for session management
7. All wallet data isolated per user when auth is enabled
8. **Development Mode**: Pre-configured test users (`delivered+admin@resend.dev`, `delivered+alice@resend.dev`, `delivered+bob@resend.dev` all with password `password123`) - no email verification required

### Frontend Authentication Routes
- `/sign-up` - Registration form with email/password/name
- `/sign-up/success` - Post-registration success page with email verification instructions  
- `/sign-in` - Login form with email/password
- `/forgot-password` - Password reset request form
- `/reset-password/{token}` - Password reset form with token
- `/verify-email/{token}` - Email verification handler

## Stripe Integration & Billing

### Complete Subscription Management System
Canary features a fully integrated Stripe billing system with automatic subscription management, proactive limit enforcement, and seamless user experience.

### Backend Integration (Rust)
- **Custom Stripe Client**: Built with `reqwest` using Stripe API version `2025-07-30.basil` for latest compatibility
- **Manual Webhook Verification**: HMAC-SHA256 signature verification compatible with 2025 API changes
- **Dynamic Price Loading**: Loads all product tiers and pricing from Stripe on backend startup
- **Native Trial Subscriptions**: Creates Stripe customers and trial subscriptions immediately on user registration
- **Checkout Sessions**: `POST /api/checkout/create-session` - Creates Stripe Checkout sessions for plan upgrades
- **Customer Portal**: `POST /api/billing/customer-portal` - Generates Stripe Customer Portal sessions for subscription management
- **Webhook Processing**: `POST /api/stripe/webhook` - Processes Stripe webhooks for subscription lifecycle events
- **Billing Status**: `GET /api/billing/status` - Returns current subscription status and billing information

### Stripe Webhook Events Handled
- **`customer.created`** - Customer created in Stripe
- **`customer.subscription.created`** - Trial subscription activated, sets status to "trialing"  
- **`customer.subscription.updated`** - Subscription changes, detects trial ending transitions
- **`customer.subscription.deleted`** - Subscription cancelled, preserves access until expiration
- **`customer.subscription.trial_will_end`** - Fired 3 days before trial ends for notifications
- **`invoice.payment_succeeded`** - Successful payment, ensures continued access
- **`invoice.payment_failed`** - Failed payment, may affect service access
- **Customer ID Lookup**: Automatically finds users by `stripe_customer_id` for webhook processing

### Frontend Billing Integration
- **AuthContext Enhancement**: Integrated billing status into authentication context
- **Billing Status API**: Real-time subscription status, trial information, and customer portal access
- **Upgrade Modals**: Context-aware upgrade prompts for both wallet and contact limits
- **Customer Portal**: Direct access to Stripe Customer Portal for subscription management
- **Development Mode**: Test mode with Stripe test keys and webhook forwarding

### Subscription Lifecycle Management

#### Stripe Native Trial Management  
- **Immediate Stripe Integration**: Users created as Stripe customers on registration with trial subscriptions
- **30-day Team Trial**: All new users start with 30-day trial on Team tier (not Personal)
- **Stripe Trial Handling**: Uses Stripe's native `trial_period_days: 30` for subscription creation
- **Webhook-Driven Updates**: Trial status managed entirely through Stripe webhooks
- **Frontend Trial Display**: Shows "Team Trial: X days left" with Subscribe button during trial

#### Subscription States
- **`pending`** - User created, waiting for Stripe webhook confirmation
- **`trialing`** - Active 30-day trial period (Stripe native status)
- **`active`** - Paid subscription in good standing
- **`canceled`** - Subscription cancelled but access remains until `subscription_ends_at`
- **`expired`** - Trial or cancelled subscription has expired, no wallet syncing

#### Never Downgrade Policy
- **Tier Preservation**: Users never lose their subscription tier, even after expiration
- **Access Control**: Expired users keep tier but lose wallet syncing functionality
- **Historical Data**: All transaction history and wallet data remains accessible
- **Upgrade Path**: Clear upgrade prompts guide users back to active subscriptions

### Billing Configuration
Set up Stripe integration with environment variables:
```bash
# Stripe Configuration (2025 API)
STRIPE_SECRET_KEY=sk_test_... # or sk_live_...
STRIPE_WEBHOOK_SECRET=whsec_...

# Required: Stripe CLI for webhook forwarding in development
# stripe listen --forward-to localhost:3000/api/stripe/webhook
```

**Product Setup in Stripe Dashboard:**
- Create products with `metadata.tier` set to "personal" or "team"
- Add monthly recurring prices to each product (these will be the primary prices shown)
- Add yearly recurring prices with built-in discounts (typically 20% off)
- Configure yearly prices as "upsells" on the monthly prices in Stripe Dashboard
- Backend automatically loads all products and prices on startup
- Frontend displays monthly prices with yearly savings percentage calculated from price differences

### Stripe Upsell Implementation
- **Dashboard Configuration**: Yearly price upsells are configured directly in Stripe Dashboard on monthly prices
- **No Programmatic Upsells**: Backend does not add upsell line items; relies on Stripe Dashboard configuration
- **Price Discovery**: Backend automatically discovers both monthly and yearly prices for each tier
- **Savings Calculation**: Frontend calculates percentage savings by comparing `(monthly * 12 - yearly) / (monthly * 12) * 100`
- **Simplified UX**: Users see monthly price prominently with yearly savings displayed below
- **Stripe Checkout Integration**: When users click "Subscribe", they see the configured upsells in Stripe's checkout flow

### Customer Portal Features
- **Subscription Management**: Change plans, update billing frequency
- **Payment Methods**: Add/update credit cards and payment sources
- **Billing History**: Download invoices and view payment history
- **Cancellation**: Cancel subscriptions with end-of-period access
- **Reactivation**: Reactivate cancelled subscriptions

### Proactive Limit Enforcement System
- **Pre-flight Checks**: Limits validated before showing creation forms
- **Smart Upgrade Modals**: Context-aware upgrade prompts with plan comparisons
- **Unified Modal Component**: Single `UpgradeModal` component handles both wallet and contact limits
- **Flexible Messaging**: Dynamic content based on limit type (wallets vs contacts)

### Database Integration
```sql
-- Enhanced users table with subscription fields
CREATE TABLE users (
    -- Subscription management (Team trial by default)
    subscription_tier TEXT DEFAULT 'team' CHECK (subscription_tier IN ('personal', 'team')),
    trial_ends_at DATETIME DEFAULT (datetime('now', '+30 days')),
    subscription_status TEXT DEFAULT 'trial' CHECK (subscription_status IN ('pending', 'trial', 'trialing', 'active', 'expired', 'cancelled')),
    is_admin BOOLEAN NOT NULL DEFAULT 0,
    
    -- Stripe integration
    stripe_customer_id TEXT UNIQUE,
    stripe_subscription_id TEXT,
    subscription_started_at DATETIME,
    subscription_ends_at DATETIME
);
```

### Testing & Development

#### Required: Stripe CLI for Development
**IMPORTANT**: Stripe CLI is required for local development to receive webhooks that update user trial status.

1. **Install Stripe CLI**: https://stripe.com/docs/stripe-cli
2. **Login to Stripe**: `stripe login`
3. **Start webhook forwarding** (required for user registration):
   ```bash
   stripe listen --forward-to localhost:3000/api/stripe/webhook
   ```
4. **Start backend** in separate terminal:
   ```bash
   cd backend && cargo run
   ```

**Without Stripe CLI**: Users will register but remain in `pending` status (trial won't activate).
**With Stripe CLI**: Users register → Stripe creates subscription → Webhook fires → Status updates to `trialing`.

#### Development Features
- **Test Mode**: Complete Stripe integration works in test mode
- **Mock Customer Portal**: Fully functional customer portal in test mode  
- **Development Users**: Pre-configured test accounts with various subscription states

### Error Handling & Resilience
- **Webhook Retries**: Stripe automatically retries failed webhook deliveries
- **Graceful Degradation**: Billing failures don't break core wallet functionality  
- **User Communication**: Clear error messages for payment and subscription issues
- **Monitoring**: Comprehensive logging of all Stripe interactions

### Security Considerations
- **Manual Webhook Verification**: Custom HMAC-SHA256 signature verification compatible with Stripe 2025 API
- **Timestamp Validation**: Webhooks rejected if older than 5 minutes to prevent replay attacks  
- **API Key Management**: Secure handling of Stripe API keys via environment variables
- **Customer Data**: Minimal customer data stored locally, full details remain in Stripe
- **PCI Compliance**: No credit card data stored locally, all handled by Stripe

### SMS Contact Verification (Separate from Auth)
For adding SMS contacts, Twilio Verify is still used:
```
TWILIO_VERIFY_SERVICE_SID=VAxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
```
This is separate from the main authentication system and only used when users add phone number contacts.

## Storage
- **Wallets**: `database/{network}/wallets/*.sqlite` (BDK storage, user-isolated when auth enabled)
- **Metadata**: `database/{network}/metadata.sqlite` (normalized schema with users, contacts, contact_notification_methods, events, notification_logs, email_verification_tokens, password_reset_tokens)
- **Schema**: Single migration file (001_initial_schema.sql) with normalized design for extensible notification methods, multi-user support, and email authentication
- **Reset**: `./regtest-env/docker-utils.sh reset` removes all databases

## Address Management & Deep Scanning
The service uses BDK's address revelation mechanism with advanced deep scanning capabilities:

### Deep Scanning System
- **Fast Initial Response**: Wallet creation POST returns in ~1.5s using prefix-based script type detection
- **Background Deep Scan**: Progressive address revelation in batches (100, 200, 300, 400, 500 addresses)
- **Wallet State Management**: 'pending' → 'ready' transition with skeleton UI support
- **High Index Detection**: Successfully finds funds at any address index (tested up to index 250+)
- **Script Type Intelligence**: P2WPKH (Native SegWit) default for fresh XPUB wallets

### Address Revelation Mechanics  
- **Initial Sync**: Starts with 50 addresses, dynamically reveals more until finding 20 consecutive unused addresses
- **Incremental Sync**: After each sync, checks the highest used address index and ensures 20+ unused addresses are revealed beyond it
- **No Address Limits**: Automatically adapts to any wallet usage pattern, detecting transactions at any index (e.g., index 150, 200+)
- **Stop Gap**: Always maintains 20 consecutive unused addresses to prevent missing transactions

## Development Workflow
- **Testing**: `./regtest-env/docker-utils.sh` provides complete Bitcoin regtest environment
- **Database Management**: Single migration file for clean schema initialization

## Testing & Quality Assurance

### Frontend Test Suite
Comprehensive test coverage for subscription limits and user interactions:

- **Contact Limit Enforcement Tests** (`contact-limit-enforcement.test.tsx`):
  - Utility function testing for `getContactLimit()` and `hasReachedContactLimit()`  
  - Personal tier: 1 contact limit enforcement
  - Team tier: 5 contact limit enforcement
  - Edge cases: Zero contacts, null arrays, case-insensitive tiers
  - Integration scenarios: Alice (Personal), Bob (Team) user workflows

- **Upgrade Modal Tests** (`upgrade-modal-basic.test.tsx`):
  - Modal visibility and state management
  - Dynamic content for wallet vs contact limits
  - Tier badge display (Personal, Team)
  - Plural/singular form handling
  - Plan comparison integration
  - Default props and edge cases

- **Contact Modal Tests** (`contact-modal.test.tsx`):
  - Multi-provider notification setup (ntfy, SMS, email)
  - SMS verification flow with Twilio Verify
  - Email verification with auto-verification for user's own email
  - Edit mode with existing contact data handling
  - Error handling and validation
  - State cleanup and management

### Test Coverage Areas
- **Subscription Limits**: Proactive enforcement of wallet and contact limits
- **Modal Interactions**: Upgrade prompts and user flow validation  
- **Authentication**: Login, registration, email verification flows
- **Contact Management**: Multi-provider notification setup and verification
- **Error Handling**: Graceful error states and user feedback
- **Edge Cases**: Boundary conditions and invalid input handling

### Backend Testing
- **Integration Tests**: Stripe webhook processing and billing flows (`backend/tests/stripe_integration_tests.rs`)
- **Unit Tests**: Metadata operations (`backend/src/tests/metadata.rs`)
- **Test Execution**: Must use `--test-threads=1` to prevent SQLite database conflicts

## Code Standards
- No commented-out code (use git history)  
- Clean, maintainable codebase
- Plugin architecture for extensibility
- Generic database design supporting multiple notification providers
- Proper error handling with user-friendly messages
- Type-safe API contracts between frontend and backend