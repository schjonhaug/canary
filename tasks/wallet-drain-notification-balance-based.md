# Wallet Drain Notification - Balance-Based Approach

## Overview
Implement a simple, reliable wallet drain notification system that detects when a wallet balance reaches zero using BDK's balance directly, rather than complex transaction-level detection.

## Problem Statement
The previous transaction-based drain detection approach (documented in the now-deleted `fix-wallet-drain-notification.md`) was overly complex and prone to edge cases. It tried to detect drains by analyzing transaction patterns, which led to:
- Complex conditional logic with multiple edge cases
- Difficulty handling fast confirmations vs mempool transactions
- Conflicts with regular transaction event detection
- Missed drain events in certain scenarios

## Proposed Solution
Use BDK wallet balance directly to detect when a wallet has been drained:
1. Track wallet balance before and after sync
2. If balance goes from >0 to 0, trigger a drain notification
3. Provide user preference to enable/disable drain notifications
4. Keep drain notifications separate from transaction events

## Implementation Plan

### 1. Database Schema Changes
Add user preference for drain notifications:
```sql
-- Migration 010_add_drain_notification_preference.sql
ALTER TABLE users ADD COLUMN enable_drain_notifications BOOLEAN DEFAULT TRUE;

-- Optional: Track wallet drain events separately
CREATE TABLE wallet_drain_events (
    id TEXT PRIMARY KEY, -- UUIDv4
    wallet_checksum TEXT NOT NULL,
    previous_balance INTEGER NOT NULL,
    drained_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    notification_sent BOOLEAN DEFAULT FALSE,
    FOREIGN KEY (wallet_checksum) REFERENCES wallets (checksum) ON DELETE CASCADE
);
```

### 2. Sync Service Modifications
In `backend/src/sync.rs`, modify the `sync_wallet_by_checksum` function:

```rust
// Around line 166-177 where balance is updated
let balance_update_start = Instant::now();
let previous_balance = self.metadata_db
    .get_wallet_balance(wallet_checksum)
    .await?;
let current_balance = wallet.balance().total();

// Detect wallet drain
if previous_balance > 0 && current_balance.to_sat() == 0 {
    // Check if user has drain notifications enabled
    if let Some(user_preferences) = self.metadata_db.get_user_preferences_for_wallet(wallet_checksum).await? {
        if user_preferences.enable_drain_notifications {
            self.send_wallet_drain_notification(wallet_checksum, previous_balance).await?;
        }
    }
}

// Update balance as normal
self.metadata_db
    .update_wallet_balance_by_checksum(wallet_checksum, current_balance.to_sat() as i64)
    .await?;
```

### 3. Notification System Integration
Create a new notification type for wallet drains:

```rust
pub enum WalletNotification {
    Drained {
        wallet_checksum: String,
        wallet_name: String,
        previous_balance: i64,
    }
}
```

### 4. API Endpoints
Update user preferences endpoints to include drain notification setting:

```rust
// In backend/src/api.rs
#[derive(Serialize, Deserialize)]
pub struct UserPreferences {
    preferred_fiat_currency: String,
    enable_drain_notifications: bool, // New field
}
```

### 5. Frontend Integration
Add UI elements for:
- Toggle in user settings to enable/disable drain notifications
- Special notification display for wallet drain events
- Clear messaging that wallet balance has reached zero

## Benefits of This Approach

1. **Simplicity**: Direct balance comparison is straightforward and reliable
2. **No Transaction Conflicts**: Drain detection doesn't interfere with transaction event logic
3. **User Control**: Users can opt-in/out of drain notifications
4. **Clear Separation**: Wallet state changes are tracked separately from individual transactions
5. **Reliability**: BDK's balance is the authoritative source of truth

## Testing Scenarios

1. **Normal Drain**: Send entire wallet balance in single transaction
2. **Partial Drains**: Multiple transactions that eventually drain wallet
3. **Fast Confirmation**: Transaction mined directly that drains wallet
4. **RBF Drain**: Replace transaction that drains wallet
5. **User Preference**: Verify drain notifications respect user settings

## Migration Path
1. Add database migration for user preferences
2. Default existing users to `enable_drain_notifications = true`
3. Implement balance-based detection in sync service
4. Add frontend toggle for user preference
5. Remove old transaction-based drain detection code (if any remains)

## Success Criteria
- Wallet drains are reliably detected when balance reaches exactly 0
- Users receive clear notifications about wallet drains
- Users can control whether they receive drain notifications
- No interference with regular transaction notifications
- Clean separation between wallet state and transaction events