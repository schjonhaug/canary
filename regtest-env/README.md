# Bitcoin Regtest Development Environment

This setup provides a complete Bitcoin regtest environment using Docker for fast development and testing.

## Quick Start

```bash
# From the regtest-env directory:
./docker-utils.sh start

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
- **Ports**: Bitcoin RPC (18443), Electrum (50001)

## Utilities

### Environment Management
```bash
./docker-utils.sh start            # Start Bitcoin + Fulcrum containers
./docker-utils.sh create-wallets   # Create Alice, Bob, Charlie test wallets
./docker-utils.sh status           # Check environment status
./docker-utils.sh stop             # Stop containers
./docker-utils.sh reset            # Reset all data
```

### Wallet Commands
```bash
# Wallet basics
./docker-utils.sh alice balance                    # Show wallet balance
./docker-utils.sh alice address                    # Generate new address

# Wallet-to-wallet transfers
./docker-utils.sh alice send bob 0.5               # Send 0.5 BTC from Alice to Bob
./docker-utils.sh alice send miner max             # Drain Alice wallet to miner
./docker-utils.sh miner send alice 1.0             # Refund Alice from miner

# Fund external addresses
./docker-utils.sh alice fund <addr> 1.0            # Send 1 BTC to external address

# Available wallets: alice (1 BTC), bob (unfunded), charlie (0.5 BTC), miner (heavily funded)
```

### Mining & Testing
```bash
./docker-utils.sh mine 6                           # Mine 6 blocks
./docker-utils.sh alice rbf <txid>                 # Replace-by-fee
./docker-utils.sh alice cpfp <txid>                # Child-pays-for-parent
./docker-utils.sh run-tests <wallet-address>       # Comprehensive test suite
```

## Files

- `docker-compose.yml` - Container orchestration
- `bitcoin.conf` - Bitcoin Core regtest configuration  
- `fulcrum.conf` - Fulcrum Electrum server configuration
- `docker-utils.sh` - Development utilities

## Development Workflow

### Complete Setup
```bash
# 1. Start regtest environment
./docker-utils.sh start

# 2. Create test wallets (Alice, Bob, Charlie with funds)
./docker-utils.sh create-wallets

# 3. Run backend against regtest
cd ../backend
BITCOIN_NETWORK=regtest cargo run

# 4. Add test wallets to backend (in another terminal)
cd ../regtest-env
./docker-utils.sh add-wallets-to-backend
```

### Frontend Development
```bash
# Start frontend (in another terminal)
cd ../frontend
npm run dev
```

### Testing Scenarios
```bash
# Test wallet-to-wallet transfers
./docker-utils.sh alice send bob 0.5        # Transfer between test wallets
./docker-utils.sh alice send miner max      # Drain wallet for testing
./docker-utils.sh miner send alice 1.0      # Refund drained wallet

# Test external funding  
./docker-utils.sh alice fund <wallet-address> 1.0

# Mine blocks to confirm transactions  
./docker-utils.sh mine 6

# Advanced testing
./docker-utils.sh run-tests <wallet-address>  # Complete test suite
```

## Environment Variable

**Important**: Set `BITCOIN_NETWORK=regtest` to use the local environment instead of mainnet.

Without this variable, the backend will connect to real Bitcoin mainnet servers!