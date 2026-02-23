# Bitcoin Regtest Development Environment

This setup provides a complete Bitcoin regtest environment using Docker for fast development and testing.

## Quick Start

```bash
# From the scripts directory:
./dev.sh start

# Start backend with regtest environment
cd ../backend
BITCOIN_NETWORK=regtest cargo run

# Or export the variable for the session
export BITCOIN_NETWORK=regtest
cargo run
```

## Docker Services

- **Bitcoin Core**: Regtest network with 101 pre-mined blocks
- **Fulcrum**: Electrum server providing protocol compatibility
- **ntfy**: Self-hosted push notification server with auth enabled
- **Ports**: Bitcoin RPC (18443), Electrum (50001), ntfy (2586)

## Utilities

### Environment Management
```bash
./dev.sh start            # Start Bitcoin + Fulcrum + ntfy containers
./dev.sh create-wallets   # Create Alice, Bob, Charlie test wallets
./dev.sh status           # Check environment status
./dev.sh stop             # Stop containers
./dev.sh reset            # Reset all data
```

### Wallet Commands
```bash
# Wallet basics
./dev.sh alice balance                    # Show wallet balance
./dev.sh alice address                    # Generate new address

# Wallet-to-wallet transfers
./dev.sh alice send bob 0.5               # Send 0.5 BTC from Alice to Bob
./dev.sh alice send miner max             # Drain Alice wallet to miner
./dev.sh miner send alice 1.0             # Refund Alice from miner

# Fund external addresses
./dev.sh alice fund <addr> 1.0            # Send 1 BTC to external address

# Available wallets: alice (1 BTC), bob (unfunded), charlie (0.5 BTC), miner (heavily funded)
```

### Mining & Testing
```bash
./dev.sh mine 6                           # Mine 6 blocks
./dev.sh alice rbf <txid>                 # Replace-by-fee
./dev.sh alice cpfp <txid>                # Child-pays-for-parent
./dev.sh run-tests <wallet-address>       # Comprehensive test suite
```

## Files

- `docker-compose.yml` - Container orchestration
- `bitcoin.conf` - Bitcoin Core regtest configuration  
- `fulcrum.conf` - Fulcrum Electrum server configuration
- `dev.sh` - Development utilities

## Development Workflow

### Complete Setup
```bash
# 1. Start regtest environment
./dev.sh start

# 2. Create test wallets (Alice, Bob, Charlie with funds)
./dev.sh create-wallets

# 3. Run backend against regtest
cd ../backend
BITCOIN_NETWORK=regtest cargo run

# 4. Add test wallets to backend (in another terminal)
cd ../scripts
./dev.sh add-wallets-to-backend
```

### Frontend Development
```bash
# Start frontend (in another terminal)
cd ../frontend
pnpm dev
```

### Testing Scenarios
```bash
# Test wallet-to-wallet transfers
./dev.sh alice send bob 0.5        # Transfer between test wallets
./dev.sh alice send miner max      # Drain wallet for testing
./dev.sh miner send alice 1.0      # Refund drained wallet

# Test external funding  
./dev.sh alice fund <wallet-address> 1.0

# Mine blocks to confirm transactions  
./dev.sh mine 6

# Advanced testing
./dev.sh run-tests <wallet-address>  # Complete test suite
```

## Local ntfy Server

`./dev.sh start` automatically sets up a local ntfy server with auth enabled (`deny-all` default access). A test user and access token are created on first start.

| Setting | Value |
|---------|-------|
| Server URL | `http://localhost:2586` |
| Username | `testuser` |
| Password | `testpassword` |

An access token is generated automatically — run `./dev.sh status` to see it.

### Testing from the frontend

1. Go to Settings and set the ntfy server URL to `http://localhost:2586`
2. Enter either username/password or the access token
3. Use "Send Test Notification" to verify the connection

### Testing from the command line

```bash
# Unauthenticated (should be denied)
curl http://localhost:2586/test-topic/json

# Username/password auth
curl -u testuser:testpassword http://localhost:2586/test-topic/json

# Access token auth (get token from ./dev.sh status)
curl -H "Authorization: Bearer tk_..." http://localhost:2586/test-topic/json
```

## Environment Variable

**Important**: Set `BITCOIN_NETWORK=regtest` to use the local environment instead of mainnet.

Without this variable, the backend will connect to real Bitcoin mainnet servers!