# Claude Code Configuration

This file contains project-specific information and preferences for Claude Code.

## Project Overview
Canary is a Bitcoin wallet management service built in Rust that provides REST API endpoints for creating and managing Bitcoin wallets using BDK (Bitcoin Development Kit). The service supports multipath output descriptors, syncs with Electrum servers, includes advanced transaction analysis capabilities with automatic background synchronization, and real-time SMS notifications in Norwegian for all Bitcoin transaction events.

## Development Commands

### Backend (Rust)
```bash
# Run the backend server (regtest - default)
cd backend && cargo run

# Run on testnet
cd backend && cargo run -- --network testnet

# Run on mainnet
cd backend && cargo run -- --network mainnet

# Run with custom Electrum server
cd backend && cargo run -- --network mainnet --electrum-url ssl://custom.electrum.server:50002

# Build the project
cd backend && cargo build

# Check code
cd backend && cargo check

# Run tests
cd backend && cargo test

# Format code
cd backend && cargo fmt

# Lint code
cd backend && cargo clippy
```

### Frontend (Next.js)
```bash
# Run the frontend development server (http://localhost:3001)
cd frontend && npm run dev

# Build the frontend for production
cd frontend && npm run build

# Start the frontend production server
cd frontend && npm start

# Lint frontend code
cd frontend && npm run lint

# Run frontend tests
cd frontend && npm test

# Run frontend tests in watch mode
cd frontend && npm run test:watch
```

### Docker Environment
```bash

# Start regtest environment (Bitcoin Core + Fulcrum)
cd regtest-env && docker-compose up -d

# Stop regtest environment
cd regtest-env && docker-compose down

# Reset regtest environment (removes all data)
cd regtest-env && ./docker-utils.sh reset

# Comprehensive testing commands
cd regtest-env && ./docker-utils.sh run-tests <wallet_address>  # Full test suite
cd regtest-env && ./docker-utils.sh mempool-purge restart       # Test mempool purge
cd regtest-env && ./docker-utils.sh reorg 3                     # Test blockchain reorg
cd regtest-env && ./docker-utils.sh get-mempool-txid 0          # Get mempool TXID
```

## Project Structure
```
canary/
├── backend/                    # Rust backend service
│   ├── src/
│   │   ├── main.rs            # Application entry point with background sync and SMS worker
│   │   ├── api.rs             # REST API endpoints with OpenAPI docs
│   │   ├── config.rs          # Configuration management with CLI args and env vars
│   │   ├── wallet.rs          # Comprehensive wallet management using BDK
│   │   ├── electrum.rs        # Electrum client with dual sync modes
│   │   ├── metadata.rs        # Wallet metadata and contact database operations
│   │   ├── migrations.rs      # Database migration runner
│   │   ├── sms.rs             # Norwegian SMS notifications via Twilio
│   │   └── tests/             # Test modules (104 tests total)
│   │       ├── mod.rs         # Test module definitions
│   │       ├── api.rs         # API endpoint tests (957 lines)
│   │       ├── electrum.rs    # Electrum integration tests (217 lines)
│   │       ├── metadata.rs    # Database operation tests (282 lines)
│   │       ├── sms.rs         # SMS notification tests (478 lines)
│   │       └── wallet.rs      # Wallet management tests (555 lines)
│   ├── target/                # Build artifacts
│   ├── database/              # Network-specific database storage
│   │   ├── regtest/          # Regtest network databases
│   │   ├── testnet/          # Testnet network databases
│   │   └── mainnet/          # Mainnet network databases
│   ├── migrations/           # Database migration files
│   │   └── 001_initial_schema.sql # Complete initial database schema
│   ├── Cargo.toml             # Rust dependencies
│   └── Cargo.lock             # Dependency lock file
├── frontend/                  # Next.js frontend application
│   ├── src/
│   │   ├── app/               # Next.js app router
│   │   │   ├── layout.tsx     # Root layout component
│   │   │   ├── page.tsx       # Main dashboard page
│   │   │   └── globals.css    # Global styles
│   │   ├── components/        # React components
│   │   │   ├── ui/            # Reusable UI components (shadcn/ui)
│   │   │   ├── __tests__/     # Component tests
│   │   │   ├── wallet-cards.tsx         # Wallet display cards
│   │   │   ├── transaction-events.tsx   # Transaction event list
│   │   │   ├── create-wallet-modal.tsx  # New wallet creation
│   │   │   ├── edit-wallet-modal.tsx    # Wallet editing
│   │   │   ├── delete-wallet-modal.tsx  # Wallet deletion
│   │   │   ├── settings-modal.tsx       # App settings
│   │   │   └── block-status.tsx         # Blockchain status
│   │   ├── hooks/             # Custom React hooks
│   │   │   ├── useDashboard.ts    # Dashboard data management (hybrid REST + SSE)
│   │   │   ├── useBlockHeaders.ts # Block header streaming
│   │   │   └── useModal.ts        # Modal state management
│   │   ├── lib/               # Utility libraries
│   │   │   ├── api.ts         # API client functions
│   │   │   └── utils.ts       # Utility functions and SVG processing
│   │   └── types/             # TypeScript type definitions
│   │       └── index.ts       # Shared type definitions
│   ├── public/                # Static assets
│   │   └── images/            # Image assets
│   │       ├── canary.svg     # Canary logo (also used for wallet icons)
│   │       └── canary-in-a-coalmine.svg     # Canary in a coalmine logo
│   ├── package.json           # Node.js dependencies
│   ├── next.config.ts         # Next.js configuration
│   ├── tailwind.config.ts     # Tailwind CSS configuration
│   └── jest.config.js         # Jest testing configuration
├── regtest-env/               # Complete Bitcoin regtest environment
│   ├── docker-compose.yml     # Bitcoin Core + Fulcrum setup
│   ├── bitcoin.conf           # Bitcoin Core regtest configuration
│   ├── fulcrum.conf          # Fulcrum Electrum server configuration
│   ├── docker-utils.sh       # Comprehensive development and testing utilities
│   └── README.md             # Regtest environment documentation
└── CLAUDE.md                  # This file
```

## Key Dependencies

### Backend Dependencies
- `bdk_wallet = "2"` with `rusqlite` feature - Bitcoin wallet functionality
- `bdk_electrum = "0.23"` - Electrum server integration
- `miniscript = "12.3"` - Bitcoin script processing
- `axum = "0.8"` - Web framework for REST API
- `tokio = "1.46"` with full features - Async runtime
- `serde = "1.0"` with derive - JSON serialization
- `serde_json = "1.0"` - JSON processing
- `utoipa = "5.4"` with axum_extras - OpenAPI documentation
- `utoipa-swagger-ui = "9"` with axum - Swagger UI integration
- `anyhow = "1.0"` - Error handling
- `secp256k1 = "0.31"` - Secp256k1 elliptic curve operations
- `tower-http = "0.6"` with cors features - HTTP middleware for CORS
- `clap = "4.0"` with derive - Command line argument parsing
- `dotenvy = "0.15"` - Environment variable loading from .env files
- `reqwest = "0.12"` with json features - HTTP client for Twilio API
- `base64 = "0.22"` - Base64 encoding for Twilio authentication
- `chrono = "0.4"` - Date and time handling for SMS logs
- `phonenumber = "0.3"` - International phone number validation and formatting
- `tokio-stream = "0.1"` with sync features - Async stream processing for SSE
- `futures-util = "0.3"` - Stream utilities for Server-Sent Events

### Frontend Dependencies
- `next = "15.3.5"` - React framework with app router
- `react = "19.0.0"` - React library
- `react-dom = "19.0.0"` - React DOM rendering
- `tailwindcss = "4"` - Utility-first CSS framework
- `libphonenumber-js = "1.12.9"` - Client-side phone number formatting
- `lucide-react = "0.525.0"` - Icon library
- `@radix-ui/react-dialog = "1.1.14"` - Modal components
- `@radix-ui/react-label = "2.1.7"` - Form label components
- `class-variance-authority = "0.7.1"` - CSS class variant utilities
- `clsx = "2.1.1"` - Conditional className utility
- `tailwind-merge = "3.3.1"` - Tailwind CSS class merging
- `@testing-library/react = "16.3.0"` - React testing utilities
- `@testing-library/jest-dom = "6.6.3"` - Jest DOM matchers
- `jest = "30.0.4"` - JavaScript testing framework
- `typescript = "5"` - TypeScript language support

## API Endpoints

### Dashboard Data (Hybrid REST + SSE Approach)
- `GET /api/dashboard`: **Initial dashboard state** - REST endpoint for immediate data
  - **Content-Type**: `application/json`
  - **Data**: Complete dashboard state with all wallets and transaction events
  - **Format**: JSON object with `timestamp`, `wallets` array, and `events` array
  - **Usage**: Called on page load for immediate data availability
- `GET /api/dashboard/stream`: **Real-time updates** - Server-Sent Events for changes
  - **Content-Type**: `text/event-stream`
  - **Data**: Dashboard updates only when wallet data changes
  - **Format**: JSON objects with `timestamp`, `wallets` array, and `events` array
  - **Frequency**: Only sent when actual changes occur (balance changes, new transactions)
  - **Auto-reconnect**: Frontend handles automatic reconnection on connection loss
  - **Optimization**: No periodic updates - only sends on actual data changes
- `GET /api/block-headers/stream`: Real-time Bitcoin block header updates
  - **Content-Type**: `text/event-stream`
  - **Data**: New block headers as they arrive from Electrum server
  - **Format**: JSON objects with `height`, `hash`, and `timestamp`

### Wallet Management (CRUD Operations)
- `POST /api/wallets`: Create a new wallet from multipath descriptor and name
  - Requires both `name` (user-friendly) and `descriptor` (multipath) fields
  - Validates multipath descriptors and enforces descriptor uniqueness
  - Returns 201 (created) with full wallet metadata including ID, 400 (invalid), or 409 (duplicate descriptor)
  - Allows duplicate wallet names but enforces unique descriptors
- `GET /api/wallets/{id}`: Get a specific wallet by ID
  - Returns wallet metadata object or 404 if not found
  - Uses database ID as path parameter
- `PUT /api/wallets/{id}`: Update wallet name
  - Allows updating the user-friendly name of an existing wallet
  - Returns 200 on success, 404 if wallet not found
- `DELETE /api/wallets/{id}`: Delete a wallet by ID
  - Completely removes wallet: unloads from BDK memory, deletes database file, removes metadata
  - Returns 204 (no content) on success, 404 if not found
  - Uses database ID as path parameter

### Contact Management
- `POST /api/wallets/{id}/contacts`: Create a new contact for a specific wallet
  - Requires `name` and `phone_number` fields (with country code, e.g., +4712345678)
  - Phone number validation with Norwegian locale support using libphonenumber-js
  - Returns 201 (created) with contact ID and metadata
  - Returns 404 if wallet not found, 400 for invalid phone number
  - **Triggers dashboard SSE update** to immediately reflect contact count changes
- `GET /api/wallets/{id}/contacts`: Get all contacts for a specific wallet
  - Returns array of contact objects for the specified wallet, ordered by name
  - Each contact includes wallet_id, name, phone_number, and created_at
  - Returns 404 if wallet not found
- `DELETE /api/wallets/{wallet_id}/contacts/{contact_id}`: Delete a wallet-specific contact
  - Completely removes the contact and stops SMS notifications
  - Returns 204 (no content) on success, 404 if contact not found
  - Automatic CASCADE deletion: contact is deleted when wallet is deleted
  - **Triggers dashboard SSE update** to immediately reflect contact count changes

### Configuration
- `POST /api/twilio/config`: Configure Twilio SMS settings
  - Requires `account_sid`, `auth_token`, and `messaging_service_sid`
  - Only one configuration supported (upserts existing)
  - Enables SMS sending for all wallets
- `GET /api/twilio/config`: Get current Twilio configuration
  - Returns configuration details (auth_token is included but should be secured)
  - Returns 404 if no configuration exists

### Utility Endpoints
- `GET /api/block-headers/current`: Get current block header from database
  - Returns stored block header with height, hash, and Unix timestamp
  - Returns 404 if no block header is stored yet
  - Provides immediate block information on app startup
- `/swagger-ui`: Interactive API documentation
- `/api-docs/openapi.json`: OpenAPI specification

### ⚠️ Legacy Endpoints (Removed)
The following bulk data endpoints have been **removed** and replaced with real-time SSE streams:
- ~~`GET /api/wallets`~~ → Use `GET /api/dashboard/stream` for all wallet data
- ~~`GET /api/transaction-events`~~ → Use `GET /api/dashboard/stream` for all events
- ~~`GET /api/transaction-events/{id}/sms-recipients`~~ → SMS data included in dashboard stream

## Network Configuration
Canary supports multiple Bitcoin networks with configurable Electrum servers and network-specific database storage:

### Supported Networks
- **Regtest** (default): For development and testing
- **Testnet**: For testing with testnet bitcoins
- **Mainnet**: For production use with real bitcoins

### Configuration Methods

#### Command Line Arguments
```bash
# Network selection
cargo run -- --network regtest
cargo run -- --network testnet
cargo run -- --network mainnet

# Custom Electrum server
cargo run -- --network mainnet --electrum-url ssl://your.electrum.server:50002

# Custom bind address and paths
cargo run -- --bind-address 0.0.0.0:8080 --wallet-dir /custom/wallets --metadata-db custom.sqlite
```

#### Environment Variables
```bash
# Network configuration
export CANARY_NETWORK=mainnet
export CANARY_ELECTRUM_URL=ssl://electrum.blockstream.info:50002
export CANARY_BIND_ADDRESS=0.0.0.0:3000
export CANARY_WALLET_DIR=/app/wallets
export CANARY_METADATA_DB=/app/metadata.sqlite

# Run with environment configuration
cargo run
```

#### Environment File (.env)
Create a `.env` file in the backend directory:
```env
CANARY_NETWORK=mainnet
CANARY_ELECTRUM_URL=ssl://electrum.blockstream.info:50002
CANARY_BIND_ADDRESS=127.0.0.1:3000
CANARY_WALLET_DIR=./wallets
CANARY_METADATA_DB=metadata.sqlite
```

### Default Electrum Servers
- **Regtest**: `tcp://127.0.0.1:50001` (local development)
- **Testnet**: `ssl://electrum.blockstream.info:60002` (Blockstream)
- **Mainnet**: `ssl://electrum.blockstream.info:50002` (Blockstream)

### Default Configuration
- **Bitcoin Network**: Regtest
- **Electrum Server**: tcp://127.0.0.1:50001
- **Backend API Server**: http://127.0.0.1:3000
- **Frontend Development Server**: http://127.0.0.1:3001
- **Wallet Directory**: database/{network}/wallets
- **Metadata Database**: database/{network}/metadata.sqlite
- **Background Sync**: Every 4 seconds automatic wallet synchronization with optimized dashboard updates (only sends when changes detected)

### Network-Specific Storage
- **Database Structure**: All databases are stored in network-specific subdirectories under `database/`
- **Wallet Isolation**: Each network maintains separate wallet databases to prevent cross-network contamination
- **Metadata Isolation**: Each network has its own metadata database for contacts, events, and SMS logs

## Advanced Features
### Transaction Analysis & SMS Notifications
- Real-time balance change detection and reporting with user-friendly wallet names
- Transaction type classification (send, receive, confirmation)
- RBF (Replace-By-Fee) detection
- CPFP (Child-Pays-For-Parent) detection
- Detailed Bitcoin amount formatting in Norwegian locale
- Balance change notifications display wallet names instead of technical IDs
- **Accurate Transaction Timestamps**: Historical transactions use actual block timestamps from Electrum, unconfirmed transactions use mempool `first_seen` timestamps
- **Norwegian SMS Alerts**: Instant SMS notifications for all transaction events
  - **📤 Sending**: "Sender 0,00012345 BTC fra Min Wallet"
  - **📥 Receiving**: "Mottar 0,00012345 BTC til Min Wallet"
  - **✅ Confirmations**: "Sending bekreftet for Min Wallet" / "Mottak bekreftet: 0,00012345 BTC til Min Wallet"
  - **📤 RBF**: "RBF gebyr økning: +0,00001000 BTC for Min Wallet"
  - **🚀 CPFP**: "CPFP gebyr: 0,00001000 BTC for Min Wallet"
- **Norwegian Number Formatting**: Comma (,) as decimal separator, space ( ) as thousands separator
- **Multi-recipient**: Each wallet can have multiple contacts for SMS notifications
- **Delivery Tracking**: Complete SMS logs with message content, Twilio SIDs and delivery status

### Real-Time Blockchain Integration
- **Block Header Subscription**: Real-time monitoring of new Bitcoin blocks via Electrum protocol
- **Server-Sent Events (SSE)**: Live streaming of block headers to frontend with automatic reconnection
- **Immediate Display**: Current block information shown instantly on app startup from database cache
- **Persistent Storage**: Block headers stored in SQLite for offline access and fast loading
- **Block Header Data**: Height, hash, and Unix timestamp for each block
- **Network-Aware**: Works across regtest, testnet, and mainnet with appropriate block explorers

### Optimized Dashboard Data Flow
- **Hybrid REST + SSE Architecture**: Eliminates loading states and reduces network traffic
- **Initial Load**: REST endpoint (`GET /api/dashboard`) provides immediate data on page load
- **Real-time Updates**: SSE stream sends updates when wallet data actually changes
- **Change Detection**: Leverages existing wallet sync logic to detect balance and transaction changes
- **Contact Management Integration**: Adding/deleting contacts triggers immediate dashboard updates via SSE
- **SMS Processing Updates**: Automatic dashboard refresh after SMS processing completes to display recipient names
- **Performance**: ~98% reduction in SSE traffic (from every 4 seconds to only on changes)

### Sync Capabilities
- **Full Scan**: Initial comprehensive sync with address revelation (up to 50 addresses)
- **Incremental Sync**: Ongoing updates with transaction cache management
- **Background Sync**: Automatic 4-second interval synchronization with block header polling
- **Progress Indicators**: Detailed logging with keychain information

### Development Environment
- Complete Docker-based regtest setup with Bitcoin Core + Fulcrum Electrum server
- Comprehensive testing utilities integrated in `docker-utils.sh`:
  - **Alice/Bob wallet management**: Funded (Alice) and unfunded (Bob) test wallets with deterministic descriptors
  - **Advanced transaction testing**: RBF (Replace-By-Fee), CPFP (Child-Pays-For-Parent), consolidation
  - **Mempool testing**: Transaction purge scenarios (restart, double-spend, low-fee), status monitoring
  - **Blockchain testing**: Reorganization simulation, tip invalidation/reconsideration
  - **Automated test suite**: Comprehensive testing with wallet address integration
  - **Helper utilities**: Mempool TXID extraction, mining, environment management
- Complete environment reset capability that removes all SQLite databases

## Storage Details
### BDK Wallet Storage
- **Database**: SQLite with `.sqlite` extension
- **Location**: `database/{network}/wallets/` directory (completely removed on reset)
- **Naming**: Uses BDK's `wallet_name_from_descriptor()` function for standardized filenames
- **Persistence**: Automatic wallet loading on startup
- **Sync Parameters**: STOP_GAP=20, BATCH_SIZE=5

### Wallet Metadata & SMS Storage
- **Database**: `database/{network}/metadata.sqlite` (completely removed on reset)
- **Database Schema**: Single initial schema file (`001_initial_schema.sql`) - no incremental migrations needed since app isn't released yet
- **Core Tables**:
  - `wallets`: Stores wallet IDs, names, descriptors, filenames, and creation timestamps
  - `transaction_events`: Bitcoin transaction events with accurate timestamps (block time for confirmed, first_seen for unconfirmed) and type-safe enums
  - `contact_persons`: Wallet-specific contact information (wallet_id, name, phone_number) with CASCADE DELETE
  - `twilio_config`: Single Twilio account configuration (account SID, auth token, messaging service SID)
  - `sms_logs`: Complete SMS delivery tracking (event ID, contact ID, message content, Twilio SID, status, errors)
  - `current_block_header`: Current blockchain tip (height, hash, timestamp, updated_at)
- **Constraints**: 
  - `wallets.id` is PRIMARY KEY AUTOINCREMENT (unique wallet identifier)
  - `wallets.descriptor` has UNIQUE constraint (prevents duplicate wallets)
  - `wallets.name` allows duplicates (multiple wallets can have same name)
  - `contact_persons.wallet_id` has FOREIGN KEY with CASCADE DELETE (contacts deleted when wallet is deleted)
  - `contact_persons` are wallet-specific (no longer reusable across wallets)
- **Event System**: Write-through pattern with database persistence + tokio broadcast channels for real-time SMS

### Reset Behavior
- **Complete Cleanup**: `./docker-utils.sh reset` removes all SQLite databases
- **BDK Wallets**: Entire `database/{network}/wallets/` directory is deleted (not just individual files)
- **Metadata**: `database/{network}/metadata.sqlite` file is completely removed (not just table contents)
- **Fresh Start**: All databases are recreated from scratch on next backend startup with the initial schema

## SMS Integration Setup

### 1. Configure Twilio
```bash
curl -X POST http://127.0.0.1:3000/twilio/config \
  -H "Content-Type: application/json" \
  -d '{
    "account_sid": "ACxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
    "auth_token": "your_auth_token_here",
    "messaging_service_sid": "MGxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
  }'
```

### 2. Create Wallet-Specific Contacts
```bash
curl -X POST http://127.0.0.1:3000/wallets/1/contacts \
  -H "Content-Type: application/json" \
  -d '{
    "name": "John Doe",
    "phone_number": "+4712345678"
  }'
```

### 3. SMS Notifications Active
Once configured, all Bitcoin transactions will automatically trigger Norwegian SMS notifications to all contacts associated with each wallet. Contacts are now created directly for specific wallets, eliminating the need for separate linking steps.

## Code Quality Standards
### No Commented-Out Code Policy
**NEVER leave commented-out code in the codebase.** This project maintains strict cleanliness standards:
- Remove all commented-out code blocks, functions, variables, and imports
- Use git history if you need to recover deleted code
- Replace commented-out code with proper documentation comments if explanation is needed
- Commented-out code is considered noise and reduces code readability

This policy ensures:
- Clean, maintainable codebase
- No confusion between active code and dead code
- Proper focus on current functionality
- Better code reviews and debugging experience

## Architecture Notes
- Uses Rust 2024 edition with comprehensive async/await support
- **Event-Driven SMS**: tokio broadcast channels for real-time notifications without blocking wallet sync
- **Real-Time Blockchain Streaming**: Server-Sent Events (SSE) for live block header updates with tokio streams
- **Block Header Caching**: Persistent storage of current blockchain tip for immediate UI display
- **Norwegian Localization**: Custom number formatting (comma decimal, space thousands separator)
- **Twilio Integration**: Direct HTTP API calls using reqwest with proper authentication
- **Database-Driven Config**: All settings stored in SQLite for web interface management
- **Wallet-Specific Contacts**: Direct foreign key relationship with CASCADE DELETE for simplified contact management
- **Complete Logging**: Every SMS attempt tracked with full message content, delivery status and Twilio SIDs
- **Network Isolation**: Complete database separation per Bitcoin network to prevent cross-contamination
- **Migration System**: Automatic database schema migrations with version tracking
- **Configuration Management**: Flexible config system supporting CLI args, environment variables, and .env files
- **Hybrid Dashboard Architecture**: REST API for initial data load + SSE for real-time updates only when changes occur
- **Optimized SSE**: Dashboard updates sent only on actual wallet changes, not on every sync cycle
- **SMS Recipients Real-Time Updates**: Dual dashboard updates ensure SMS recipient data appears immediately without manual refresh
- **Accurate Transaction Timestamps**: Historical transactions fetch actual block timestamps from Electrum servers, unconfirmed transactions use BDK's `first_seen` mempool timestamps, no caching needed as timestamps are stored permanently in database
- **Comprehensive Test Coverage**: 104 tests covering API endpoints, wallet management, SMS integration, network isolation, RBF/CPFP detection, background sync behavior, and error handling
- Shared state management with Arc<Mutex<>> for thread-safe access
- Automatic loading of existing wallets on startup
- Full wallet sync performed on creation and incremental sync ongoing
- RESTful API design: uses proper HTTP methods and status codes
- Complete wallet and contact lifecycle management with proper foreign key constraints