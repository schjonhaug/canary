# Additional System Test Scenarios

## Overview
This document outlines additional comprehensive system test scenarios to enhance our Bitcoin wallet transaction detection and notification testing beyond the core scenarios.

## Additional Test Scenarios

### 1. Multiple UTXOs Consolidation
**Purpose**: Test UTXO management and consolidation transactions
- Alice has multiple small UTXOs (e.g., 0.1, 0.05, 0.03 BTC)
- Alice sends them all to Bob in one consolidation transaction
- Verify correct total amount calculation and single transaction event generation
- Check notification content reflects consolidated amount

### 2. Self-Send/Change Handling
**Purpose**: Test change detection and event classification
- Alice sends Bitcoin to her own wallet (different address)
- Verify system correctly identifies this as internal transfer vs external send
- Check that change outputs are handled properly
- Ensure notifications (if any) are appropriate for self-sends

### 3. Batch Transactions
**Purpose**: Test multi-recipient transaction handling
- Alice sends to multiple recipients (Bob and Charlie) in one transaction
- Verify correct event generation for each recipient
- Check that fees are allocated appropriately across outputs
- Test notification content for batch sends

### 4. Zero-Confirmation Double Spend
**Purpose**: Test conflict detection and handling
- Alice sends UTXOs to Bob (unconfirmed)
- Alice immediately tries to send same UTXOs to Charlie
- Verify system detects conflict appropriately
- Check notification behavior for conflicted transactions

### 5. Chain of Unconfirmed Transactions
**Purpose**: Test mempool chain handling
- Alice sends to Bob (unconfirmed)
- Bob immediately sends received funds to Charlie (unconfirmed)
- Verify both transactions are detected properly before confirmation
- Check notification sequence and timing

### 6. Wallet Recovery After Transactions
**Purpose**: Test persistence and recovery
- Perform transactions
- Restart wallet manager/service
- Verify all transaction history persists correctly
- Check that sync process doesn't create duplicate events

### 7. Large Amount Notifications
**Purpose**: Test notification formatting with edge cases
- Test with very large BTC amounts (e.g., 21 BTC, 100 BTC)
- Test with very small amounts (1 satoshi, 100 sats)
- Verify proper decimal formatting in notifications
- Test Norwegian vs English number formatting

### 8. Notification Failure Scenarios
**Purpose**: Test resilience of notification system
- Simulate ntfy.sh service unavailable
- Simulate Twilio SMS delivery failure
- Simulate email provider (Resend) failure
- Verify transactions still process correctly despite notification failures
- Check retry logic and error logging

## Implementation Priority
1. **High Priority**: Multiple UTXOs consolidation, self-send handling, wallet recovery
2. **Medium Priority**: Batch transactions, large amount formatting, notification failures  
3. **Low Priority**: Double spend detection, unconfirmed chains

## Integration Notes
- All tests should follow the existing `IsolatedTestEnvironment` pattern
- Each test should include proper cleanup and Docker container reset
- Tests should verify both database state and notification generation
- Consider adding performance benchmarks for complex scenarios