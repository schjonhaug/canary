# System Tests

System tests (also called End-to-End tests) test the complete Canary application stack with real external dependencies. These tests use actual Bitcoin blockchain (regtest) in isolated Docker containers.

## Test Categories

- **`fast_confirmation_scenarios.rs`** - Tests for transactions mined before sync (direct to confirmed)
- **`wallet_drain_scenarios.rs`** - Tests for wallet drain detection and related scenarios
- **`high_index_scanning.rs`** - Tests for deep address scanning (high index detection)
- **`transaction_flows.rs`** - Tests for normal send/receive transaction flows
- **`advanced_transactions.rs`** - Tests for complex transaction scenarios
- **`notification_verification.rs`** - Tests for notification system verification

## Prerequisites

- Docker installed and running
- Bitcoin Core Docker image available (`bitcoin/bitcoin:27.1`)

## Running System Tests

### Basic Test Execution (Clean Output)

```bash
# Run all system tests (if there's a combined test file)
cargo test --test system_tests -- --ignored

# Run specific test category
cargo test --test fast_confirmation_scenarios -- --ignored
cargo test --test wallet_drain_scenarios -- --ignored
cargo test --test high_index_scanning -- --ignored
cargo test --test transaction_flows -- --ignored
cargo test --test advanced_transactions -- --ignored
cargo test --test notification_verification -- --ignored

# Run individual test (clean output)
cargo test test_alice_sent_bob_direct_confirmed --test fast_confirmation_scenarios -- --ignored
cargo test test_multiple_fast_confirmations --test fast_confirmation_scenarios -- --ignored
```

### Detailed Output for Manual Verification

```bash
# Run test category with full output (shows all debug info)
cargo test --test fast_confirmation_scenarios -- --ignored --nocapture

# Run individual test with detailed output
cargo test test_alice_sent_bob_max_direct_drain --test fast_confirmation_scenarios -- --ignored --nocapture

# Run with debug logs (most verbose - shows internal BDK/wallet operations)
RUST_LOG=debug cargo test test_alice_sent_bob_direct_confirmed --test fast_confirmation_scenarios -- --ignored --nocapture
```

### What Detailed Output Shows

The `--nocapture` flag reveals:
- 🏗️ **Infrastructure Setup**: Docker containers, Bitcoin Core startup, Fulcrum sync
- 💰 **Wallet Creation**: Descriptors, address generation, initial funding details  
- 📊 **Balance Tracking**: Before/after balance tables with exact amounts
- ⚡ **Transaction Flow**: Send → Mine → Sync with timing information
- 🔍 **Event Detection**: New events created with amounts and confirmation status
- ✅ **Test Verification**: All assertions and expected vs actual values
- 🧹 **Cleanup**: Automatic Docker container removal

## Test Architecture

Each system test:

1. **Creates isolated Docker Bitcoin container** with unique name
2. **Sets up deterministic wallets** (same as `regtest-env/docker-utils.sh`)
3. **Runs real Bitcoin transactions** via `docker exec`
4. **Tests full pipeline**: Bitcoin → Electrum → BDK → Database → Events
5. **Automatically cleans up** containers and temporary data

## Test Environment Details

- **Alice**: Funded with 1.0 BTC at index 0
- **Bob**: Unfunded (empty wallet for receive scenarios)
- **Charlie**: Funded with 0.5 BTC at index 250 (high-index testing)

Each test gets fresh containers with predictable, deterministic wallet states.

## Performance Notes

System tests are slower than integration tests (~15-30 seconds each) because they:
- Start/stop Docker containers
- Generate Bitcoin addresses up to index 250
- Wait for blockchain confirmations
- Perform actual wallet sync operations

They are marked with `#[ignore]` and must be explicitly run with `-- --ignored`.

## Docker Port Management

System tests create isolated Docker containers with random ports. If tests fail due to port conflicts:

```bash
# Clean up orphaned test containers
docker stop $(docker ps -q --filter "name=test-") 2>/dev/null || true
docker rm $(docker ps -aq --filter "name=test-") 2>/dev/null || true

# Or run tests individually to avoid conflicts
cargo test test_specific_test --test test_file -- --ignored
```