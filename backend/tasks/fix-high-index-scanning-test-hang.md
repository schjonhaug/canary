# Fix high_index_scanning Test Hang

## Problem

The `high_index_scanning` system test hangs indefinitely and never completes. The test was terminated after hanging for multiple hours during a version bump.

### Test Details

- **Test File**: `backend/system_tests/high_index_scanning.rs`
- **Test Function**: `test_high_index_fund_detection()`
- **Expected Behavior**: Test should complete within a reasonable time (< 10 minutes)
- **Actual Behavior**: Test hangs indefinitely, requiring manual termination (SIGTERM)

### Error Output

```
test test_high_index_fund_detection ... ./run-system-tests.sh: line 54:  3926 Terminated: 15
❌ FAILED: high_index_scanning
```

## Root Cause Analysis

The test creates three wallets:
1. **Alice** - Normal wallet (index 0-20)
2. **Bob** - Normal wallet (index 0-20)
3. **Charlie** - Deep scanning wallet (start_index: "250")

Charlie's wallet is funded at address index 250, requiring the wallet to:
1. Progressively reveal addresses from 0 → 250+
2. Scan blockchain to find the transaction at index 250
3. Mark wallet as "ready" in database

### Where It Hangs

Based on Fulcrum connection logs, the last activity was at 20:06:51, suggesting the test hangs during:

1. **`IsolatedTestEnvironment::new_with_charlie()`** - Test setup phase
2. **`wait_for_wallets_ready()`** - Waiting for Charlie wallet to be marked as ready
3. The 30-second timeout in `wait_for_wallets_ready()` should trigger but doesn't

### Key Code Locations

- **Test**: `backend/system_tests/high_index_scanning.rs:14` - `test_high_index_fund_detection()`
- **Environment Setup**: `backend/system_tests/common/docker_environment.rs:227` - `new_with_charlie()`
- **Wallet Creation**: `backend/system_tests/common/docker_environment.rs:326-361` - Creates Charlie with `start_index: "250"`
- **Timeout Check**: `backend/system_tests/common/docker_environment.rs:36-69` - `wait_for_wallets_ready()` with 30s timeout

## Hypotheses

1. **Background task not running**: `create_wallet_non_blocking()` may not actually be non-blocking for deep scanning wallets
2. **Wallet never marked ready**: Deep scanning completes but wallet state never updates to "ready"
3. **Timeout mechanism broken**: The 30-second timeout loop may have a bug preventing it from triggering
4. **Docker container issue**: Test containers may have exited early, preventing Electrum sync

## Investigation Steps

1. **Check if wallet creation is truly non-blocking**:
   - Add logging to `create_wallet_non_blocking()` to verify background task spawning
   - Check if deep scanning task is actually running asynchronously

2. **Monitor wallet state changes**:
   - Add logging to track when wallets transition from 'pending' → 'ready'
   - Check database directly during test execution to see wallet states

3. **Verify timeout mechanism**:
   - Add debug logging to `wait_for_wallets_ready()` timeout loop
   - Confirm the timeout actually fires after 30 seconds

4. **Check Docker containers**:
   - Verify test containers remain running during the test
   - Check Fulcrum logs for errors or connection issues

## Temporary Workaround

The test has been commented out in `run-system-tests.sh` to unblock version bumps:

```bash
TESTS=(
    "two_stage_send_scenarios"
    # "high_index_scanning"  # TODO: Skipped - test hangs during deep scanning at index 250
    "advanced_transactions"
    ...
)
```

## Next Steps

1. **Add comprehensive logging** to wallet creation and deep scanning code
2. **Run test manually** with `--nocapture` to see detailed output:
   ```bash
   cd backend
   cargo test --test high_index_scanning -- --ignored --test-threads=1 --nocapture
   ```
3. **Fix the underlying issue** - either:
   - Make deep scanning truly asynchronous
   - Increase timeout appropriately for deep scanning wallets
   - Fix whatever is preventing the wallet from being marked as ready
4. **Re-enable the test** once fixed

## Priority

**High** - This test validates critical deep scanning functionality needed for wallet recovery scenarios. Users must be able to recover wallets that have been used at high address indexes.

## Related Files

- `backend/system_tests/high_index_scanning.rs`
- `backend/system_tests/common/docker_environment.rs`
- `backend/src/wallet.rs` - `create_wallet_non_blocking()` implementation
- `backend/run-system-tests.sh` - Test runner (test currently commented out)
