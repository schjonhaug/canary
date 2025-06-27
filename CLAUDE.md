# Claude Code Configuration

This file contains project-specific information and preferences for Claude Code.

## Project Overview
TxRay is a Bitcoin wallet management service built in Rust that provides REST API endpoints for creating and managing Bitcoin wallets using BDK (Bitcoin Development Kit). The service supports multipath output descriptors and syncs with Electrum servers.

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
```

## Project Structure
```
txray/
├── backend/                 # Rust backend service
│   ├── src/
│   │   ├── main.rs         # Application entry point
│   │   ├── api.rs          # REST API endpoints with OpenAPI docs
│   │   ├── wallet.rs       # Wallet management logic using BDK
│   │   └── electrum.rs     # Electrum client integration
│   ├── wallets/            # Wallet database files
│   └── Cargo.toml          # Rust dependencies
└── CLAUDE.md               # This file
```

## Key Dependencies
- `bdk_wallet`: Bitcoin wallet functionality
- `bdk_electrum`: Electrum server integration
- `axum`: Web framework for REST API
- `utoipa`: OpenAPI documentation generation
- `tokio`: Async runtime

## API Endpoints
- `POST /wallet`: Create a new wallet from multipath descriptor
- `/swagger-ui`: Interactive API documentation

## Network Configuration
- Currently configured for Bitcoin Regtest
- Electrum server: tcp://127.0.0.1:50001
- Web server: http://127.0.0.1:3000

## Notes
- Wallets are persisted as .db files in the `wallets/` directory
- Each wallet filename is based on the descriptor checksum
- The service automatically loads existing wallets on startup
- Full wallet sync with Electrum is performed on creation and loading