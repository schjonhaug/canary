# Blockchain Reorganization Detection Implementation

## Status: Postponed
We've decided to wait for BDK to handle reorgs natively rather than implementing our own workaround.

## Problem Discovery

### BDK's Anchor Persistence Issue
During testing, we discovered that BDK (Bitcoin Development Kit) has a fundamental issue with how it handles blockchain reorganizations:

1. **BDK stores transaction anchors in its SQLite database**
2. **These anchors persist even after a blockchain reorganization**
3. **Even after a full rescan, BDK continues to report transactions as confirmed at blocks that no longer exist**

Example from our testing:
```
[07y2fln3] DEBUG: BDK reports tx b030cae6 - confirmed: true, height: Some(103), 
chain_position: Confirmed { anchor: ConfirmationBlockTime { block_id: BlockId { 
height: 103, hash: 473692e3... }, confirmation_time: 1757506645 }, transitively: None }
```
Even after block 103 was invalidated and the chain height was 102, BDK still reported the transaction as confirmed at the non-existent block.

### Related BDK Issues
- **Issue #1125**: Transactions can get "stuck" as unconfirmed even after they're actually confirmed
- **PR #1535**: Added test coverage for reorgs but didn't fix the underlying issue

## Our Implementation Approach

### 1. Reorg Detection (in sync.rs)
```rust
async fn detect_reorg(&self, electrum_client: &ElectrumClient) -> Result<bool> {
    let current_height = electrum_client.get_current_block_height()?;
    let current_header = electrum_client.get_block_header(current_height)?;
    let stored_header = self.metadata_db.get_current_block_header().await?;
    
    // Check 1: Height decreased
    if current_height < stored_header.height {
        println!("⚠️ REORG DETECTED: Block height decreased from {} to {}", 
                 stored_header.height, current_height);
        return Ok(true);
    }
    
    // Check 2: Same height but different hash
    if current_height == stored_header.height && current_header.hash != stored_header.hash {
        println!("⚠️ REORG DETECTED: Block at height {} has different hash", current_height);
        return Ok(true);
    }
    
    Ok(false)
}
```

### 2. Anchor Validation Workaround (in sync.rs)
```rust
// WORKAROUND: Check if BDK thinks the transaction is confirmed but at an invalid height
// This happens after a reorg when BDK's persistent anchors are stale
if is_confirmed && electrum_client.is_some() {
    if let bdk_wallet::chain::ChainPosition::Confirmed { anchor, .. } = &tx_item.chain_position {
        let anchor_height = anchor.block_id.height;
        let current_height = electrum_client.unwrap().get_current_block_height()?;
        
        // Check if the anchor height is beyond the current chain height
        if anchor_height > current_height {
            debug!(
                "[{}] Transaction {} anchored at height {} but chain is at height {} - treating as pending",
                wallet_checksum, &txid[..8], anchor_height, current_height
            );
            is_confirmed = false;
            block_height = None;
        } else {
            // Also check if the block hash at that height matches
            match electrum_client.unwrap().get_block_header(anchor_height) {
                Ok(current_block) => {
                    let anchor_hash = anchor.block_id.hash.to_string();
                    if current_block.hash != anchor_hash {
                        debug!(
                            "[{}] Transaction {} anchored to block {} with hash {} but current block has hash {} - treating as pending",
                            wallet_checksum, &txid[..8], anchor_height, &anchor_hash[..8], &current_block.hash[..8]
                        );
                        is_confirmed = false;
                        block_height = None;
                    }
                }
                Err(e) => {
                    warn!("[{}] Failed to get block header at height {}: {:?}", wallet_checksum, anchor_height, e);
                    // If we can't verify, treat as pending to be safe
                    is_confirmed = false;
                    block_height = None;
                }
            }
        }
    }
}
```

### 3. Database Schema Changes (migration 008)
```sql
-- Add block hash to track specific blocks
ALTER TABLE current_block_header ADD COLUMN block_hash TEXT;

-- Add 'dropped' status for transactions that disappear from mempool
-- Updated CHECK constraint to include 'dropped'
-- transaction_status CHECK (transaction_status IN ('pending', 'confirmed', 'replaced', 'dropped'))
```

### 4. Electrum Client Updates (electrum.rs)
```rust
pub struct BlockHeader {
    pub height: u32,
    pub timestamp: u64,
    pub hash: String,  // Added to track specific blocks
}

pub fn get_block_header(&self, height: u32) -> Result<BlockHeader> {
    let header = self.client.block_header(height)?;
    Ok(BlockHeader {
        height,
        timestamp: header.time as u64,
        hash: header.block_hash().to_string(),
    })
}

// Added option to skip tx cache during full scan for fresh state
pub fn full_scan_wallet_with_options(
    &self,
    wallet: &mut PersistedWallet<Connection>,
    custom_stop_gap: Option<usize>,
    skip_tx_cache: bool,
) -> Result<()> {
    // Implementation that optionally skips cache population
}
```

### 5. Metadata Updates (metadata.rs)
```rust
// Store block hash when updating current block header
pub async fn upsert_current_block_header(
    &self,
    height: u32,
    timestamp: u64,
    hash: &str,
) -> Result<()> {
    // Store hash along with height and timestamp
}

// Mark transaction as dropped (not used but added for completeness)
pub async fn update_transaction_to_dropped(
    &self,
    wallet_checksum: &str,
    txid: &str,
) -> Result<bool> {
    // Update transaction status to 'dropped'
}
```

## System Test Code

```rust
// File: system_tests/reorg_detection.rs
use canary::test_utils::docker_environment::IsolatedTestEnvironment;

#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_single_block_reorg_confirmed_to_pending() {
    let env = IsolatedTestEnvironment::new()
        .await
        .expect("Failed to create test environment");

    // Step 1: Send transaction from Alice to Bob
    let txid = env.send_transaction("alice", "bob", 0.1, false, None)
        .await
        .expect("Failed to send transaction");

    // Step 2: Mine a block to confirm the transaction
    env.mine_blocks(1).await.expect("Failed to mine block");
    env.sync_wallets().await.expect("Failed to sync");
    
    // Verify transaction is confirmed
    let alice_txs = env.get_wallet_transactions("alice").await.unwrap();
    let tx = alice_txs.iter().find(|t| t.txid == txid).unwrap();
    assert_eq!(tx.transaction_status, "confirmed");

    // Step 3: Invalidate the block to trigger reorg
    let block_hash = env.get_block_hash(103).await.unwrap();
    env.invalidate_block(&block_hash).await.unwrap();
    
    // Step 4: Sync and verify transaction is back to pending
    env.sync_wallets().await.expect("Failed to sync after reorg");
    
    let alice_txs = env.get_wallet_transactions("alice").await.unwrap();
    let tx = alice_txs.iter().find(|t| t.txid == txid).unwrap();
    assert_eq!(tx.transaction_status, "pending", 
               "Transaction should be pending after reorg");

    // Step 5: Re-mine and verify confirmation
    env.mine_blocks(1).await.expect("Failed to mine block");
    env.sync_wallets().await.expect("Failed to sync");
    
    let alice_txs = env.get_wallet_transactions("alice").await.unwrap();
    let tx = alice_txs.iter().find(|t| t.txid == txid).unwrap();
    assert_eq!(tx.transaction_status, "confirmed",
               "Transaction should be confirmed again");
}

#[tokio::test]
#[ignore] // System test - requires Docker
async fn test_multi_block_reorg() {
    // Test for deeper reorganizations
}

#[tokio::test]
#[ignore] // System test - requires Docker  
async fn test_mempool_purge_pending_to_dropped() {
    // Test for transactions that disappear from mempool
}
```

## Why We're Waiting

### Reasons for Postponement:
1. **Reorgs are rare on mainnet** - Most users will never experience one
2. **BDK team is aware** - Issue #1125 shows they know about the problem
3. **Complexity vs. benefit** - The workaround adds significant complexity for an edge case
4. **Temporary impact** - Even without handling, the impact is temporary until next sync

### What Happens Without Reorg Handling:
- Transactions may show incorrect confirmation status temporarily
- Balance calculations might be off until next full sync
- Notifications might be sent for confirmations that aren't real
- BUT: System will self-correct on next transaction or full scan

### Future Action:
When BDK provides native reorg support:
1. Remove our workarounds
2. Update to latest BDK version
3. Test with the system tests we created
4. Ensure proper notification handling for all state transitions

## Lessons Learned

1. **BDK's persistence model** has limitations with blockchain reorgs
2. **Anchor validation** is crucial - can't trust stored anchors blindly
3. **Comprehensive validation** after reorg requires checking ALL transactions, not just what BDK returns
4. **Test infrastructure** (Docker-based regtest) was invaluable for discovering and testing this issue

## References
- BDK Issue #1125: https://github.com/bitcoindevkit/bdk/issues/1125
- BDK PR #1535: https://github.com/bitcoindevkit/bdk/pull/1535 (test coverage for reorgs)
- Bitcoin Core invalidateblock RPC: Used to trigger reorgs in testing
- Fulcrum/Electrum: Correctly handles reorgs, issue is in BDK's persistent storage