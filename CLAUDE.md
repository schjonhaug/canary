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

### Contact Management
- `POST /contacts`: Create a new contact person
  - Requires `name` and `phone_number` fields (without country code)
  - Returns 201 (created) with contact ID and metadata
- `GET /contacts`: List all available contacts
  - Returns array of contact objects ordered by name
- `DELETE /contacts/{id}`: Delete a contact person
  - Returns 204 (no content) on success, 404 if not found

### Wallet-Contact Relationships
- `POST /wallets/{id}/contacts`: Add a contact to a wallet for SMS notifications
  - Requires `contact_id` in request body
  - Enables SMS alerts for that contact when wallet has transactions
- `GET /wallets/{id}/contacts`: Get all contacts linked to a wallet
  - Returns array of contact objects for the specified wallet
- `DELETE /wallets/{wallet_id}/contacts/{contact_id}`: Remove contact from wallet
  - Stops SMS notifications for that contact on this wallet

### Twilio Configuration
- `POST /twilio/config`: Configure Twilio SMS settings
  - Requires `account_sid`, `auth_token`, and `messaging_service_sid`
  - Only one configuration supported (upserts existing)
  - Enables SMS sending for all wallets
- `GET /twilio/config`: Get current Twilio configuration
  - Returns configuration details (auth_token is included but should be secured)
  - Returns 404 if no configuration exists

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

### Sync Capabilities
- **Full Scan**: Initial comprehensive sync with address revelation (up to 50 addresses)
- **Incremental Sync**: Ongoing updates with transaction cache management
- **Background Sync**: Automatic 4-second interval synchronization
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
- **Core Tables**:
  - `wallets`: Stores wallet IDs, names, descriptors, filenames, and creation timestamps
  - `transaction_events`: Bitcoin transaction events with type-safe enums and broadcast channels
  - `contact_persons`: Reusable contact information (name, phone number)
  - `wallet_contacts`: Junction table linking wallets to contacts (many-to-many)
  - `twilio_config`: Single Twilio account configuration (account SID, auth token, messaging service SID)
  - `sms_logs`: Complete SMS delivery tracking (event ID, contact ID, Twilio SID, status, errors)
- **Constraints**: 
  - `wallets.id` is PRIMARY KEY AUTOINCREMENT (unique wallet identifier)
  - `wallets.descriptor` has UNIQUE constraint (prevents duplicate wallets)
  - `wallets.name` allows duplicates (multiple wallets can have same name)
  - `contact_persons` can be reused across multiple wallets via junction table
  - `wallet_contacts` has UNIQUE constraint on (wallet_id, contact_id) pairs
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

### 2. Create Contacts
```bash
curl -X POST http://127.0.0.1:3000/contacts \
  -H "Content-Type: application/json" \
  -d '{
    "name": "John Doe",
    "phone_number": "+4712345678"
  }'
```

### 3. Link Contact to Wallet
```bash
curl -X POST http://127.0.0.1:3000/wallets/1/contacts \
  -H "Content-Type: application/json" \
  -d '{
    "contact_id": 1
  }'
```

### 4. SMS Notifications Active
Once configured, all Bitcoin transactions will automatically trigger Norwegian SMS notifications to all contacts linked to each wallet.

## Architecture Notes
- Uses Rust 2024 edition with comprehensive async/await support
- **Event-Driven SMS**: tokio broadcast channels for real-time notifications without blocking wallet sync
- **Norwegian Localization**: Custom number formatting (comma decimal, space thousands separator)
- **Twilio Integration**: Direct HTTP API calls using reqwest with proper authentication
- **Database-Driven Config**: All settings stored in SQLite for web interface management
- **Contact Reusability**: Junction table pattern allows sharing contacts across wallets
- **Complete Logging**: Every SMS attempt tracked with delivery status and Twilio SIDs
- **Network Isolation**: Complete database separation per Bitcoin network to prevent cross-contamination
- **Migration System**: Automatic database schema migrations with version tracking
- **Configuration Management**: Flexible config system supporting CLI args, environment variables, and .env files
- Shared state management with Arc<Mutex<>> for thread-safe access
- Automatic loading of existing wallets on startup
- Full wallet sync performed on creation and incremental sync ongoing
- RESTful API design: uses proper HTTP methods and status codes
- Complete wallet and contact lifecycle management with proper foreign key constraints