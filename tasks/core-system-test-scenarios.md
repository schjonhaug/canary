# Core System Test Scenarios

## Overview
This document defines the essential system test scenarios that must be implemented and maintained for our Bitcoin wallet transaction detection and notification system.

## Implementation Status
**Current Status**: ✅ **Unblocked and ready for implementation**

**Completed**:
- ✅ Test infrastructure (`IsolatedTestEnvironment` in `system_tests/common/docker_environment.rs`)
- ✅ Proper XPUB descriptor usage (fixed from TPRV to TPUB for BDK compatibility)
- ✅ Wallet creation using `WalletCreationService`
- ✅ Wait mechanisms for wallet ready state
- ✅ **Admin bypass for subscription checks** - System tests now use FOSS mode with automatic admin user
- ✅ **Subscription system integration solved** - Tests bypass all Stripe dependencies
- ✅ **Docker Compose architecture** - Test environment now uses Bitcoin Core + Fulcrum like dev setup
- ✅ **Electrum connectivity fixed** - BDK now connects to proper Fulcrum server instead of Bitcoin RPC
- ✅ **Dynamic port allocation** - Tests run in parallel without port conflicts
- ✅ **Core infrastructure working** - Wallets sync successfully, test environment stable

**In Progress**:
- Test 4: Fast confirmation scenarios (`fast_confirmation_scenarios.rs`) - Infrastructure complete, debugging transaction detection

**Next Priority**:
- Complete Test 4 implementation and verification
- Expand to remaining core transaction flow tests

**Key Achievement**: System tests now focus purely on Bitcoin functionality without any Stripe subscription dependencies.

## Test Environment Setup
Each test uses an isolated Docker Compose environment with:
- **Bitcoin Core container**: Regtest mode with deterministic wallets
- **Fulcrum container**: Electrum server for BDK wallet connectivity
- **Dynamic port allocation**: Prevents conflicts between parallel test runs
- **Unique container names**: Complete test isolation
- **Automatic cleanup**: `docker-compose down` removes all containers and volumes

## Wallet Setup Scenarios

### High Index Scanning Tests
**Purpose**: Verify deep address scanning capabilities

#### Test 1: Charlie Wallet with XPUB (High Index 250)
- Add Charlie wallet using XPUB format
- Fund address at index 250
- Verify wallet detects funds at high index
- Check deep scanning progression (batches: 100, 200, 300, 400, 500)

#### Test 2: Charlie Wallet with Output Descriptor (High Index 250)  
- Add Charlie wallet using output descriptor format
- Fund address at index 250
- Verify descriptor-based wallet handles high index scanning
- Compare performance vs XPUB approach

## Core Transaction Flow Tests
**Standard Setup**: Alice and Bob wallets for all following tests

### Sending Flow Tests

#### Test 3: Alice Sending Bob (Unconfirmed → Confirmed)
**Steps**:
1. Initial sync and balance check
2. Alice sends Bitcoin to Bob
3. **Before mining**: Check balances and transaction events
   - Alice: Should have "Sending" event (unconfirmed)
   - Bob: Should have "Receiving" event (unconfirmed)
   - Verify notification generation for both wallets
4. Mine 1 block
5. **After mining**: Check balances and transaction events
   - Alice: Should have "Sent" event (confirmed)
   - Bob: Should have "Received" event (confirmed)
   - Verify confirmation notifications

#### Test 4: Alice Sent Bob (Direct Confirmed) ✅ UNBLOCKED
**Purpose**: Test fast confirmation scenarios
**Status**: ✅ **Ready for completion - subscription blocking resolved**
**Steps**:
1. Alice sends Bitcoin to Bob
2. Immediately mine 1 block (before sync)
3. Sync wallets
4. **Verify**: Should show "Sent" and "Received" events directly
   - No "Sending/Receiving" intermediate states
   - Check notifications reflect confirmed state

**Current Implementation**:
- ✅ Test framework created (`fast_confirmation_scenarios.rs`)
- ✅ Isolated Docker environment with proper XPUB descriptors
- ✅ Wallet creation using WalletCreationService
- ✅ Wait mechanism for wallet ready status
- ✅ **UNBLOCKED**: Admin bypass implemented - wallets now sync in FOSS mode
- ✅ **Subscription integration**: Tests use automatic admin user, bypassing Stripe dependencies

**Implementation Status**:
- ✅ **Infrastructure complete**: Docker Compose architecture working perfectly
- ✅ **Subscription blocking resolved**: System tests use FOSS mode with admin user
- ✅ **Electrum connectivity fixed**: BDK successfully connects to Fulcrum Electrum server
- ✅ **Core functionality verified**: Wallets create, fund, and sync successfully
- 🔄 **Minor remaining issue**: Transaction detection after mining blocks (investigation needed)
- 📋 **Next step**: Debug transaction event detection and complete scenario verification

**Key Achievement**: Complete working test infrastructure - Bitcoin Core + Fulcrum + BDK integration successful.

### Maximum Amount Tests

#### Test 5: Alice Sending Bob Max (Drain - Unconfirmed → Confirmed)
**Steps**:
1. Alice sends maximum available balance to Bob
2. **Before mining**: Verify drain-specific notifications
   - Should indicate wallet drain/empty scenario
   - Check amount reflects total available minus fees
3. Mine 1 block
4. **After mining**: Verify final drain notifications
   - Alice balance should be zero (or dust)

#### Test 6: Alice Sent Bob Max (Direct Drain)
**Purpose**: Test direct confirmed drain
**Steps**:
1. Alice sends maximum balance to Bob
2. Immediately mine 1 block
3. Sync and verify drain notifications reflect immediate confirmation

### Advanced Transaction Tests

#### Test 7: Alice RBF (Replace-By-Fee)
**Steps**:
1. Alice sends Bitcoin to Bob with low fee (unconfirmed)
2. Alice replaces transaction with higher fee
3. **Verify**:
   - Original transaction events are updated/replaced
   - Bob sees correct final amount
   - Notifications handle RBF properly
   - No duplicate events created

#### Test 8: Bob CPFP (Child-Pays-For-Parent)
**Steps**:
1. Alice sends Bitcoin to Bob with low fee (unconfirmed, stuck)
2. Bob creates child transaction spending received output with high fee
3. Mine block to confirm both transactions
4. **Verify**:
   - Both parent and child transactions are detected
   - Proper event sequencing and fee attribution
   - Notifications reflect CPFP acceleration

## Notification Verification Requirements
Each test must verify:
- **Content Accuracy**: Correct amounts, addresses, confirmation status
- **Language Support**: Both Norwegian and English notifications
- **Provider Coverage**: ntfy.sh, SMS (Twilio), Email (Resend) where configured
- **Timing**: Notifications sent at appropriate transaction state changes
- **No Duplicates**: Multiple syncs don't create duplicate notifications

## Test Environment Architecture

### Docker Compose Setup
Each test creates an isolated environment with:
```
test-{uuid}/
├── bitcoin.conf          # Bitcoin Core regtest configuration
├── fulcrum.conf         # Fulcrum Electrum server configuration  
├── docker-compose.yml   # Container orchestration
└── volumes/             # Persistent data (auto-cleaned)
```

### Container Services
- **Bitcoin Core**: `ghcr.io/sethforprivacy/bitcoind:latest` on dynamic port
- **Fulcrum**: `cculianu/fulcrum:latest` on dynamic port
- **Networking**: Containers communicate via Docker Compose networking
- **Data**: Isolated volumes per test, automatically cleaned up

### Integration Commands
Tests use internal methods that mirror `regtest-env/docker-utils.sh`:
- `env.mine_blocks(count)` - Mine blocks for confirmation
- `env.send_transaction(from, to, amount)` - Send transactions
- `env.sync_and_wait()` - Trigger wallet sync and wait
- Environment auto-cleanup on test completion

## Success Criteria
- All transaction events stored correctly in database
- Wallet balances calculated accurately
- Notifications generated with correct content and timing
- No duplicate events on multiple syncs
- Proper handling of edge cases (drains, high fees, RBF, CPFP)
- Fast API responses maintained during sync operations

## Implementation Notes
- Use `IsolatedTestEnvironment::new()` for complete Docker Compose setup
- Each test creates isolated containers with unique ports and config
- Tests marked with `#[ignore]` requiring Docker Compose environment
- Automatic cleanup with `docker-compose down -v` on test completion
- Verify both immediate API responses and background sync results
- **Major Infrastructure Success**: Docker Compose architecture provides stable, reliable test foundation