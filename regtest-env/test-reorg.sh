#!/bin/bash

# Blockchain Reorganization Test Script
# Usage: ./test-reorg.sh [blocks_to_reorg]

set -e

BLOCKS_TO_REORG=${1:-3}

echo "🔄 Testing Blockchain Reorganization (reorg $BLOCKS_TO_REORG blocks)"

# Function to run bitcoin-cli
btc() {
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 "$@"
}

# Function to run bitcoin-cli with wallet
btc_wallet() {
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 -rpcwallet=default "$@"
}

echo "📊 Current blockchain state:"
INITIAL_HEIGHT=$(btc getblockcount)
INITIAL_TIP=$(btc getbestblockhash)
echo "   Height: $INITIAL_HEIGHT"
echo "   Tip: $INITIAL_TIP"

# Ensure we have enough blocks to reorg
if [ $INITIAL_HEIGHT -lt $BLOCKS_TO_REORG ]; then
    echo "⚠️  Not enough blocks for reorg. Mining some blocks first..."
    NEEDED_BLOCKS=$((BLOCKS_TO_REORG + 3))
    ../docker-utils.sh mine $NEEDED_BLOCKS
    INITIAL_HEIGHT=$(btc getblockcount)
    INITIAL_TIP=$(btc getbestblockhash)
    echo "✅ New height: $INITIAL_HEIGHT"
fi

# Create test transaction before reorg
echo ""
echo "💰 Creating test transaction before reorg..."
TEST_ADDRESS=$(btc_wallet getnewaddress)
TEST_TXID=$(btc_wallet sendtoaddress $TEST_ADDRESS 0.001)
echo "✅ Test transaction: $TEST_TXID"

# Mine blocks to confirm the transaction
echo "⛏️  Mining blocks to confirm transaction..."
../docker-utils.sh mine $BLOCKS_TO_REORG
CONFIRMED_HEIGHT=$(btc getblockcount)
CONFIRMED_TIP=$(btc getbestblockhash)

echo "📊 After mining:"
echo "   Height: $CONFIRMED_HEIGHT"
echo "   Tip: $CONFIRMED_TIP"

# Verify transaction is confirmed
TX_INFO=$(btc_wallet gettransaction $TEST_TXID)
CONFIRMATIONS=$(echo "$TX_INFO" | jq -r '.confirmations')
echo "   Transaction confirmations: $CONFIRMATIONS"

# Find the block hash to invalidate (go back to before our test transaction)
REORG_TARGET_HEIGHT=$((INITIAL_HEIGHT))
REORG_BLOCK_HASH=$(btc getblockhash $REORG_TARGET_HEIGHT)

echo ""
echo "🔄 Starting reorganization..."
echo "   Invalidating blocks from height $REORG_TARGET_HEIGHT"
echo "   Target block: $REORG_BLOCK_HASH"

# Invalidate the block (this will cause a reorg)
btc invalidateblock $REORG_BLOCK_HASH

NEW_HEIGHT=$(btc getblockcount)
NEW_TIP=$(btc getbestblockhash)

echo "📊 After invalidation:"
echo "   Height: $NEW_HEIGHT"
echo "   Tip: $NEW_TIP"

# Check transaction status (should be back in mempool)
echo ""
echo "🔍 Checking transaction status after reorg..."
TX_INFO_AFTER=$(btc_wallet gettransaction $TEST_TXID 2>/dev/null || echo "{}")
CONFIRMATIONS_AFTER=$(echo "$TX_INFO_AFTER" | jq -r '.confirmations // 0')

if [ "$CONFIRMATIONS_AFTER" -eq 0 ]; then
    echo "✅ Transaction is back in mempool (0 confirmations)"
    
    # Check if it's actually in mempool
    if btc getmempoolentry $TEST_TXID > /dev/null 2>&1; then
        echo "✅ Confirmed: Transaction is in mempool"
    else
        echo "⚠️  Transaction not found in mempool (may have been dropped)"
    fi
else
    echo "⚠️  Transaction still has $CONFIRMATIONS_AFTER confirmations"
fi

# Create alternate chain (longer than original)
ALTERNATE_BLOCKS=$((BLOCKS_TO_REORG + 2))
echo ""
echo "⛏️  Mining alternate chain ($ALTERNATE_BLOCKS blocks)..."
../docker-utils.sh mine $ALTERNATE_BLOCKS

FINAL_HEIGHT=$(btc getblockcount)
FINAL_TIP=$(btc getbestblockhash)

echo "📊 Final state:"
echo "   Height: $FINAL_HEIGHT"
echo "   Tip: $FINAL_TIP"

# Check final transaction status
echo ""
echo "🔍 Final transaction status..."
TX_INFO_FINAL=$(btc_wallet gettransaction $TEST_TXID 2>/dev/null || echo "{}")
CONFIRMATIONS_FINAL=$(echo "$TX_INFO_FINAL" | jq -r '.confirmations // 0')

if [ "$CONFIRMATIONS_FINAL" -gt 0 ]; then
    echo "✅ Transaction re-confirmed with $CONFIRMATIONS_FINAL confirmations"
elif [ "$CONFIRMATIONS_FINAL" -eq 0 ]; then
    echo "📋 Transaction still in mempool"
else
    echo "❌ Transaction status unknown"
fi

echo ""
echo "🎯 Reorg test completed! Check your application for:"
echo "   - Transaction moved from confirmed → mempool → confirmed"
echo "   - Proper state transitions in database"
echo "   - Balance updates reflecting reorg"
echo "   - No lost transactions or double-counting"
echo ""
echo "📊 Summary:"
echo "   Initial height: $INITIAL_HEIGHT → Final height: $FINAL_HEIGHT"
echo "   Transaction: $TEST_TXID"
echo "   Final confirmations: $CONFIRMATIONS_FINAL"

echo ""
echo "🔍 Monitor your backend logs for reorg detection messages!"

# Show mempool and blockchain info
echo ""
echo "📊 Final mempool status:"
btc getmempoolinfo