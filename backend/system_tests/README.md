# System Tests

System tests (also called End-to-End tests) test the complete Canary application stack with real external dependencies. These tests use actual Bitcoin blockchain (regtest) in isolated Docker containers.

## Test Categories

- **`wallet_drain_scenarios.rs`** - Tests for the wallet drain detection bug and related scenarios
- **`high_index_scanning.rs`** - Tests for deep address scanning (high index detection)
- **`transaction_flows.rs`** - Tests for normal send/receive transaction flows

## Prerequisites

- Docker installed and running
- Bitcoin Core Docker image available (`bitcoin/bitcoin:27.1`)

## Running System Tests

```bash
# Run all system tests
cargo test --manifest-path=../Cargo.toml --test system_tests

# Run specific test category
cargo test --manifest-path=../Cargo.toml --test wallet_drain_scenarios

# Run individual test with output
cargo test --manifest-path=../Cargo.toml --test wallet_drain_scenarios test_alice_wallet_drain -- --ignored --nocapture
```

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