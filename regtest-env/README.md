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

```bash
./docker-utils.sh mine 6           # Mine 6 blocks
./docker-utils.sh fund <addr> 1.0  # Send 1 BTC to address
./docker-utils.sh balance          # Show wallet balance
./docker-utils.sh status           # Check environment status
./docker-utils.sh stop             # Stop containers
./docker-utils.sh reset            # Reset all data
```

## Files

- `docker-compose.yml` - Container orchestration
- `bitcoin.conf` - Bitcoin Core regtest configuration  
- `fulcrum.conf` - Fulcrum Electrum server configuration
- `docker-utils.sh` - Development utilities

## Development Workflow

### Backend Development
```bash
# Start regtest environment
./docker-utils.sh start

# Run backend against regtest (one-time)
cd ../backend
BITCOIN_NETWORK=regtest cargo run

# Or set permanently for your session
export BITCOIN_NETWORK=regtest
cargo run
```

### Frontend Development
```bash
# Start frontend (in another terminal)
cd ../frontend
pnpm dev
```

### Testing Transactions
```bash
# Fund addresses for testing
./docker-utils.sh fund <wallet-address> 1.0

# Mine blocks to confirm transactions  
./docker-utils.sh mine 6

# Check wallet balance
./docker-utils.sh balance
```

## Environment Variable

**Important**: Set `BITCOIN_NETWORK=regtest` to use the local environment instead of mainnet.

Without this variable, the backend will connect to real Bitcoin mainnet servers!