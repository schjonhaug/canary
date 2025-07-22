# Claude Code Configuration

**Development Status**: This project is in unreleased developer mode. Backwards compatibility is not a priority at this stage.

## Project Overview
Canary is a Bitcoin wallet management service built in Rust that provides REST API endpoints for creating and managing Bitcoin wallets using BDK (Bitcoin Development Kit). Features multipath descriptors, Electrum sync, transaction analysis, background sync, and multi-language SMS notifications (Norwegian and English).

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
├── backend/           # Rust service with BDK wallet management
│   ├── src/          # Source files (api.rs, wallet.rs, electrum.rs, sms.rs, etc.)
│   ├── database/     # Network-specific SQLite databases  
│   └── migrations/   # Database schema
├── frontend/         # Next.js app with React components
├── regtest-env/     # Docker Bitcoin + Fulcrum setup
└── CLAUDE.md        # This file
```

## Key Dependencies
- **Backend**: BDK wallet v2, Axum, Tokio, SQLite with r2d2 pooling, Twilio SMS
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
- `POST /api/wallets/{id}/contacts` - Add contact (name + phone)
- `GET /api/wallets/{id}/contacts` - List contacts
- `DELETE /api/wallets/{wallet_id}/contacts/{contact_id}` - Remove contact

### Configuration
- `POST /api/twilio/config` - Configure SMS (account_sid, auth_token, messaging_service_sid)
- `GET /api/twilio/config` - Get SMS config
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
- **Multi-language SMS**: Real-time transaction notifications in Norwegian and English via Twilio
- **Performance**: Async SQLite with r2d2 connection pooling
- **Real-time**: Hybrid REST + SSE architecture, block header streaming
- **Transaction Analysis**: RBF/CPFP detection, accurate timestamps
- **Network Isolation**: Separate databases per Bitcoin network
- **Background Sync**: 4-second wallet sync intervals

## SMS Setup
1. Configure Twilio: `POST /api/twilio/config`
2. Add contacts: `POST /api/wallets/{id}/contacts` 
3. Automatic notifications for all transactions

## Storage
- **Wallets**: `database/{network}/wallets/*.sqlite` (BDK storage)
- **Metadata**: `database/{network}/metadata.sqlite` (contacts, events, SMS logs)
- **Reset**: `./docker-utils.sh reset` removes all databases

## Code Standards
- No commented-out code (use git history)
- Clean, maintainable codebase
- 104 comprehensive tests