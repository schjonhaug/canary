# Claude Code Configuration

**Development Status**: This project is in unreleased developer mode. Backwards compatibility is not a priority at this stage.

**License**: Open Source (FOSS)

**Version**: 0.5.1

## Project Overview
Canary is a Bitcoin wallet management service built in Rust that provides REST API endpoints for creating and managing Bitcoin wallets using BDK (Bitcoin Development Kit). Features include multipath descriptors, Electrum sync, transaction analysis, background sync, multi-language notifications (Norwegian and English) via configurable providers, and optional email/password authentication with email verification.

## Architecture
Built with a plugin-based notification system that allows extensible notification providers. Supports both ntfy.sh push notifications and Twilio SMS, configurable via environment variables. All providers share message formatting and notification logging functionality. Features optional JWT-based authentication with email/password and email verification for multi-user support. SMS verification via Twilio Verify is still used for contact verification when adding SMS contacts. Uses polling-based frontend updates rather than server-sent events.

## Development Commands

### Backend (Rust)
```bash
# Run backend (regtest default)
cd backend && cargo run

# Other networks
cd backend && cargo run -- --network testnet
cd backend && cargo run -- --network mainnet

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
cd frontend && npm test
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
│   ├── src/         # All source code (api.rs, main.rs, wallet management, notifications)
│   ├── database/    # Network-specific SQLite databases (database/{network}/)
│   └── migrations/  # Database schema migrations (001_initial_schema.sql, 002_dev_users.sql)
├── frontend/        # Next.js app with React components
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
- Environment: `CANARY_NETWORK=mainnet`, `CANARY_ELECTRUM_URL=...`, `CANARY_SYNC_INTERVAL=60`
- `.env` file in backend directory

**Defaults:**
- Regtest: tcp://127.0.0.1:50001
- Testnet: ssl://electrum.blockstream.info:60002  
- Mainnet: ssl://electrum.blockstream.info:50002
- Sync interval: 60 seconds (configurable via CANARY_SYNC_INTERVAL)
- Frontend polling: 60 seconds (configurable via NEXT_PUBLIC_SYNC_INTERVAL)

## Key Features
- **Optional Authentication**: Email/password authentication with email verification
- **Multi-user Support**: JWT-based sessions with user isolation when auth is enabled
- **Plugin-based Notifications**: Extensible provider system supporting ntfy.sh, Twilio SMS, and Resend email
- **Multiple Notification Methods**: Each contact can have multiple notification methods (SMS + email + ntfy + future: telegram + webhooks)
- **Multi-language Support**: Norwegian and English with proper Bitcoin amount formatting  
- **Auto-detection**: Automatically detects provider type from contact address format (phone numbers → SMS, email addresses → email, topics → ntfy)
- **Normalized Database**: Clean separation of contacts and notification methods for future extensibility
- **Notification Tracking**: Delivery status tracking with ✅/❌ UI indicators for all providers
- **Environment Configuration**: Provider selection via .env variables, no database config needed
- **Performance**: Async SQLite with r2d2 connection pooling
- **Real-time Updates**: Frontend polls wallet endpoints at configurable intervals (default 60s) for wallet and transaction updates
- **Transaction Analysis**: RBF/CPFP detection, accurate timestamps
- **Network Isolation**: Separate databases per Bitcoin network
- **Background Sync**: 4-second wallet sync intervals
- **Dynamic Address Revelation**: Automatically reveals addresses to maintain stop gap, ensuring transactions at any index are detected
- **User Onboarding**: Guided wallet creation with BIP39 mnemonic generation
- **Development Mode**: Quick login options for testing with pre-configured email accounts
- **Clean Registration Flow**: Dedicated success page (/sign-up/success) after registration with clear email verification instructions

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
   CANARY_ENABLE_AUTH=true
   RESEND_API_KEY=re_your-resend-api-key
   FROM_EMAIL=notifications@canarybitcoin.com
   FROM_NAME=Canary Wallet
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
   CANARY_ENABLE_AUTH=true
   JWT_SECRET=your_secure_jwt_secret_here
   # Email service for verification emails (optional in dev mode)
   SMTP_HOST=smtp.gmail.com
   SMTP_PORT=587
   SMTP_USERNAME=your-email@gmail.com
   SMTP_PASSWORD=your-app-password
   FROM_EMAIL=your-email@gmail.com
   FROM_NAME="Canary Wallet"
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

## Address Management
The service uses BDK's address revelation mechanism with a stop gap of 20:
- **Initial Sync**: Starts with 50 addresses, dynamically reveals more until finding 20 consecutive unused addresses
- **Incremental Sync**: After each sync, checks the highest used address index and ensures 20+ unused addresses are revealed beyond it
- **No Address Limits**: Automatically adapts to any wallet usage pattern, detecting transactions at any index (e.g., index 150, 200+)
- **Stop Gap**: Always maintains 20 consecutive unused addresses to prevent missing transactions

## Development Workflow
- **Testing**: `./regtest-env/docker-utils.sh` provides complete Bitcoin regtest environment
- **Database Management**: Single migration file for clean schema initialization

## Code Standards
- No commented-out code (use git history)  
- Clean, maintainable codebase
- Plugin architecture for extensibility
- Generic database design supporting multiple notification providers
- Proper error handling with user-friendly messages
- Type-safe API contracts between frontend and backend