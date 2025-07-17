# Canary

Canary is a Bitcoin wallet management service built in Rust that provides REST API endpoints for creating and managing Bitcoin wallets using BDK (Bitcoin Development Kit).

## Future Improvements

### Additional Transaction Pattern Detection

Currently, Canary detects and classifies the following transaction patterns:
- 🔄 **Consolidation** - Combining multiple UTXOs into one
- 📤 **RBF (Replace-By-Fee)** - Fee bumping existing unconfirmed transactions  
- 🚀 **CPFP (Child-Pays-For-Parent)** - Spending unconfirmed outputs to boost parent transaction fees

#### High Priority Patterns to Add:

**1. Payment Batching Detection** 🎯
- **What**: Single transaction paying multiple recipients (common with exchanges/services)
- **Detection**: Large confirmed decrease, but multiple smaller outputs to different addresses
- **Value**: Users should know when they're paying multiple people vs. one person

**2. Dust Attack Detection** ⚠️
- **What**: Tiny amounts (546-1000 sats) sent to spy on users
- **Detection**: Small untrusted_pending increases from unknown sources
- **Value**: Security warning - users shouldn't spend these UTXOs

**3. Lightning Channel Operations** ⚡
- **Opening**: Funds locked in 2-of-2 multisig (shows as "locked" rather than "sent")
- **Closing**: Multisig unlocked back to regular addresses
- **Value**: Users should know their funds are locked in Lightning, not lost

**4. CoinJoin Privacy Mixing** 🔒
- **What**: Collaborative transactions mixing multiple users' coins
- **Detection**: Zero net balance change but transaction activity
- **Value**: Privacy indication, regulatory awareness

**5. Block Reorg/Invalidation Detection** 🔄
- **What**: A previously confirmed block becomes invalid due to a blockchain reorganization (reorg)
- **Detection**: Transactions that were confirmed become unconfirmed or disappear; confirmed balance decreases unexpectedly after a reorg event
- **Value**: Users are alerted that some transactions may have been reversed or are now pending due to a chain reorganization

**6. Mempool Purge Detection** 🗑️
- **What**: A transaction in the mempool gets purged (e.g., due to low fee, mempool eviction, or node restart)
- **Detection**: Unconfirmed transaction disappears from the mempool and wallet, pending balance decreases without confirmation or spending event
- **Value**: Users are notified that their unconfirmed transaction was dropped and may need to resend or increase the fee

#### Medium Priority Patterns:

**5. Payment Batching Variants**
- Detecting when you're the recipient of a batched payment
- Multi-output consolidations (consolidating to multiple addresses)

**6. Multi-signature Patterns**
- Corporate/institutional wallet usage detection
- Time-locked multisig operations

#### Implementation Approach:
- Add new detection logic in the `sync_all_wallets()` function similar to existing RBF/CPFP/consolidation
- Use the same early-exit pattern with `is_special_tx` flag to prevent duplicate messages
- Add appropriate emoji indicators for each transaction type
- Maintain hierarchical detection order for clean, specific transaction classification