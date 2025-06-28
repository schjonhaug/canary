#!/bin/bash

# Mempool Purge Test Script
# Usage: ./test-mempool-purge.sh [method]
# Methods: restart, double-spend, low-fee

set -e

METHOD=${1:-restart}

echo "🗑️  Testing Mempool Purge using method: $METHOD"

# Function to run bitcoin-cli
btc() {
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 "$@"
}

# Function to run bitcoin-cli with wallet
btc_wallet() {
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 -rpcwallet=default "$@"
}

case "$METHOD" in
    "restart")
        echo "🔄 Method: Bitcoin node restart (simulates mempool purge)"
        
        # Check current mempool
        echo "📊 Current mempool before restart:"
        MEMPOOL_BEFORE=$(btc getrawmempool)
        echo "$MEMPOOL_BEFORE" | jq length
        echo "$MEMPOOL_BEFORE" | jq -r '.[]' | head -5
        
        if [ "$(echo "$MEMPOOL_BEFORE" | jq length)" -eq 0 ]; then
            echo "⚠️  Mempool is empty. Creating test transaction first..."
            NEW_ADDRESS=$(btc_wallet getnewaddress)
            # Use alice-fund instead of non-existent send-mempool
            ./docker-utils.sh alice-fund $NEW_ADDRESS 0.001
            echo "✅ Created test transaction"
        fi
        
        echo ""
        echo "🛑 Stopping Bitcoin node..."
        docker stop bitcoind-regtest
        
        echo "⏳ Waiting 3 seconds..."
        sleep 3
        
        echo "🚀 Starting Bitcoin node..."
        docker start bitcoind-regtest
        
        # Wait for Bitcoin to be ready
        echo "⏳ Waiting for Bitcoin Core to be ready..."
        timeout=30
        while [ $timeout -gt 0 ]; do
            if btc getblockchaininfo > /dev/null 2>&1; then
                echo "✅ Bitcoin Core is ready"
                break
            fi
            sleep 1
            timeout=$((timeout-1))
        done
        
        echo ""
        echo "📊 Mempool after restart:"
        MEMPOOL_AFTER=$(btc getrawmempool)
        echo "$MEMPOOL_AFTER" | jq length
        
        echo ""
        echo "🎯 Result: Mempool should be empty after restart"
        echo "   This simulates various purge scenarios like:"
        echo "   - Node restart"
        echo "   - Memory pressure eviction"
        echo "   - Network partition recovery"
        ;;
        
    "double-spend")
        echo "💰 Method: Double-spend conflict (one tx will be purged)"
        
        # Get a UTXO to double-spend
        echo "🔍 Finding UTXO to double-spend..."
        UTXOS=$(btc_wallet listunspent 1)
        
        if [ "$(echo "$UTXOS" | jq length)" -eq 0 ]; then
            echo "❌ No confirmed UTXOs available for double-spend test"
            echo "💡 Mine some blocks first: ./docker-utils.sh mine 6"
            exit 1
        fi
        
        # Get first available UTXO
        UTXO=$(echo "$UTXOS" | jq -r '.[0]')
        UTXO_TXID=$(echo "$UTXO" | jq -r '.txid')
        UTXO_VOUT=$(echo "$UTXO" | jq -r '.vout')
        UTXO_AMOUNT=$(echo "$UTXO" | jq -r '.amount')
        
        echo "📋 Using UTXO: $UTXO_TXID:$UTXO_VOUT ($UTXO_AMOUNT BTC)"
        
        # Create two addresses for double-spend
        ADDRESS1=$(btc_wallet getnewaddress)
        ADDRESS2=$(btc_wallet getnewaddress)
        
        # Send amount split for fees
        SEND_AMOUNT=$(echo "scale=8; $UTXO_AMOUNT - 0.001" | bc -l)
        
        echo "🚀 Creating first transaction to $ADDRESS1..."
        # Create raw transaction manually to ensure we use the same UTXO
        RAW_TX1=$(btc createrawtransaction "[{\"txid\":\"$UTXO_TXID\",\"vout\":$UTXO_VOUT}]" "{\"$ADDRESS1\":$SEND_AMOUNT}")
        SIGNED_TX1=$(btc_wallet signrawtransactionwithwallet "$RAW_TX1" | jq -r '.hex')
        TXID1=$(btc sendrawtransaction "$SIGNED_TX1")
        
        echo "✅ First transaction: $TXID1"
        
        echo "🚀 Creating conflicting transaction to $ADDRESS2..."
        # Create conflicting transaction using same UTXO
        RAW_TX2=$(btc createrawtransaction "[{\"txid\":\"$UTXO_TXID\",\"vout\":$UTXO_VOUT}]" "{\"$ADDRESS2\":$SEND_AMOUNT}")
        SIGNED_TX2=$(btc_wallet signrawtransactionwithwallet "$RAW_TX2" | jq -r '.hex')
        
        # Try to send second transaction (should fail)
        echo "🚀 Attempting to send conflicting transaction..."
        if btc sendrawtransaction "$SIGNED_TX2" 2>/dev/null; then
            echo "❌ Unexpected: Second transaction was accepted"
        else
            echo "✅ Expected: Second transaction rejected (double-spend)"
        fi
        
        echo ""
        echo "🎯 Result: First transaction should remain in mempool"
        echo "   Second transaction should be rejected/purged"
        echo "   This demonstrates conflict resolution"
        ;;
        
    "low-fee")
        echo "💸 Method: Low-fee transaction (may be purged under fee pressure)"
        
        echo "🚀 Creating very low-fee transaction..."
        NEW_ADDRESS=$(btc_wallet getnewaddress)
        
        # Create transaction with extremely low fee (1 sat/byte)
        LOW_FEE_RATE=0.00000001  # 1 sat/kB
        
        TXID=$(btc_wallet sendtoaddress $NEW_ADDRESS 0.001 "" "" false true $LOW_FEE_RATE "unset" 2>/dev/null || echo "")
        
        if [ -n "$TXID" ]; then
            echo "✅ Low-fee transaction created: $TXID"
            
            # Get fee details
            TX_INFO=$(btc getmempoolentry $TXID 2>/dev/null || echo "")
            if [ -n "$TX_INFO" ]; then
                FEE=$(echo "$TX_INFO" | jq -r '.fees.base')
                SIZE=$(echo "$TX_INFO" | jq -r '.size')
                FEE_RATE=$(echo "scale=8; $FEE * 100000000 / $SIZE" | bc -l)
                
                echo "📊 Transaction details:"
                echo "   Fee: $FEE BTC"
                echo "   Size: $SIZE bytes"
                echo "   Fee rate: $FEE_RATE sat/byte"
            fi
            
            echo ""
            echo "💡 This transaction may be purged when:"
            echo "   - Mempool becomes full"
            echo "   - Higher fee transactions arrive"
            echo "   - Node restarts"
            echo ""
            echo "🧪 To test purging, create many higher-fee transactions:"
            echo "   for i in {1..10}; do ./docker-utils.sh alice-fund \$(btc_wallet getnewaddress) 0.001; done"
        else
            echo "❌ Failed to create low-fee transaction"
            echo "💡 Fee might be too low to be accepted even in regtest"
        fi
        ;;
        
    *)
        echo "❌ Unknown method: $METHOD"
        echo "Available methods:"
        echo "  restart     - Restart Bitcoin node (clears mempool)"
        echo "  double-spend - Create conflicting transactions"
        echo "  low-fee     - Create low-fee transaction for purging"
        echo ""
        echo "Usage: $0 [method]"
        exit 1
        ;;
esac

echo ""
echo "🔍 Monitor your backend logs for purge detection messages!"
echo "📊 Check your application for updated transaction states!"

# Show final mempool status
echo ""
echo "📊 Final mempool status:"
btc getmempoolinfo