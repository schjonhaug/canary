# Claude Code Configuration

This file contains project-specific information and preferences for Claude Code.

## Project Overview
TxRay is a Bitcoin wallet management service built in Rust that provides REST API endpoints for creating and managing Bitcoin wallets using BDK (Bitcoin Development Kit). The service supports multipath output descriptors, syncs with Electrum servers, and includes advanced transaction analysis capabilities with automatic background synchronization.

## Development Commands
```bash
# Run the backend server
cd backend && cargo run

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
```

## Project Structure
```
txray/
├── backend/                    # Rust backend service
│   ├── src/
│   │   ├── main.rs            # Application entry point with background sync
│   │   ├── api.rs             # REST API endpoints with OpenAPI docs
│   │   ├── wallet.rs          # Comprehensive wallet management using BDK
│   │   ├── electrum.rs        # Electrum client with dual sync modes
│   │   └── metadata.rs        # Wallet metadata database operations
│   ├── target/                # Build artifacts
│   ├── wallets/               # BDK SQLite wallet database files
│   ├── txray.sqlite           # Wallet metadata database
│   ├── Cargo.toml             # Rust dependencies
│   └── Cargo.lock             # Dependency lock file
├── regtest-env/               # Complete Bitcoin regtest environment
│   ├── docker-compose.yml     # Bitcoin Core + Fulcrum setup
│   ├── bitcoin.conf           # Bitcoin Core regtest configuration
│   ├── fulcrum.conf          # Fulcrum Electrum server configuration
│   ├── docker-utils.sh       # Development utilities script
│   ├── README.md             # Regtest environment documentation
│   └── test-*.sh             # Transaction testing scripts
└── CLAUDE.md                  # This file
```

## Key Dependencies
- `bdk_wallet = "2"` with `rusqlite` feature - Bitcoin wallet functionality
- `rusqlite = "0.31"` - SQLite database operations
- `bdk_electrum = "0.23"` - Electrum server integration
- `miniscript = "12.3"` - Bitcoin script processing
- `axum = "0.8"` - Web framework for REST API
- `tokio = "1.45"` with full features - Async runtime
- `serde = "1.0"` with derive - JSON serialization
- `serde_json = "1.0"` - JSON processing
- `utoipa = "5.4"` with axum_extras - OpenAPI documentation
- `utoipa-swagger-ui = "9"` with axum - Swagger UI integration
- `utoipa-axum = "0.2"` - OpenAPI-Axum integration
- `anyhow = "1.0"` - Error handling
- `secp256k1 = "0.29"` - Secp256k1 elliptic curve operations

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

### Documentation
- `/swagger-ui`: Interactive API documentation
- `/api-docs/openapi.json`: OpenAPI specification

## Network Configuration
- **Bitcoin Network**: Regtest (hardcoded)
- **Electrum Server**: tcp://127.0.0.1:50001
- **Web Server**: http://127.0.0.1:3000
- **Background Sync**: Every 4 seconds automatic wallet synchronization

## Advanced Features
### Transaction Analysis
- Real-time balance change detection and reporting with user-friendly wallet names
- Transaction type classification (send, receive, confirmation)
- RBF (Replace-By-Fee) detection
- CPFP (Child-Pays-For-Parent) detection
- Detailed Bitcoin amount formatting
- Balance change notifications display wallet names instead of technical IDs

### Sync Capabilities
- **Full Scan**: Initial comprehensive sync with address revelation (up to 50 addresses)
- **Incremental Sync**: Ongoing updates with transaction cache management
- **Background Sync**: Automatic 4-second interval synchronization
- **Progress Indicators**: Detailed logging with keychain information

### Development Environment
- Complete Docker-based regtest setup
- Bitcoin Core + Fulcrum Electrum server
- Comprehensive testing utilities including RBF and CPFP scenarios
- Transaction testing scripts with Alice/Bob wallet management
- Complete environment reset capability that removes all SQLite databases

## Storage Details
### BDK Wallet Storage
- **Database**: SQLite with `.sqlite` extension
- **Location**: `wallets/` directory (completely removed on reset)
- **Naming**: Uses BDK's `wallet_name_from_descriptor()` function for standardized filenames
- **Persistence**: Automatic wallet loading on startup
- **Sync Parameters**: STOP_GAP=20, BATCH_SIZE=5

### Wallet Metadata Storage
- **Database**: `txray.sqlite` in backend root directory (completely removed on reset)
- **Schema**: Stores wallet IDs, names, descriptors, filenames, and creation timestamps
- **Constraints**: 
  - `id` field is PRIMARY KEY AUTOINCREMENT (unique wallet identifier)
  - `descriptor` field has UNIQUE constraint (prevents duplicate wallets)
  - `name` field allows duplicates (multiple wallets can have same name)
- **Purpose**: Maps user-friendly names to BDK wallet files and provides API access via IDs

### Reset Behavior
- **Complete Cleanup**: `./docker-utils.sh reset` removes all SQLite databases
- **BDK Wallets**: Entire `wallets/` directory is deleted (not just individual files)
- **Metadata**: `txray.sqlite` file is completely removed (not just table contents)
- **Fresh Start**: All databases are recreated from scratch on next backend startup

## Notes
- Uses Rust 2024 edition
- Comprehensive error handling throughout
- Shared state management with Arc<Mutex<>>
- Automatic loading of existing wallets on startup
- Full wallet sync performed on creation and incremental sync ongoing
- RESTful API design: uses proper HTTP methods and status codes
- Complete wallet lifecycle management: create, read, list, delete operations