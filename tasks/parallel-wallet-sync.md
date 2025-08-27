# Implement True Parallel Wallet Syncing

## Current Status
The wallet sync operation is **completely serial**. The `sync_wallet_by_checksum` method requires mutable access to `self`, preventing concurrent execution.

## Problem
- Wallets are synced sequentially in a for loop at `src/wallet.rs:1769-1786`
- Each wallet sync blocks the next one
- With many wallets, sync cycles take longer than necessary
- Code at `src/wallet.rs:1763` has a TODO comment: "TODO: Implement true parallel syncing with futures"
- The method `sync_single_wallet_by_checksum` at line 1803 requires `&mut self`

## Solution Architecture

### 1. Extract Sync Logic to Static Functions
**Goal**: Remove the need for `&mut self` in sync operations

- Move wallet sync operations out of methods requiring mutable self
- Create standalone async functions that take immutable references or cloned data
- Use `Arc<Mutex<>>` for any shared state that needs mutation
- Pass required dependencies (Electrum client, metadata DB) as parameters

### 2. Refactor sync_wallets_due_for_sync()
**Goal**: Enable true parallel execution

```rust
// Pseudocode for parallel sync
let sync_tasks: Vec<_> = due_wallets
    .into_iter()
    .map(|(wallet_metadata, tier)| {
        let checksum = wallet_metadata.checksum.clone();
        let electrum = self.electrum_client.clone();
        let metadata_db = self.metadata_db.clone();
        
        tokio::spawn(async move {
            sync_wallet_standalone(checksum, electrum, metadata_db).await
        })
    })
    .collect();

// Wait for all syncs to complete
let results = futures::future::join_all(sync_tasks).await;
```

### 3. Implement Concurrency Control
**Goal**: Prevent overwhelming the Electrum server

- Use `tokio::sync::Semaphore` to limit concurrent syncs (e.g., 5-10 wallets)
- Configure based on Electrum server capacity
- Add configuration option for max concurrent syncs

### 4. Update Database Operations
**Goal**: Efficient batch updates after parallel syncs

- Collect all sync results before updating database
- Use database transactions for atomic updates
- Update sync timestamps for all wallets in a single operation
- Handle partial failures gracefully (some wallets fail, others succeed)

## Implementation Steps

1. **Phase 1: Refactor sync logic**
   - Create `sync_wallet_standalone()` function
   - Extract all mutable operations
   - Test with single wallet

2. **Phase 2: Implement parallel execution**
   - Update `sync_wallets_due_for_sync()` to spawn tasks
   - Add semaphore for concurrency control
   - Implement result collection

3. **Phase 3: Optimize database operations**
   - Batch update sync timestamps
   - Add transaction support
   - Improve error handling

4. **Phase 4: Performance tuning**
   - Add metrics for sync performance
   - Tune concurrency limits
   - Add configuration options

## Expected Benefits

- **Performance**: 3-5x speedup for multi-wallet sync operations
- **Scalability**: Better handling of users with many wallets
- **Resource efficiency**: Optimal use of available CPU cores
- **Electrum server load**: Controlled concurrent connections

## Technical Considerations

- **Memory usage**: Each parallel sync holds wallet data in memory
- **Electrum connections**: Need to manage connection pool properly
- **Error handling**: Partial failures shouldn't break entire sync cycle
- **Monitoring**: Add detailed logging for parallel operations

## Success Metrics

- [ ] Wallet sync operations run truly in parallel
- [ ] Sync time reduces proportionally with concurrency level
- [ ] No increase in error rates
- [ ] Memory usage remains reasonable
- [ ] Electrum server doesn't get overwhelmed

## Related Files
- `src/wallet.rs` - Main wallet management logic
- `src/main.rs` - Sync task spawning
- `src/metadata.rs` - Database operations

## Priority
Medium - The current serial sync works but becomes a bottleneck with many wallets or slow Electrum responses.