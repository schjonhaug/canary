# Claude Code Configuration

**Development Status**: This project is in unreleased developer mode. Backwards compatibility is not a priority at this stage.

**License**: Open Source (FOSS)

## Project Overview
Canary is a Bitcoin wallet management service built in Rust that provides REST API endpoints for creating and managing Bitcoin wallets using BDK (Bitcoin Development Kit). Features include multipath descriptors, Electrum sync, transaction analysis, background sync, and multi-language notifications (Norwegian and English) via configurable providers.

## Architecture
Built with a plugin-based notification system that allows extensible notification providers. Supports both ntfy.sh push notifications and Twilio SMS, configurable via environment variables. All providers share message formatting and notification logging functionality.

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
│   ├── database/    # Network-specific SQLite databases  
│   └── migrations/  # Single initial database schema
├── frontend/        # Next.js app with React components
├── regtest-env/    # Docker Bitcoin + Fulcrum setup
└── CLAUDE.md       # This file
```

## Key Dependencies
- **Backend**: BDK wallet v2, SQLite with r2d2 pooling, Axum web framework, ntfy.sh + Twilio notifications
- **Frontend**: Next.js 15.3.5, React 19, Tailwind CSS 4, shadcn/ui components

## API Endpoints

### Dashboard (Hybrid REST + SSE)
- `GET /api/dashboard` - Initial state (REST)
- `GET /api/dashboard/stream` - Real-time updates (SSE)
- `GET /api/block-headers/stream` - Block header updates (SSE)

### Wallet Management
- `POST /api/wallets` - Create wallet (name + descriptor)
- `GET /api/wallets/{id}` - Get wallet
- `PUT /api/wallets/{id}` - Update wallet name
- `DELETE /api/wallets/{id}` - Delete wallet

### Contact Management  
- `POST /api/wallets/{id}/contacts` - Add contact with automatic provider detection (name + contact_address + language)
- `GET /api/wallets/{id}/contacts` - List contacts with notification methods
- `DELETE /api/wallets/{wallet_id}/contacts/{contact_id}` - Remove contact and all notification methods

### Notification System
- `GET /api/providers` - List available and configured notification providers
- **Multiple Notification Methods**: Contacts can receive notifications through multiple providers simultaneously
- **Auto-detection**: Phone numbers (starting with +) → SMS, topics → ntfy
- **Normalized Database**: Separate tables for contacts and notification methods for extensibility
- Generic notification logs with delivery status tracking for all providers
- `/swagger-ui` - API documentation

## Network Configuration
Supports regtest (default), testnet, mainnet with configurable Electrum servers.

**Configuration methods:**
- CLI: `cargo run -- --network mainnet --electrum-url ssl://electrum.blockstream.info:50002`
- Environment: `CANARY_NETWORK=mainnet`, `CANARY_ELECTRUM_URL=...`
- `.env` file in backend directory

**Defaults:**
- Regtest: tcp://127.0.0.1:50001
- Testnet: ssl://electrum.blockstream.info:60002  
- Mainnet: ssl://electrum.blockstream.info:50002

## Key Features
- **Plugin-based Notifications**: Extensible provider system supporting ntfy.sh and Twilio SMS
- **Multiple Notification Methods**: Each contact can have multiple notification methods (future: SMS + email + ntfy + telegram + webhooks)
- **Multi-language Support**: Norwegian and English with proper Bitcoin amount formatting  
- **Auto-detection**: Automatically detects provider type from contact address format
- **Normalized Database**: Clean separation of contacts and notification methods for future extensibility
- **Notification Tracking**: Delivery status tracking with ✅/❌ UI indicators for all providers
- **Environment Configuration**: Provider selection via .env variables, no database config needed
- **Performance**: Async SQLite with r2d2 connection pooling
- **Real-time**: Hybrid REST + SSE architecture, block header streaming
- **Transaction Analysis**: RBF/CPFP detection, accurate timestamps
- **Network Isolation**: Separate databases per Bitcoin network
- **Background Sync**: 4-second wallet sync intervals
- **Dynamic Address Revelation**: Automatically reveals addresses to maintain stop gap, ensuring transactions at any index are detected

## Notification Setup

### ntfy.sh (Default, always enabled)
1. Add contacts with ntfy topics: `POST /api/wallets/{id}/contacts` (name + language + topic)
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
2. Add contacts with phone numbers: `POST /api/wallets/{id}/contacts` (name + language + phone)
3. Phone numbers must include country code (e.g., `+4712345678`)
4. Automatic SMS notifications for all transactions

### Multiple Notification Methods (New Architecture)
- **Current Implementation**: Contacts have single notification method per contact (auto-detected from address format)
- **Database Schema**: Normalized design with separate `contacts` and `contact_notification_methods` tables
- **Future Extensibility**: Architecture supports multiple methods per contact (same person can have SMS + ntfy + email)
- **Auto-routing**: System automatically routes to appropriate provider(s) based on available methods
- **Provider Independence**: All providers process all contacts, notification methods determine delivery targets

## Storage
- **Wallets**: `database/{network}/wallets/*.sqlite` (BDK storage)
- **Metadata**: `database/{network}/metadata.sqlite` (normalized schema with contacts, contact_notification_methods, events, notification_logs)
- **Schema**: Single migration file with normalized design for extensible notification methods
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