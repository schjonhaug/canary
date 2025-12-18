# System Tests

System tests (also called End-to-End tests) test the complete Canary application stack with real external dependencies. These tests use actual Bitcoin blockchain (regtest) in isolated Docker containers.

## Test Categories

### **`mined_directly_scenarios.rs`** - Transactions mined before sync
- `test_alice_partial_send_bob_mined_directly` - Basic mined directly partial send
- `test_alice_full_send_bob_mined_directly` - Mined directly full send (wallet emptied)
- `test_multiple_partial_sends_mined_directly` - Multiple partial sends in same block

### **`two_stage_send_scenarios.rs`** - Two-stage event flow testing
- `test_alice_partial_send_bob_two_stage` - Partial send unconfirmed → confirmed events
- `test_alice_full_send_bob_two_stage` - Full send (wallet drain) unconfirmed → confirmed events
- `test_multiple_partial_sends_bob_two_stage` - Multiple partial sends (3 mempool events → 1 confirmed event)

### **`advanced_transactions.rs`** - Complex transaction scenarios
- `test_alice_rbf_transaction_replacement` - Replace-By-Fee (RBF) testing
- `test_bob_cpfp_transaction_acceleration` - Child-Pays-For-Parent (CPFP) testing
- `test_multiple_rbf_replacements` - Multiple RBF replacements

### **`high_index_scanning.rs`** - Deep address scanning (high index detection)
- `test_high_index_fund_detection` - Detect funds at high address indexes (250+) for wallet recovery

### **`transaction_timestamp_scenarios.rs`** - Transaction timestamp validation
- `test_mempool_first_transaction_timestamp` - Verify mempool-first transactions show mempool time
- `test_direct_mining_transaction_timestamp` - Verify direct-mined transactions show block time

### **`balance_alert_scenarios.rs`** - Balance alert system testing
- `test_balance_alert_below_threshold` - Alert triggers when balance drops below threshold
- `test_balance_alert_above_threshold` - Alert triggers when balance rises above threshold
- `test_balance_drain_alert_equals_zero` - Alert triggers when wallet is completely drained
- `test_multiple_balance_alerts` - Multiple alerts on same wallet trigger independently
- `test_balance_alert_deactivation` - Deactivated alerts don't trigger

**⚠️ Important**: Balance alert tests must be run **individually** due to Docker port conflicts:
```bash
# Run balance alert tests one by one (not all together)
cargo test --test balance_alert_scenarios test_balance_alert_below_threshold -- --ignored --nocapture
cargo test --test balance_alert_scenarios test_balance_alert_above_threshold -- --ignored --nocapture
cargo test --test balance_alert_scenarios test_balance_drain_alert_equals_zero -- --ignored --nocapture
cargo test --test balance_alert_scenarios test_multiple_balance_alerts -- --ignored --nocapture
cargo test --test balance_alert_scenarios test_balance_alert_deactivation -- --ignored --nocapture
```

**⚠️ Important**: These timestamp tests must be run **individually** due to Docker timing dependencies:
```bash
# Run tests one by one (not all together)
cargo test --test transaction_timestamp_scenarios test_mempool_first_transaction_timestamp -- --ignored --nocapture
cargo test --test transaction_timestamp_scenarios test_direct_mining_transaction_timestamp -- --ignored --nocapture
```

## Prerequisites

- Docker installed and running
- Bitcoin Core Docker image available (`bitcoin/bitcoin:27.1`)

## Running System Tests

### Basic Test Execution (Clean Output)

```bash
# Run all system tests (if there's a combined test file)
cargo test --test system_tests -- --ignored

# Run specific test category
cargo test --test mined_directly_scenarios -- --ignored
cargo test --test two_stage_send_scenarios -- --ignored
cargo test --test high_index_scanning -- --ignored
cargo test --test advanced_transactions -- --ignored
cargo test --test balance_alert_scenarios -- --ignored

# Run individual test (clean output)
cargo test test_alice_partial_send_bob_mined_directly --test mined_directly_scenarios -- --ignored
cargo test test_multiple_partial_sends_mined_directly --test mined_directly_scenarios -- --ignored
cargo test test_balance_alert_below_threshold --test balance_alert_scenarios -- --ignored
cargo test test_balance_alert_above_threshold --test balance_alert_scenarios -- --ignored
cargo test test_balance_drain_alert_equals_zero --test balance_alert_scenarios -- --ignored
cargo test test_multiple_balance_alerts --test balance_alert_scenarios -- --ignored
cargo test test_balance_alert_deactivation --test balance_alert_scenarios -- --ignored
```

### Detailed Output for Manual Verification

```bash
# Run test category with full output (shows all debug info)
cargo test --test mined_directly_scenarios -- --ignored --nocapture
cargo test --test balance_alert_scenarios -- --ignored --nocapture

# Run individual test with detailed output
cargo test test_alice_full_send_bob_mined_directly --test mined_directly_scenarios -- --ignored --nocapture
cargo test test_balance_alert_below_threshold --test balance_alert_scenarios -- --ignored --nocapture
cargo test test_balance_alert_above_threshold --test balance_alert_scenarios -- --ignored --nocapture
cargo test test_balance_drain_alert_equals_zero --test balance_alert_scenarios -- --ignored --nocapture
cargo test test_multiple_balance_alerts --test balance_alert_scenarios -- --ignored --nocapture
cargo test test_balance_alert_deactivation --test balance_alert_scenarios -- --ignored --nocapture

# Run with debug logs (most verbose - shows internal BDK/wallet operations)
RUST_LOG=debug cargo test test_alice_partial_send_bob_mined_directly --test mined_directly_scenarios -- --ignored --nocapture
```

### Discovering Available Tests

```bash
# List all tests in a specific test file (most useful)
cargo test --test mined_directly_scenarios -- --list
cargo test --test two_stage_send_scenarios -- --list
cargo test --test balance_alert_scenarios -- --list

# Find all test names across all files using grep
grep -r "async fn test_" system_tests/ | grep -v ".rs~"

# Available test files
ls system_tests/*.rs
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
2. **Sets up deterministic wallets** (same as `scripts/dev.sh`)
3. **Runs real Bitcoin transactions** via `docker exec`
4. **Tests full pipeline**: Bitcoin → Electrum → BDK → Database → Events
5. **Automatically cleans up** containers and temporary data

## Test Environment Details

- **Alice**: Funded with 1.0 BTC at index 0
- **Bob**: Unfunded (empty wallet for receive scenarios)
- **Charlie**: Funded with 0.5 BTC at index 250 (high-index testing)

Each test gets fresh containers with predictable, deterministic wallet states.

### Balance Alert System Tests

The balance alert tests verify the complete notification system using real Bitcoin transactions:

- **Real Balance Detection**: Uses actual wallet balances from BDK after Bitcoin transactions
- **Alert Triggering**: Tests the `check_balance_alerts` method with real balance changes
- **Multiple Alert Types**: Above, Below, and Equals (wallet drain) scenarios
- **Threshold Testing**: Various BTC amounts (0.2, 0.3, 0.5, 0.8 BTC) as thresholds
- **Alert Management**: Tests deactivation and multiple alerts per wallet

**Key Features Tested:**
- Balance drops below threshold (Alice sends large amount)
- Balance rises above threshold (Bob receives funds)
- Wallet drain detection (balance equals exactly 0)
- Multiple alerts firing independently
- Deactivated alerts being ignored
- Auto-deactivation after triggering (alerts automatically deactivate and record timestamps)

**Implementation Details:**
- Uses real sync service integration - alerts are checked during wallet sync operations
- Tests verify `last_triggered_at` timestamps and automatic alert deactivation
- Handles transaction fees in wallet drain scenarios (balance may be small amount, not exactly 0)
- Balance alert checking only occurs when wallet has changes during sync

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