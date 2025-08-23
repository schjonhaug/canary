# Fix Mainnet Performance Issues

## Problem Description
When running the backend on mainnet, login and page navigation is extremely slow. The issue does not occur on regtest or testnet networks.

### Commands Used
```bash
# Backend
CANARY_NETWORK=mainnet CANARY_ELECTRUM_URL=ssl://electrum.blockstream.info:50002 cargo run

# Frontend
pnpm dev
```

## Root Cause Analysis

### 1. Startup Wallet Loading
- When the backend starts, `load_all_wallets()` loads ALL ready wallets from disk into memory
- On mainnet with many wallets or complex transaction histories, this is slow

### 2. Redundant Wallet Loading During Sync
- During the periodic sync cycle (line 1452 in `wallet.rs`), the system calls `load_all_wallets()` AGAIN
- This reloads all wallets from disk even if they're already in memory
- This happens every sync interval (1-10 minutes depending on tier)

### 3. Why Mainnet is Slower
- **Wallet Complexity**: Mainnet wallets have more transaction history and address usage
- **SQLite File Size**: Larger wallet databases take longer to load from disk
- **BDK Loading**: Loading wallets with extensive history requires more processing

## Solution Plan

### 1. Optimize Wallet Loading Strategy
- Modify `sync_wallets_due_for_sync()` to check if wallets are already loaded before calling `load_all_wallets()`
- Only load wallets that aren't already in memory
- Add wallet checksum tracking to avoid redundant loads

### 2. Implement Lazy Loading for Wallets
- Don't load all wallets at startup - only load them when needed
- Load wallets on-demand when they're accessed via API
- Keep frequently accessed wallets in memory cache
- Implement LRU cache to manage memory usage

### 3. Add Performance Logging
- Add timing measurements to wallet loading operations
- Log slow operations to identify bottlenecks
- Track wallet loading times per network

## Implementation Details

### Files to Modify
- `backend/src/wallet.rs`
  - Optimize `load_all_wallets()` method
  - Fix `sync_wallets_due_for_sync()` to avoid redundant loading
  - Add lazy loading logic for wallet access
  - Implement wallet cache management

### Quick Fix (Immediate)
The simplest immediate fix is to prevent redundant wallet loading in the sync cycle:

```rust
// In sync_wallets_due_for_sync() around line 1452
// Replace:
if let Err(e) = self.load_all_wallets().await {
    eprintln!("Failed to load wallets: {}", e);
    return Ok(());
}

// With:
// Only load wallets if we don't have any loaded yet
if self.wallets.is_empty() {
    if let Err(e) = self.load_all_wallets().await {
        eprintln!("Failed to load wallets: {}", e);
        return Ok(());
    }
}
```

### Long-term Solution
Implement proper lazy loading with an LRU cache:

1. Create a `WalletCache` struct with max capacity
2. Load wallets only when accessed via API
3. Track last access time for each wallet
4. Evict least recently used wallets when cache is full
5. Keep wallet metadata in memory, but wallet data on disk until needed

## Testing Plan
1. Test on mainnet with multiple wallets
2. Measure startup time before and after changes
3. Measure API response times for wallet list and detail endpoints
4. Monitor memory usage with different cache sizes
5. Test wallet sync performance

## Notes
- Frontend configuration is correct - no changes needed there
- The issue is entirely backend-related
- This affects all API endpoints that trigger wallet loading