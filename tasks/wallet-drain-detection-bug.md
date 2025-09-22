# Wallet Drain Detection Bug

## Status: CRITICAL BUG
**Priority**: High
**Discovered**: During transaction event duplicate detection investigation

## Summary
Wallet drain transactions (sending entire balance to another address) are not creating any transaction events in the database. While the Bitcoin transactions are successfully sent and confirmed on the blockchain, no corresponding events are stored in the metadata database.

## Problem Description
When a user drains their wallet by sending the maximum balance ("sending max"), the transaction is executed successfully but no `Send` events are created in the database. This means:
- Users don't see transaction history for wallet drain operations
- No notifications are sent for these transactions
- Wallet balance appears to change without any visible transaction events

## Steps to Reproduce
1. Fund Bob's wallet with some Bitcoin: `./docker-utils.sh miner sent bob 0.1`
2. Sync wallets to detect funding
3. Drain Bob's wallet: `./docker-utils.sh bob sending charlie max`
4. Sync wallets again
5. Check transaction events in database - no events are created

## Debug Test Evidence
The debug test in `/backend/tests/debug_wallet_drain.rs` confirms this bug:
```rust
// Test output shows:
// ❌ NO SEND EVENTS FOUND - This confirms the wallet drain detection bug!
// The transaction was sent but no events were created in the database.
// This means none of the transaction detection cases (1-4) are matching.
```

## Root Cause Analysis
The issue is in the wallet sync logic in `/backend/src/wallet.rs`. The transaction detection cases (Cases 1-4) are not properly matching wallet drain scenarios:

### Current Transaction Detection Cases:
1. **Case 1**: Receiving funds (trusted pending increase)
2. **Case 2**: Sending funds (trusted pending decrease) 
3. **Case 3**: Fast confirmation (transaction confirmed in same sync)
4. **Case 4**: Wallet drain (spending entire balance)

### The Problem:
None of these cases are triggering for actual wallet drain transactions, suggesting:
- The transaction type detection logic has gaps
- Wallet drain conditions are too restrictive or incorrect
- There may be timing issues with when sync occurs vs when transactions are detected

## Recent Fixes Applied
1. **Fixed wallet drain false positives**: Added condition `&& total_after.to_sat() == 0` to Case 4 to prevent normal transactions from being detected as wallet drains
2. **Fixed fast confirmation conflicts**: Added condition `&& total_after.to_sat() != 0` to fast confirmation detection to prevent conflicts with wallet drains

## Files Involved
- `/backend/src/wallet.rs` - Core sync logic with transaction detection (lines ~1600-1800)
- `/backend/tests/debug_wallet_drain.rs` - Debug test that reproduces the issue
- `/backend/tests/transaction_events_test.rs` - Comprehensive test framework

## Proposed Solutions
1. **Debug the sync conditions**: Add extensive logging to understand why Cases 1-4 are not matching
2. **Review wallet state changes**: Examine how BDK reports balance changes for wallet drain vs normal transactions
3. **Fix the detection logic**: Update the transaction detection cases to properly handle wallet drain scenarios
4. **Add comprehensive testing**: Ensure the fix works for various wallet drain scenarios (different amounts, timing, etc.)

## Impact
- **User Experience**: Missing transaction history for wallet drain operations
- **Notifications**: No alerts sent for significant wallet activities
- **Data Integrity**: Incomplete transaction event records
- **Trust**: Users may think transactions failed when they actually succeeded

## Test Coverage Needed
Once fixed, ensure testing covers:
- Wallet drain with various amounts
- Wallet drain with RBF transactions
- Wallet drain with CPFP transactions  
- Wallet drain timing (immediate vs delayed sync)
- Edge cases (dust amounts, fee calculations)

## Related Issues
This bug was discovered while investigating duplicate transaction events (which have been resolved). The wallet drain detection system needs to be robust and accurate for both preventing false positives and ensuring real wallet drains are properly detected.