# Bitcoin Regtest Development Environment

This setup provides a complete Bitcoin regtest environment using Docker for fast development and testing.

## Quick Start

```bash
# From the scripts directory:
./dev.sh start

# Start backend with regtest environment
cd ../backend
CANARY_NETWORK=regtest cargo run

# Or export the variable for the session
export CANARY_NETWORK=regtest
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
./dev.sh init             # Create regtest wallets and add them to the backend
./dev.sh status           # Check environment status
./dev.sh stop             # Stop containers
./dev.sh reset            # Reset all data
./test-upgrade.sh         # Manual notification-preservation release gate
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

# 2. Create test wallets and add them to the backend
./dev.sh init

# 3. Run backend against regtest
cd ../backend
CANARY_NETWORK=regtest cargo run

# 4. Start frontend (in another terminal)
cd ../frontend
pnpm dev
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

**Important**: Set `CANARY_NETWORK=regtest` to use the local environment instead of mainnet.

Without this variable, the backend will connect to real Bitcoin mainnet servers!

## Release Screenshots

With the regtest services, backend, and frontend running, refresh all release screenshots with:

```bash
./update-readme-screenshots.sh
```

The script maintains the deterministic regtest fixture, captures six current UI screenshots in `../screenshots/` for the README and myNode Marketplace, then renders three 2160×1350 yellow presentation cards in `../screenshots/umbrel/` for the Umbrel gallery. The release script runs this workflow in Phase 1 and requires visual approval before it commits a release version bump.

## Upgrade Verification

`./test-upgrade.sh` is the manual regtest release gate for notification-preserving upgrades. It creates an isolated worktree from the source release, runs equivalent authenticated local-ntfy scenarios before and after the upgrade, and compares normalized delivery semantics plus database and Chromium UI state.

> **Destructive regtest warning:** the gate stops its temporary application processes and deletes the Docker volumes declared by `scripts/docker-compose.yml`. Those volumes contain only the local regtest Bitcoin, Fulcrum, ntfy, Postgres, NBXplorer, and BTCPay fixtures. Stop any local Canary servers using ports 3000/3001 first. Never point the gate at non-regtest or user data.

```bash
# Upgrade from the latest tag to the current branch
./test-upgrade.sh

# Release gate from v1.5.2 to an exact target ref
./test-upgrade.sh --from-tag v1.5.2 --to-ref HEAD

# Validate a release-candidate branch or commit
./test-upgrade.sh --from-tag v1.5.2 --to-ref origin/release/v1.6.0-rc

# Use another frontend port if localhost:3001 is already occupied
CANARY_UPGRADE_FRONTEND_PORT=3101 ./test-upgrade.sh
```

The success summary prints the resolved source and target SHAs and confirms the incoming/outgoing pending and confirmation, RBF, CPFP, balance-threshold, active fan-out, inactive non-delivery, and restart-dedup scenarios. The script exits non-zero for missing, extra, duplicate, wrong-topic, failed, or privacy-expanding delivery.

Playwright remains isolated under `scripts/playwright/`, so the gate does not change frontend workspace dependencies. Successful runs clean up by default; use `--keep-worktree` to retain the temporary checkout and artifacts. Failed runs always retain their worktree, exact refs, ntfy JSON, normalized manifests, transaction IDs, database snapshots/log extracts, and service logs under the printed `${TMPDIR:-/tmp}/canary-upgrade-test.*` path.

Run the same gate through the project adapter with `.agent-loop/checks.sh upgrade`. It intentionally remains a manual release gate rather than a required GitHub Actions workflow.
