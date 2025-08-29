# Fix Wallet Drain Notification Issue

## Problem Identified

The l6refrr3 wallet isn't sending notifications because it encounters an edge case in the transaction detection logic where the entire wallet balance is spent without any change being returned.

## Root Cause

The send detection logic in `/opt/canary/backend/src/wallet.rs` (lines 1513-1580) only handles three specific cases:
1. Spending from confirmed with change returned to trusted pending
2. Spending from both trusted pending and confirmed  
3. Spending from only trusted pending

The problematic scenario doesn't match any of these because:
- Confirmed balance: 0.00067777 BTC → 0 (decreased)
- Trusted pending: 0 → 0 (no change)
- Total: 0.00067777 BTC → 0 (decreased)

This is a "complete wallet drain" scenario where no change is returned.

## Solution

Add a new detection case for when the wallet is completely emptied:

```rust
// Case 4: Spending entire balance without change (wallet drain)
else if !trusted_pending_increase && !trusted_pending_decrease && 
        confirmed_decrease && total_decrease {
    let total_spent = confirmed_before.to_sat() - confirmed_after.to_sat();
    
    let message = format!(
        "📤 Sending {:.8} BTC", 
        total_spent as f64 / 100_000_000.0
    );
    println!("[{}] {}", wallet_checksum, message);
    
    // Get timestamp of the new sending transaction
    let send_timestamp = Self::get_new_send_transaction_timestamp(
        wallet, 
        &unconfirmed_sends_before
    );
    
    // Insert sending event to database and broadcast
    if let Err(e) = Self::insert_and_broadcast_event_helper(
        metadata_db,
        event_sender,
        &EventInsert {
            wallet_checksum: wallet_checksum.to_string(),
            event_type: EventType::Send,
            amount_sats: total_spent as i64,
            is_confirmed: false,
            is_rbf: false,
            is_cpfp: false,
            balance_total: Some(total_after.to_sat() as i64),
            transaction_time: send_timestamp,
        },
    )
    .await
    {
        eprintln!("Failed to insert sending event: {}", e);
    }
}
```

## Implementation Location

This code should be added in `/opt/canary/backend/src/wallet.rs` around line 1580, after the existing Case 3 logic and before the closing bracket of the send detection section.

## Testing

After implementation, test by:
1. Creating a wallet with a small balance
2. Sending the entire balance without change
3. Verifying that notifications are sent and transactions appear in the web interface

## Expected Behavior

When implemented, the system will:
- Detect when a wallet's entire balance is spent
- Generate appropriate "Sending" notifications
- Display the transaction in the wallet's transaction history
- Properly track the balance change to zero