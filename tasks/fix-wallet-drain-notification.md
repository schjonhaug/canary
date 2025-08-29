# Fix Wallet Drain Notification Issue

## Problem Identified

We found a wallet that isn't sending notifications nor inserting transaction events because it encounters an edge case in the transaction detection logic where the entire wallet balance is spent without any change being returned.


## Backend log:


backend-1  | ✅ Team tier sync completed: 2/2 successful in 36.774775411s
backend-1  | 🔓 Released wallet manager mutex after 36.77649222s (Team tier parallel sync)
backend-1  | 🔓 Released team tier sync mutex after 36.776499539s
backend-1  | 🔒 Block sync task waited 36.776438895s for wallet manager mutex
backend-1  | 🔓 Released block sync mutex after 36.944132667s
backend-1  | 🔓 Released block sync mutex after 170.29375ms
backend-1  | 🔒 Team sync task waited 170.245346ms for wallet manager mutex
backend-1  | 🔄 Starting Team tier parallel sync for 2 wallets
backend-1  | ⚡ Wallet list unchanged in 457.554µs: 4 wallets in memory
backend-1  | ⏱️ Wallet list sync completed in 474.184µs
backend-1  |   ⏱️ Synced v67x36rm in 21.616729415s (no changes)
backend-1  |   ⏱️ Synced wgws4n7n in 10.05331664s (no changes)
backend-1  | ✅ Team tier sync completed: 2/2 successful in 31.670107968s
backend-1  | 🔓 Released wallet manager mutex after 31.67159622s (Team tier parallel sync)
backend-1  | 🔓 Released team tier sync mutex after 31.84185087s
backend-1  | 🔄 Starting Team tier parallel sync for 2 wallets
backend-1  | ⚡ Wallet list unchanged in 620.334µs: 4 wallets in memory
backend-1  | ⏱️ Wallet list sync completed in 644.449µs
backend-1  |   ⏱️ Synced 4afzscrv in 4.261425079s (no changes)
backend-1  |
backend-1  | -------------------------------------------------------------------------------------
backend-1  |  Wallet Enkeltsignatur | Before             | After              | Diff
backend-1  | -------------------------------------------------------------------------------------
backend-1  |        Trusted pending |                    |                    |
backend-1  |    Unconfirmed pending |                    |                    |
backend-1  |              Confirmed |    0.00067777 BTC  |                    |   -0.00067777 BTC
backend-1  | -------------------------------------------------------------------------------------
backend-1  |                  Total |    0.00067777 BTC  |                    |   -0.00067777 BTC
backend-1  | -------------------------------------------------------------------------------------
backend-1  |
backend-1  |   ⏱️ Synced l6refrr3 in 32.608753927s (with changes)
backend-1  | ✅ Team tier sync completed: 2/2 successful in 36.870323862s (with changes)
backend-1  | 🔓 Released wallet manager mutex after 36.871990289s (Team tier parallel sync)
backend-1  | 🔓 Released team tier sync mutex after 36.872076851s
backend-1  | 🔒 Personal sync task waited 36.872061265s for wallet manager mutex
backend-1  | 📊 No Personal tier wallets due for sync
backend-1  | 🔓 Released wallet manager mutex after 729.586µs (Personal tier parallel sync)
backend-1  | 🔓 Released personal tier sync mutex after 36.872857399s
backend-1  | 🔒 Block sync task waited 36.872877257s for wallet manager mutex
backend-1  | 🔓 Released block sync mutex after 37.039539594s
backend-1  | 2025-08-29T19:38:46.546726Z  INFO canary::api: get_current_block_header completed in 1.137113ms
backend-1  | ⚡ Non-blocking wallet list served in 156.865857ms
backend-1  | ⚡ Non-blocking wallet detail served in 199.742027ms
backend-1  | ⚡ Non-blocking wallet list served in 199.002717ms
backend-1  | ⚡ Non-blocking wallet detail served in 202.739129ms
backend-1  | ⚡ Non-blocking wallet list served in 176.34152ms
backend-1  | 🔓 Released block sync mutex after 168.078997ms
backend-1  | 🔒 Team sync task waited 168.099348ms for wallet manager mutex
backend-1  | 🔄 Starting Team tier parallel sync for 2 wallets
backend-1  | ⚡ Wallet list unchanged in 902.938µs: 4 wallets in memory
backend-1  | ⏱️ Wallet list sync completed in 927.824µs
backend-1  | ⚡ Non-blocking wallet detail served in 182.855216ms
backend-1  | ⚡ Non-blocking wallet list served in 163.264333ms
backend-1  |
backend-1  | -------------------------------------------------------------------------------------
backend-1  |   Wallet Multisignatur | Before             | After              | Diff
backend-1  | -------------------------------------------------------------------------------------
backend-1  |        Trusted pending |                    |                    |
backend-1  |    Unconfirmed pending |                    |    0.00067655 BTC  |   +0.00067655 BTC
backend-1  |              Confirmed |    1.51904691 BTC  |    1.51904691 BTC  |
backend-1  | -------------------------------------------------------------------------------------
backend-1  |                  Total |    1.51904691 BTC  |    1.51972346 BTC  |   +0.00067655 BTC
backend-1  | -------------------------------------------------------------------------------------
backend-1  | [v67x36rm] 📥 Receiving 0.00067655 BTC
backend-1  |
backend-1  |   ⏱️ Synced v67x36rm in 21.901002599s (with changes)
backend-1  | 2025-08-29T19:39:46.495894Z  INFO canary::api: get_current_block_header completed in 857.35µs
backend-1  |   ⏱️ Synced wgws4n7n in 11.097335798s (no changes)
backend-1  | ✅ Team tier sync completed: 2/2 successful in 32.998430988s (with changes)
backend-1  | 🔓 Released wallet manager mutex after 33.001300721s (Team tier parallel sync)
backend-1  | 🔓 Released team tier sync mutex after 33.169421873s
backend-1  | 🔔 Notified 2 contacts for Multisignatur: Transaction (1×twilio, 2×email, 1×ntfy)
backend-1  | ⚡ Non-blocking wallet detail served in 179.49941ms
backend-1  | ⚡ Non-blocking wallet list served in 162.183271ms
backend-1  | ⚡ Non-blocking wallet detail served in 201.235724ms
backend-1  | ⚡ Non-blocking wallet list served in 166.658628ms
backend-1  | ⚡ Non-blocking wallet detail served in 154.391992ms
backend-1  | ⚡ Non-blocking wallet list served in 197.499959ms
backend-1  | ⚡ Non-blocking wallet detail served in 148.181798ms
backend-1  | 2025-08-29T19:40:46.488876Z  INFO canary::api: get_current_block_header completed in 526.784µs
backend-1  | ⚡ Non-blocking wallet list served in 172.384861ms
backend-1  | 🔄 Starting Team tier parallel sync for 2 wallets
backend-1  | ⚡ Wallet list unchanged in 1.027748ms: 4 wallets in memory
backend-1  | ⏱️ Wallet list sync completed in 1.137046ms
backend-1  |   ⏱️ Synced 4afzscrv in 4.267984567s (no changes)
backend-1  | ⚡ Non-blocking wallet detail served in 143.498459ms
backend-1  | 2025-08-29T19:41:46.480897Z  INFO canary::api: get_current_block_header completed in 264.701µs
backend-1  |   ⏱️ Synced l6refrr3 in 32.440053438s (no changes)
backend-1  | ✅ Team tier sync completed: 2/2 successful in 36.708208926s
backend-1  | 🔓 Released wallet manager mutex after 36.710146835s (Team tier parallel sync)
backend-1  | 🔓 Released team tier sync mutex after 36.710153553s
backend-1  | 🔒 Block sync task waited 36.710095009s for wallet manager mutex
backend-1  | 🔓 Released block sync mutex after 36.876829998s
backend-1  | ⚡ Non-blocking wallet detail served in 172.796312ms
backend-1  | 2025-08-29T19:42:46.479962Z  INFO canary::api: get_current_block_header completed in 315.843µs
backend-1  | 🔓 Released block sync mutex after 170.688956ms
backend-1  | 🔒 Team sync task waited 170.750192ms for wallet manager mutex
backend-1  | 🔄 Starting Team tier parallel sync for 2 wallets
backend-1  | ⚡ Wallet list unchanged in 1.09626ms: 4 wallets in memory
backend-1  | ⏱️ Wallet list sync completed in 1.137717ms
backend-1  | ⚡ Non-blocking wallet detail served in 146.972241ms
backend-1  |   ⏱️ Synced v67x36rm in 21.550222071s (no changes)
backend-1  | 2025-08-29T19:43:46.473833Z  INFO canary::api: get_current_block_header completed in 421.072µs
backend-1  |   ⏱️ Synced wgws4n7n in 10.044630903s (no changes)
backend-1  | ✅ Team tier sync completed: 2/2 successful in 31.59491385s
backend-1  | 🔓 Released wallet manager mutex after 31.598906092s (Team tier parallel sync)
backend-1  | 🔓 Released team tier sync mutex after 31.769688016s
backend-1  | ⚡ Non-blocking wallet detail served in 163.136995ms
backend-1  | 2025-08-29T19:44:46.584129Z  INFO canary::api: get_current_block_header completed in 384.901µs
backend-1  | 🔄 Starting Team tier parallel sync for 2 wallets
backend-1  | ⚡ Wallet list unchanged in 1.95515ms: 4 wallets in memory
backend-1  | ⏱️ Wallet list sync completed in 2.020041ms
backend-1  |   ⏱️ Synced 4afzscrv in 4.257410666s (no changes)
backend-1  | ⚡ Non-blocking wallet detail served in 172.435382ms
backend-1  | 2025-08-29T19:45:47.229644Z  INFO canary::api: get_current_block_header completed in 361.267µs
backend-1  | ⚡ Non-blocking wallet list served in 177.853366ms
backend-1  |   ⏱️ Synced l6refrr3 in 32.756122234s (no changes)
backend-1  | ✅ Team tier sync completed: 2/2 successful in 37.013598936s
backend-1  | 🔓 Released wallet manager mutex after 37.019056691s (Team tier parallel sync)
backend-1  | 🔓 Released team tier sync mutex after 37.019090069s
backend-1  | 🔒 Block sync task waited 37.01787598s for wallet manager mutex
backend-1  | 📦 New block header: height=912297 (was 912296)
backend-1  | 🔓 Released block sync mutex after 37.356885427s
backend-1  | 2025-08-29T19:46:47.340961Z  INFO canary::api: get_current_block_header completed in 2.185639ms
backend-1  | ⚡ Non-blocking wallet list served in 180.27367ms
backend-1  | 🔓 Released block sync mutex after 168.847251ms
backend-1  | 🔒 Personal sync task waited 168.991328ms for wallet manager mutex
backend-1  | 📊 No Personal tier wallets due for sync
backend-1  | 🔓 Released wallet manager mutex after 4.937508ms (Personal tier parallel sync)
backend-1  | 🔓 Released personal tier sync mutex after 173.967537ms
backend-1  | 🔒 Team sync task waited 173.949961ms for wallet manager mutex
backend-1  | 🔄 Starting Team tier parallel sync for 2 wallets
backend-1  | ⚡ Wallet list unchanged in 2.295986ms: 4 wallets in memory
backend-1  | ⏱️ Wallet list sync completed in 2.372952ms
backend-1  |
backend-1  | -------------------------------------------------------------------------------------
backend-1  |   Wallet Multisignatur | Before             | After              | Diff
backend-1  | -------------------------------------------------------------------------------------
backend-1  |        Trusted pending |                    |                    |
backend-1  |    Unconfirmed pending |    0.00067655 BTC  |                    |   -0.00067655 BTC
backend-1  |              Confirmed |    1.51904691 BTC  |    1.51972346 BTC  |   +0.00067655 BTC
backend-1  | -------------------------------------------------------------------------------------
backend-1  |                  Total |    1.51972346 BTC  |    1.51972346 BTC  |
backend-1  | -------------------------------------------------------------------------------------
backend-1  | [v67x36rm] ✅ Received confirmed: 0.00067655 BTC
backend-1  |
backend-1  |   ⏱️ Synced v67x36rm in 21.923169288s (with changes)
backend-1  | ⚡ Non-blocking wallet list served in 169.65539ms
backend-1  |   ⏱️ Synced wgws4n7n in 10.035936603s (no changes)
backend-1  | ✅ Team tier sync completed: 2/2 successful in 31.959167777s (with changes)
backend-1  | 🔓 Released wallet manager mutex after 31.962683706s (Team tier parallel sync)
backend-1  | 🔓 Released team tier sync mutex after 32.136643343s
backend-1  | 🔔 Notified 2 contacts for Multisignatur: Transaction (1×twilio, 1×ntfy, 2×email)
backend-1  | 2025-08-29T19:48:11.804460Z  INFO canary::api: get_current_block_header completed in 542.43µs
backend-1  | ⚡ Non-blocking wallet list served in 175.651192ms
backend-1  | 🔄 Starting Team tier parallel sync for 2 wallets
backend-1  | ⚡ Wallet list unchanged in 908.024µs: 4 wallets in memory
backend-1  | ⏱️ Wallet list sync completed in 953.514µs
backend-1  |   ⏱️ Synced 4afzscrv in 4.250338133s (no changes)
backend-1  | ⚡ Non-blocking wallet list served in 164.382789ms
backend-1  | 2025-08-29T19:49:59.963001Z  INFO canary::api: get_current_block_header completed in 1.460355ms
backend-1  |   ⏱️ Synced l6refrr3 in 32.985558173s (no changes)
backend-1  | ✅ Team tier sync completed: 2/2 successful in 37.235955898s
backend-1  | 🔓 Released wallet manager mutex after 37.239449336s (Team tier parallel sync)
backend-1  | 🔓 Released team tier sync mutex after 37.239457537s
backend-1  | 🔒 Block sync task waited 37.239342878s for wallet manager mutex
backend-1  | 🔓 Released block sync mutex after 37.405906952s
backend-1  | ⚡ Non-blocking wallet list served in 154.100339ms
backend-1  | 🔓 Released block sync mutex after 171.168432ms
backend-1  | 🔒 Team sync task waited 171.129617ms for wallet manager mutex

Note that we need to fix both sending/receiving (unconfirmed) and sent/received (confirmed)

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