# Claude Code Configuration

This file contains project-specific information and preferences for Claude Code.

## Project Overview
Kanari is a Bitcoin wallet management service built in Rust that provides REST API endpoints for creating and managing Bitcoin wallets using BDK (Bitcoin Development Kit). The service supports multipath output descriptors, syncs with Electrum servers, includes advanced transaction analysis capabilities with automatic background synchronization, and real-time SMS notifications in Norwegian for all Bitcoin transaction events.

## Development Commands
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
kanari/
├── backend/                    # Rust backend service
│   ├── src/
│   │   ├── main.rs            # Application entry point with background sync and SMS worker
│   │   ├── api.rs             # REST API endpoints with OpenAPI docs
│   │   ├── config.rs          # Configuration management with CLI args and env vars
│   │   ├── wallet.rs          # Comprehensive wallet management using BDK
│   │   ├── electrum.rs        # Electrum client with dual sync modes
│   │   ├── metadata.rs        # Wallet metadata and contact database operations
│   │   ├── migrations.rs      # Database migration runner
│   │   └── sms.rs             # Norwegian SMS notifications via Twilio
│   ├── target/                # Build artifacts
│   ├── database/              # Network-specific database storage
│   │   ├── regtest/          # Regtest network databases
│   │   ├── testnet/          # Testnet network databases
│   │   └── mainnet/          # Mainnet network databases
│   ├── migrations/           # Database migration files
│   ├── Cargo.toml             # Rust dependencies
│   └── Cargo.lock             # Dependency lock file
├── frontend/                  # Next.js frontend application
│   ├── src/
│   │   ├── app/               # Next.js app router
│   │   └── components/        # React components
│   ├── package.json           # Node.js dependencies
│   └── next.config.ts         # Next.js configuration
├── regtest-env/               # Complete Bitcoin regtest environment
│   ├── docker-compose.yml     # Bitcoin Core + Fulcrum setup
│   ├── bitcoin.conf           # Bitcoin Core regtest configuration
│   ├── fulcrum.conf          # Fulcrum Electrum server configuration
│   ├── docker-utils.sh       # Comprehensive development and testing utilities
│   └── README.md             # Regtest environment documentation
└── CLAUDE.md                  # This file
```

## Key Dependencies
- `bdk_wallet = "2"` with `rusqlite` feature - Bitcoin wallet functionality
- `rusqlite = "0.31"` - SQLite database operations
- `bdk_electrum = "0.23"` - Electrum server integration
- `miniscript = "12.3"` - Bitcoin script processing
- `axum = "0.8"` - Web framework for REST API
- `tokio = "1.46"` with full features - Async runtime
- `serde = "1.0"` with derive - JSON serialization
- `serde_json = "1.0"` - JSON processing
- `utoipa = "5.4"` with axum_extras - OpenAPI documentation
- `utoipa-swagger-ui = "9"` with axum - Swagger UI integration
- `utoipa-axum = "0.2"` - OpenAPI-Axum integration
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
- `libphonenumber-js = "1.12.9"` (frontend) - Client-side phone number formatting

## API Endpoints
### Wallet Management
- `POST /wallets`: Create a new wallet from multipath descriptor and name
  - Requires both `name` (user-friendly) and `descriptor` (multipath) fields
  - Validates multipath descriptors and enforces descriptor uniqueness
  - Returns 201 (created) with full wallet metadata including ID, 400 (invalid), or 409 (duplicate descriptor)
  - Allows duplicate wallet names but enforces unique descriptors
- `GET /wallets`: List all wallets
  - Returns array of wallet metadata objects ordered by creation date (newest first)
  - Includes ID, name, descriptor, filename, and created_at for each wallet
- `GET /wallets/{id}`: Get a specific wallet by ID
  - Returns wallet metadata object or 404 if not found
  - Uses database ID as path parameter
- `DELETE /wallets/{id}`: Delete a wallet by ID
  - Completely removes wallet: unloads from BDK memory, deletes database file, removes metadata
  - Returns 204 (no content) on success, 404 if not found
  - Uses database ID as path parameter

### Wallet-Specific Contact Management
- `POST /wallets/{id}/contacts`: Create a new contact for a specific wallet
  - Requires `name` and `phone_number` fields (with country code, e.g., +4712345678)
  - Phone number validation with Norwegian locale support using libphonenumber-js
  - Returns 201 (created) with contact ID and metadata
  - Returns 404 if wallet not found, 400 for invalid phone number
- `GET /wallets/{id}/contacts`: Get all contacts for a specific wallet
  - Returns array of contact objects for the specified wallet, ordered by name
  - Each contact includes wallet_id, name, phone_number, and created_at
  - Returns 404 if wallet not found
- `DELETE /wallets/{wallet_id}/contacts/{contact_id}`: Delete a wallet-specific contact
  - Completely removes the contact and stops SMS notifications
  - Returns 204 (no content) on success, 404 if contact not found
  - Automatic CASCADE deletion: contact is deleted when wallet is deleted

### Twilio Configuration
- `POST /twilio/config`: Configure Twilio SMS settings
  - Requires `account_sid`, `auth_token`, and `messaging_service_sid`
  - Only one configuration supported (upserts existing)
  - Enables SMS sending for all wallets
- `GET /twilio/config`: Get current Twilio configuration
  - Returns configuration details (auth_token is included but should be secured)
  - Returns 404 if no configuration exists

### Blockchain Information
- `GET /block-headers/current`: Get current block header from database
  - Returns stored block header with height, hash, and Unix timestamp
  - Returns 404 if no block header is stored yet
  - Provides immediate block information on app startup
- `GET /block-headers/stream`: Server-Sent Events stream of real-time block headers
  - Streams live block header updates in JSON format
  - Auto-reconnects on connection loss
  - Content-Type: text/event-stream

### Documentation
- `/swagger-ui`: Interactive API documentation
- `/api-docs/openapi.json`: OpenAPI specification

## Network Configuration
Kanari supports multiple Bitcoin networks with configurable Electrum servers and network-specific database storage:

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
export KANARI_NETWORK=mainnet
export KANARI_ELECTRUM_URL=ssl://electrum.blockstream.info:50002
export KANARI_BIND_ADDRESS=0.0.0.0:3000
export KANARI_WALLET_DIR=/app/wallets
export KANARI_METADATA_DB=/app/metadata.sqlite

# Run with environment configuration
cargo run
```

#### Environment File (.env)
Create a `.env` file in the backend directory:
```env
KANARI_NETWORK=mainnet
KANARI_ELECTRUM_URL=ssl://electrum.blockstream.info:50002
KANARI_BIND_ADDRESS=127.0.0.1:3000
KANARI_WALLET_DIR=./wallets
KANARI_METADATA_DB=metadata.sqlite
```

### Default Electrum Servers
- **Regtest**: `tcp://127.0.0.1:50001` (local development)
- **Testnet**: `ssl://electrum.blockstream.info:60002` (Blockstream)
- **Mainnet**: `ssl://electrum.blockstream.info:50002` (Blockstream)

### Default Configuration
- **Bitcoin Network**: Regtest
- **Electrum Server**: tcp://127.0.0.1:50001
- **Web Server**: http://127.0.0.1:3000
- **Wallet Directory**: database/{network}/wallets
- **Metadata Database**: database/{network}/metadata.sqlite
- **Background Sync**: Every 4 seconds automatic wallet synchronization

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
- **Norwegian SMS Alerts**: Instant SMS notifications for all transaction events
  - **📤 Sending**: "Sender 0,00012345 BTC fra Min Wallet"
  - **📥 Receiving**: "Mottar 0,00012345 BTC til Min Wallet"
  - **✅ Confirmations**: "Sending bekreftet for Min Wallet" / "Mottak bekreftet: 0,00012345 BTC til Min Wallet"
  - **📤 RBF**: "RBF gebyr økning: +0,00001000 BTC for Min Wallet"
  - **🚀 CPFP**: "CPFP gebyr: 0,00001000 BTC for Min Wallet"
- **Norwegian Number Formatting**: Comma (,) as decimal separator, space ( ) as thousands separator
- **Multi-recipient**: Each wallet can have multiple contacts for SMS notifications
- **Delivery Tracking**: Complete SMS logs with Twilio SIDs and delivery status

### Real-Time Blockchain Integration
- **Block Header Subscription**: Real-time monitoring of new Bitcoin blocks via Electrum protocol
- **Server-Sent Events (SSE)**: Live streaming of block headers to frontend with automatic reconnection
- **Immediate Display**: Current block information shown instantly on app startup from database cache
- **Persistent Storage**: Block headers stored in SQLite for offline access and fast loading
- **Block Header Data**: Height, hash, and Unix timestamp for each block
- **Network-Aware**: Works across regtest, testnet, and mainnet with appropriate block explorers

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
- **Migration System**: Automatic database schema migrations using SQL files in `migrations/` directory
  - **Latest Migration**: `008_make_contacts_wallet_specific.sql` - Converts global contacts to wallet-specific contacts
- **Core Tables**:
  - `wallets`: Stores wallet IDs, names, descriptors, filenames, and creation timestamps
  - `transaction_events`: Bitcoin transaction events with type-safe enums and broadcast channels
  - `contact_persons`: Wallet-specific contact information (wallet_id, name, phone_number) with CASCADE DELETE
  - `twilio_config`: Single Twilio account configuration (account SID, auth token, messaging service SID)
  - `sms_logs`: Complete SMS delivery tracking (event ID, contact ID, Twilio SID, status, errors)
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
- **Fresh Start**: All databases are recreated from scratch on next backend startup with automatic migrations

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

## Architecture Notes
- Uses Rust 2024 edition with comprehensive async/await support
- **Event-Driven SMS**: tokio broadcast channels for real-time notifications without blocking wallet sync
- **Real-Time Blockchain Streaming**: Server-Sent Events (SSE) for live block header updates with tokio streams
- **Block Header Caching**: Persistent storage of current blockchain tip for immediate UI display
- **Norwegian Localization**: Custom number formatting (comma decimal, space thousands separator)
- **Twilio Integration**: Direct HTTP API calls using reqwest with proper authentication
- **Database-Driven Config**: All settings stored in SQLite for web interface management
- **Wallet-Specific Contacts**: Direct foreign key relationship with CASCADE DELETE for simplified contact management
- **Complete Logging**: Every SMS attempt tracked with delivery status and Twilio SIDs
- **Network Isolation**: Complete database separation per Bitcoin network to prevent cross-contamination
- **Migration System**: Automatic database schema migrations with version tracking
- **Configuration Management**: Flexible config system supporting CLI args, environment variables, and .env files
- **Dual-Mode Frontend**: REST API for initial data load + SSE for real-time updates
- Shared state management with Arc<Mutex<>> for thread-safe access
- Automatic loading of existing wallets on startup
- Full wallet sync performed on creation and incremental sync ongoing
- RESTful API design: uses proper HTTP methods and status codes
- Complete wallet and contact lifecycle management with proper foreign key constraints