#!/bin/bash

# Bitcoin Regtest Development Utilities
# 
# This script provides a complete Bitcoin regtest development environment with:
# - Docker-based Bitcoin Core + Fulcrum Electrum server
# - Descriptor wallets covering all 4 script types (wpkh, pkh, sh(wpkh), tr)
# - Funded and empty wallet variants for each type
# - Backend integration for Output Descriptor Monitor
# - Advanced Bitcoin transaction testing (RBF, CPFP, mempool operations)
#
# Key Commands:
#   start           - Start infrastructure (Bitcoin Core + Fulcrum)
#   init            - Create wallets and add to backend
#
# Workflow:
#   1. cd ../backend && cargo run   (start backend)
#   2. ./dev.sh init                (starts infra, creates wallets, adds to backend)

# Function to run bitcoin-cli against the Docker container  
btc() {
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 "$@"
}

# Descriptor wallet helpers (funded)
btc_segwit_desc() {
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 -rpcwallet=segwit-desc "$@"
}
btc_legacy_desc() {
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 -rpcwallet=legacy-desc "$@"
}
btc_nested_desc() {
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 -rpcwallet=nested-desc "$@"
}
btc_taproot_desc() {
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 -rpcwallet=taproot-desc "$@"
}

# Descriptor wallet helpers (empty)
btc_segwit_empty() {
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 -rpcwallet=segwit-empty "$@"
}
btc_legacy_empty() {
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 -rpcwallet=legacy-empty "$@"
}
btc_nested_empty() {
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 -rpcwallet=nested-empty "$@"
}
btc_taproot_empty() {
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 -rpcwallet=taproot-empty "$@"
}

# Function to run bitcoin-cli with Charlie wallet against the Docker container
btc_charlie() {
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 -rpcwallet=charlie "$@"
}

# Function to run bitcoin-cli with Bacon wallet against the Docker container
btc_bacon() {
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 -rpcwallet=bacon "$@"
}

# Function to run bitcoin-cli with Miner wallet against the Docker container
btc_miner() {
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 -rpcwallet=miner "$@"
}


# Function to run bitcoin-cli with specific wallet (generic version)
btc_wallet() {
    local wallet_name=$1
    shift
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 -rpcwallet="$wallet_name" "$@"
}

# Get the appropriate Bitcoin Core address type for a wallet name
get_address_type() {
    case "$1" in
        legacy-desc|legacy-empty)   echo "legacy" ;;
        nested-desc|nested-empty)   echo "p2sh-segwit" ;;
        taproot-desc|taproot-empty) echo "bech32m" ;;
        *)                          echo "bech32" ;;
    esac
}

# --- CPFP logic as a function for any wallet ---
cpfp_for_wallet() {
    WALLET="$1"
    PARENT_TXID="$2"
    if [ -z "$PARENT_TXID" ]; then
        echo "Usage: $0 $WALLET cpfp <parent_txid>"
        exit 1
    fi
    btc_wallet() {
        local wallet_name=$1
        shift
        docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 -rpcwallet="$wallet_name" "$@"
    }
    # Check if wallet exists
    if ! btc_wallet "$WALLET" getwalletinfo >/dev/null 2>&1; then
        echo "❌ $WALLET wallet not found. Run '$0 init' first"
        exit 1
    fi
    # Check wallet has sufficient confirmed funds for fees
    WALLET_BALANCE=$(btc_wallet "$WALLET" getbalance)
    echo "💰 $WALLET wallet balance: $WALLET_BALANCE BTC (confirmed)"
    if [ "$(echo "$WALLET_BALANCE < 0.001" | bc -l)" -eq 1 ]; then
        echo "❌ $WALLET needs confirmed funds for CPFP fees. Current balance: $WALLET_BALANCE BTC"
        echo "💡 Fund $WALLET first with: $0 miner sending $WALLET 0.01 && $0 mine 1"
        exit 1
    fi
    # $WALLET creates high-fee child transaction (CPFP)
    echo "👶 Creating CPFP child transaction ($WALLET spends unconfirmed output)..."
    # Verify parent transaction is in wallet and unconfirmed
    PARENT_IN_WALLET=$(btc_wallet "$WALLET" gettransaction $PARENT_TXID 2>/dev/null || echo "not found")
    if [ "$PARENT_IN_WALLET" = "not found" ]; then
        echo "❌ Parent transaction not found in $WALLET wallet"
        exit 1
    fi
    PARENT_CONFIRMATIONS=$(echo "$PARENT_IN_WALLET" | jq -r '.confirmations')
    PARENT_AMOUNT=$(echo "$PARENT_IN_WALLET" | jq -r '.amount')
    if [ "$PARENT_CONFIRMATIONS" -gt 0 ]; then
        echo "❌ Parent transaction is already confirmed ($PARENT_CONFIRMATIONS confirmations)"
        echo "💡 CPFP only works on unconfirmed transactions"
        exit 1
    fi
    echo "   ✅ Parent transaction found in $WALLET wallet (unconfirmed)"
    echo "   💰 Parent amount: $PARENT_AMOUNT BTC"
    # Get the raw parent transaction to find outputs that belong to wallet
    PARENT_RAW=$(btc getrawtransaction $PARENT_TXID true)
    # Find which output(s) in the parent transaction belong to wallet
    WALLET_OUTPUTS=()
    TOTAL_WALLET_AMOUNT=0
    OUTPUT_COUNT=$(echo "$PARENT_RAW" | jq '.vout | length')
    for ((i=0; i<OUTPUT_COUNT; i++)); do
        OUTPUT_ADDRESS=$(echo "$PARENT_RAW" | jq -r ".vout[$i].scriptPubKey.address")
        OUTPUT_VALUE=$(echo "$PARENT_RAW" | jq -r ".vout[$i].value")
        # Check if this address belongs to wallet
        if btc_wallet "$WALLET" getaddressinfo "$OUTPUT_ADDRESS" 2>/dev/null | jq -r '.ismine' | grep -q "true"; then
            WALLET_OUTPUTS+=("$i:$OUTPUT_VALUE")
            TOTAL_WALLET_AMOUNT=$(echo "scale=8; $TOTAL_WALLET_AMOUNT + $OUTPUT_VALUE" | bc -l)
            echo "   📍 Found $WALLET's output $i: $OUTPUT_VALUE BTC at $OUTPUT_ADDRESS"
        fi
    done
    if [ ${#WALLET_OUTPUTS[@]} -eq 0 ]; then
        echo "❌ No outputs in parent transaction belong to $WALLET's wallet"
        exit 1
    fi
    # Calculate child amount (dynamic fee based on available amount)
    # Use 50% of available amount as fee for CPFP acceleration, but cap at 0.005 BTC
    HALF_AMOUNT=$(echo "scale=8; $TOTAL_WALLET_AMOUNT * 0.5" | bc -l)
    if [ "$(echo "$HALF_AMOUNT > 0.005" | bc -l)" -eq 1 ]; then
        DYNAMIC_FEE=0.005
    else
        DYNAMIC_FEE=$HALF_AMOUNT
    fi
    # For small amounts, use a more flexible minimum fee
    # Minimum of 0.00001 BTC (1000 sats) or 80% of available amount, whichever is smaller
    MIN_FEE_FLEXIBLE=$(echo "scale=8; $TOTAL_WALLET_AMOUNT * 0.8" | bc -l)
    MIN_FEE_ABSOLUTE=0.00001
    if [ "$(echo "$MIN_FEE_FLEXIBLE < $MIN_FEE_ABSOLUTE" | bc -l)" -eq 1 ]; then
        MIN_FEE=$MIN_FEE_FLEXIBLE
    else
        MIN_FEE=$MIN_FEE_ABSOLUTE
    fi
    # Apply the flexible minimum fee
    if [ "$(echo "$DYNAMIC_FEE < $MIN_FEE" | bc -l)" -eq 1 ]; then
        DYNAMIC_FEE=$MIN_FEE
    fi
    CHILD_AMOUNT_RAW=$(echo "scale=8; $TOTAL_WALLET_AMOUNT - $DYNAMIC_FEE" | bc -l)
    # Ensure proper decimal formatting for JSON (must start with 0. not .)
    CHILD_AMOUNT=$(echo "$CHILD_AMOUNT_RAW" | sed 's/^\./0./')
    # Flexible minimum child amount - 0.00001 BTC (1000 sats) for small transactions
    MIN_CHILD_AMOUNT=0.00001
    if [ "$(echo "$CHILD_AMOUNT < $MIN_CHILD_AMOUNT" | bc -l)" -eq 1 ]; then
        echo "❌ Child amount too small: $CHILD_AMOUNT BTC (need at least $MIN_CHILD_AMOUNT BTC after fees)"
        echo "   Available: $TOTAL_WALLET_AMOUNT BTC, Required fee: $DYNAMIC_FEE BTC"
        exit 1
    fi
    # Get change address for the child transaction
    CHANGE_ADDRESS=$(btc_wallet "$WALLET" getnewaddress "" "$(get_address_type "$WALLET")")
    echo "   🔍 Creating CPFP child spending $TOTAL_WALLET_AMOUNT BTC → $CHILD_AMOUNT BTC ($DYNAMIC_FEE BTC fee)"
    echo "   🎯 Target: $CHANGE_ADDRESS"
    # Create raw transaction inputs from wallet's outputs in the parent transaction
    INPUTS="["
    for i in "${!WALLET_OUTPUTS[@]}"; do
        OUTPUT_INDEX=$(echo "${WALLET_OUTPUTS[$i]}" | cut -d':' -f1)
        if [ $i -gt 0 ]; then
            INPUTS+="," 
        fi
        INPUTS+="{\"txid\":\"$PARENT_TXID\",\"vout\":$OUTPUT_INDEX}"
    done
    INPUTS+="]"
    # Create raw transaction output
    OUTPUTS="{\"$CHANGE_ADDRESS\":$CHILD_AMOUNT}"
    # Create the raw transaction that specifically spends from the unconfirmed parent
    echo "   🔧 Creating raw transaction..."
    RAW_TX=$(btc_wallet "$WALLET" createrawtransaction "$INPUTS" "$OUTPUTS")
    if [ -z "$RAW_TX" ]; then
        echo "❌ Failed to create raw transaction"
        exit 1
    fi
    # Sign the raw transaction
    echo "   ✍️  Signing transaction..."
    SIGNED_TX=$(btc_wallet "$WALLET" signrawtransactionwithwallet "$RAW_TX")
    SIGNED_HEX=$(echo "$SIGNED_TX" | jq -r '.hex')
    SIGN_COMPLETE=$(echo "$SIGNED_TX" | jq -r '.complete')
    if [ "$SIGN_COMPLETE" != "true" ]; then
        echo "❌ Failed to sign transaction"
        echo "Signing result: $SIGNED_TX"
        exit 1
    fi
    # Broadcast the child transaction
    echo "   📡 Broadcasting CPFP child transaction..."
    CHILD_TXID=$(btc sendrawtransaction "$SIGNED_HEX")
    if [ -z "$CHILD_TXID" ]; then
        echo "❌ Failed to create child transaction"
        echo "   $WALLET balance (confirmed): $(btc_wallet "$WALLET" getbalance)"
        echo "   $WALLET balance (unconfirmed): $(btc_wallet "$WALLET" getbalance "*" 0)"
        exit 1
    fi
    echo "   ✅ Child transaction created: $CHILD_TXID"
    echo "   💰 Amount: $CHILD_AMOUNT BTC (high fee: $DYNAMIC_FEE BTC)"
    echo "   🎯 Target: $CHANGE_ADDRESS ($WALLET change address)"
    echo ""
    echo "🔗 CPFP Relationship Created:"
    echo "   👨 Parent: $PARENT_TXID ($WALLET → $WALLET, stuck due to low fee)"
    echo "   👶 Child:  $CHILD_TXID ($WALLET → $WALLET, high fee accelerates parent)"
    echo ""
    echo "📊 Current mempool status:"
    MEMPOOL_SIZE=$(btc getmempoolinfo | grep '"size"' | cut -d':' -f2 | tr -d ' ,')
    echo "   Transactions in mempool: $MEMPOOL_SIZE"
    echo ""
    echo "🔍 Transaction Details:"
    echo "Parent transaction ($WALLET wallet view):"
    btc_wallet "$WALLET" gettransaction $PARENT_TXID | jq -r '"   Fee: " + (.fee | tostring) + " BTC, Confirmations: " + (.confirmations | tostring)'
    echo ""
    echo "Child transaction ($WALLET wallet view):"
    btc_wallet "$WALLET" gettransaction $CHILD_TXID | jq -r '"   Fee: " + (.fee | tostring) + " BTC, Confirmations: " + (.confirmations | tostring)'
    echo ""
    echo "🎉 CPFP test scenario complete!"
    echo ""
    echo "📱 Check your application to see:"
    echo "   - Both transactions appear in mempool"
    echo "   - $WALLET's balance shows pending amounts"
    echo "   - CPFP relationship should be detected"
    echo ""
    echo "⛏️  Mine blocks to confirm both transactions:"
    echo "   $0 mine 1"
    echo ""
    echo "🔍 Both transactions should confirm together due to CPFP!"
}
# --- End CPFP logic function ---

# --- Helper function to get mempool TXID by index ---
get_mempool_txid() {
    local INDEX=${1:-0}
    
    # Get mempool transactions as array
    local MEMPOOL_TXIDS=$(btc getrawmempool)
    
    # Check if mempool is empty
    if [ "$MEMPOOL_TXIDS" = "[]" ]; then
        echo "Error: Mempool is empty" >&2
        return 1
    fi
    
    # Extract the specified transaction (default to first)
    local TXID=$(echo "$MEMPOOL_TXIDS" | jq -r ".[$INDEX] // empty")
    
    if [ -z "$TXID" ] || [ "$TXID" = "null" ]; then
        echo "Error: No transaction found at index $INDEX" >&2
        echo "Available transactions:" >&2
        echo "$MEMPOOL_TXIDS" | jq -r '.[]' | nl -v0 >&2
        return 1
    fi
    
    echo "$TXID"
}

# --- Mempool purge testing function ---
mempool_purge() {
    local METHOD=${1:-restart}
    
    echo "🗑️  Testing Mempool Purge using method: $METHOD"
    
    case "$METHOD" in
        "restart")
            echo "🔄 Method: Bitcoin node restart (simulates mempool purge)"
            
            # Check current mempool
            echo "📊 Current mempool before restart:"
            local MEMPOOL_BEFORE=$(btc getrawmempool)
            echo "$MEMPOOL_BEFORE" | jq length
            echo "$MEMPOOL_BEFORE" | jq -r '.[]' | head -5
            
            if [ "$(echo "$MEMPOOL_BEFORE" | jq length)" -eq 0 ]; then
                echo "⚠️  Mempool is empty. Creating test transaction first..."
                local NEW_ADDRESS=$(btc_segwit_desc getnewaddress)
                btc loadwallet "segwit-desc" 2>/dev/null || true
                btc_segwit_desc sendtoaddress "$NEW_ADDRESS" 0.001
                echo "✅ Created test transaction"
                # Update mempool after creating transaction
                MEMPOOL_BEFORE=$(btc getrawmempool)
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
            local timeout=30
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
            local MEMPOOL_AFTER=$(btc getrawmempool)
            echo "$MEMPOOL_AFTER" | jq length
            
            # Check if mempool was actually purged
            if [ "$(echo "$MEMPOOL_AFTER" | jq length)" -eq 0 ]; then
                echo "✅ SUCCESS: Mempool was purged during restart"
            else
                echo "⚠️  WARNING: Mempool was NOT purged during restart"
                echo "   This may be due to mempool persistence being enabled"
                echo "   Check bitcoin.conf for 'persistmempool=0' setting"
                echo "   Transactions remaining:"
                echo "$MEMPOOL_AFTER" | jq -r '.[]' | head -5
            fi
            
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
            btc loadwallet "segwit-desc" 2>/dev/null || true
            local UTXOS=$(btc_segwit_desc listunspent 1)
            
            if [ "$(echo "$UTXOS" | jq length)" -eq 0 ]; then
                echo "❌ No confirmed UTXOs available for double-spend test"
                echo "💡 Mine some blocks first: $0 mine 6"
                return 1
            fi
            
            # Get first available UTXO
            local UTXO=$(echo "$UTXOS" | jq -r '.[0]')
            local UTXO_TXID=$(echo "$UTXO" | jq -r '.txid')
            local UTXO_VOUT=$(echo "$UTXO" | jq -r '.vout')
            local UTXO_AMOUNT=$(echo "$UTXO" | jq -r '.amount')
            
            echo "📋 Using UTXO: $UTXO_TXID:$UTXO_VOUT ($UTXO_AMOUNT BTC)"
            
            # Create two addresses for double-spend
            local ADDRESS1=$(btc_segwit_desc getnewaddress)
            local ADDRESS2=$(btc_segwit_desc getnewaddress)
            
            # Send amount split for fees
            local SEND_AMOUNT=$(echo "scale=8; $UTXO_AMOUNT - 0.001" | bc -l)
            
            echo "🚀 Creating first transaction to $ADDRESS1..."
            # Create raw transaction manually to ensure we use the same UTXO
            local RAW_TX1=$(btc createrawtransaction "[{\"txid\":\"$UTXO_TXID\",\"vout\":$UTXO_VOUT}]" "{\"$ADDRESS1\":$SEND_AMOUNT}")
            local SIGNED_TX1=$(btc_segwit_desc signrawtransactionwithwallet "$RAW_TX1" | jq -r '.hex')
            local TXID1=$(btc sendrawtransaction "$SIGNED_TX1")
            
            echo "✅ First transaction: $TXID1"
            
            echo "🚀 Creating conflicting transaction to $ADDRESS2..."
            # Create conflicting transaction using same UTXO
            local RAW_TX2=$(btc createrawtransaction "[{\"txid\":\"$UTXO_TXID\",\"vout\":$UTXO_VOUT}]" "{\"$ADDRESS2\":$SEND_AMOUNT}")
            local SIGNED_TX2=$(btc_segwit_desc signrawtransactionwithwallet "$RAW_TX2" | jq -r '.hex')
            
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
            btc loadwallet "segwit-desc" 2>/dev/null || true
            local NEW_ADDRESS=$(btc_segwit_desc getnewaddress)
            
            # Create transaction with extremely low fee (1 sat/kB)
            local LOW_FEE_RATE=0.00000001
            
            local TXID=$(btc_segwit_desc sendtoaddress "$NEW_ADDRESS" 0.001 "" "" false true "$LOW_FEE_RATE" "unset" 2>/dev/null || echo "")
            
            if [ -n "$TXID" ]; then
                echo "✅ Low-fee transaction created: $TXID"
                
                # Get fee details
                local TX_INFO=$(btc getmempoolentry "$TXID" 2>/dev/null || echo "")
                if [ -n "$TX_INFO" ]; then
                    local FEE=$(echo "$TX_INFO" | jq -r '.fees.base')
                    local SIZE=$(echo "$TX_INFO" | jq -r '.size')
                    local FEE_RATE=$(echo "scale=8; $FEE * 100000000 / $SIZE" | bc -l)
                    
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
                echo "   for i in {1..10}; do $0 segwit-desc fund \$(btc_segwit_desc getnewaddress) 0.001; done"
            else
                echo "❌ Failed to create low-fee transaction"
                echo "💡 Fee might be too low to be accepted even in regtest"
            fi
            ;;
        
        *)
            echo "❌ Unknown method: $METHOD"
            echo "Available methods:"
            echo "  restart     - Restart Bitcoin node (clears mempool if persistmempool=0)"
            echo "  double-spend - Create conflicting transactions"
            echo "  low-fee     - Create low-fee transaction for purging"
            echo ""
            echo "Usage: $0 mempool-purge [method]"
            return 1
            ;;
    esac
    
    echo ""
    echo "🔍 Monitor your backend logs for purge detection messages!"
    echo "📊 Check your application for updated transaction states!"
    
    # Show final mempool status
    echo ""
    echo "📊 Final mempool status:"
    btc getmempoolinfo
}

# --- Blockchain reorganization function ---
reorg() {
    echo "🔄 Testing Blockchain Reorganization"
    
    echo "📊 Current blockchain state:"
    local INITIAL_HEIGHT=$(btc getblockcount)
    local INITIAL_TIP=$(btc getbestblockhash)
    echo "   Height: $INITIAL_HEIGHT"
    echo "   Tip: $INITIAL_TIP"
    
    # Create test transaction before reorg (segwit-desc sends to segwit-empty)
    echo ""
    echo "💰 Creating test transaction before reorg..."
    btc loadwallet "segwit-desc" 2>/dev/null || true
    btc loadwallet "segwit-empty" 2>/dev/null || true
    local EMPTY_ADDRESS=$(btc_segwit_empty getnewaddress)
    local TEST_TXID=$(btc_segwit_desc sendtoaddress "$EMPTY_ADDRESS" 0.001)
    echo "✅ Test transaction: $TEST_TXID"
    echo "   segwit-desc → segwit-empty: 0.001 BTC"
    echo "   🎯 segwit-empty address: $EMPTY_ADDRESS"
    
    echo ""
    echo "⏸️  After the tx is found in the mempool, press enter to mine a block"
    read -r
    
    # Mine 1 block to confirm the transaction
    echo "⛏️  Mining 1 block to confirm transaction..."
    mine_blocks 1
    local CONFIRMED_HEIGHT=$(btc getblockcount)
    local CONFIRMED_TIP=$(btc getbestblockhash)
    
    echo "📊 After mining:"
    echo "   Height: $CONFIRMED_HEIGHT"
    echo "   Tip: $CONFIRMED_TIP"
    
    # Verify transaction is confirmed
    local TX_INFO=$(btc_segwit_desc gettransaction "$TEST_TXID")
    local CONFIRMATIONS=$(echo "$TX_INFO" | jq -r '.confirmations')
    echo "   Transaction confirmations: $CONFIRMATIONS"
    
    echo ""
    echo "⏸️  Press enter to invalidate the tip block"
    read -r
    
    # Get the current tip block hash to invalidate (same as invalidate-tip)
    local TIP_HASH=$(btc getbestblockhash)
    local TIP_HEIGHT=$(btc getblockcount)
    
    echo ""
    echo "🔄 Starting reorganization..."
    echo "   Invalidating tip block: $TIP_HASH (height: $TIP_HEIGHT)"
    echo "   This will move transaction back to mempool"
    
    # Invalidate the tip block (same approach as invalidate-tip)
    echo "🚫 Invalidating tip block..."
    btc invalidateblock "$TIP_HASH"
    echo "✅ Tip block invalidated successfully"
    
    local NEW_HEIGHT=$(btc getblockcount)
    local NEW_TIP=$(btc getbestblockhash)
    
    echo "📊 After invalidation:"
    echo "   Height: $NEW_HEIGHT"
    echo "   Tip: $NEW_TIP"
    
    # Check transaction status (should be back in mempool)
    echo ""
    echo "🔍 Checking transaction status after reorg..."
    local TX_INFO_AFTER=$(btc_segwit_desc gettransaction "$TEST_TXID" 2>/dev/null || echo "{}")
    local CONFIRMATIONS_AFTER=$(echo "$TX_INFO_AFTER" | jq -r '.confirmations // 0')
    
    if [ "$CONFIRMATIONS_AFTER" -eq 0 ]; then
        echo "✅ Transaction is back in mempool (0 confirmations)"
        
        # Check if it's actually in mempool
        if btc getmempoolentry "$TEST_TXID" > /dev/null 2>&1; then
            echo "✅ Confirmed: Transaction is in mempool"
        else
            echo "⚠️  Transaction not found in mempool (may have been dropped)"
        fi
    else
        echo "⚠️  Transaction still has $CONFIRMATIONS_AFTER confirmations"
    fi

    echo ""
    echo "⏸️  Press enter to mine a new block, completing the reorg"
    read -r
    
    # Mine a new block to re-confirm the transaction
    echo ""
    echo "⛏️  Mining new block to re-confirm transaction..."
    mine_blocks 1
    
    local FINAL_HEIGHT=$(btc getblockcount)
    local FINAL_TIP=$(btc getbestblockhash)
    
    echo "📊 Final state:"
    echo "   Height: $FINAL_HEIGHT"
    echo "   Tip: $FINAL_TIP"
    
    # Check final transaction status
    echo ""
    echo "🔍 Final transaction status..."
    local TX_INFO_FINAL=$(btc_segwit_desc gettransaction "$TEST_TXID" 2>/dev/null || echo "{}")
    local CONFIRMATIONS_FINAL=$(echo "$TX_INFO_FINAL" | jq -r '.confirmations // 0')
    
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
}

# --- Comprehensive test suite function ---
run_tests() {
    local WALLET_ADDRESS=${1:-}
    
    echo "🧪 Canary - Comprehensive Bitcoin Test Suite"
    echo "=========================================="
    
    if [ -z "$WALLET_ADDRESS" ]; then
        echo "⚠️  No wallet address provided. You'll need to:"
        echo "   1. Start your application"
        echo "   2. Add a test wallet"
        echo "   3. Get an address from that wallet"
        echo "   4. Run: $0 run-tests <wallet_address>"
        echo ""
        echo "Example: $0 run-tests bcrt1qtest123456789abcdef"
        return 1
    fi
    
    # Function to pause between tests
    pause_test() {
        echo ""
        echo "⏸️  Pausing for 5 seconds to observe changes..."
        echo "   Check your application for updates!"
        sleep 5
        echo ""
    }
    
    echo "🚀 Starting comprehensive test suite with address: $WALLET_ADDRESS"
    echo ""
    
    # Test 1: Basic mempool transaction
    echo "📍 TEST 1: Basic Mempool Transaction"
    echo "-----------------------------------"
    btc loadwallet "segwit-desc" 2>/dev/null || true
    btc_segwit_desc sendtoaddress "$WALLET_ADDRESS" 0.001
    pause_test
    
    # Test 2: RBF Testing
    echo "📍 TEST 2: RBF (Replace-By-Fee)"
    echo "-------------------------------"
    echo "Creating low-fee transaction for RBF testing..."
    local FIRST_TXID=$(btc_segwit_desc sendtoaddress "$WALLET_ADDRESS" 0.002 "" "" false true 0.00001 "unset")
    echo "First transaction: $FIRST_TXID"
    sleep 2
    
    echo "Attempting RBF replacement with bumpfee..."
    local RESULT=$(btc_segwit_desc bumpfee "$FIRST_TXID" "{\"fee_rate\": 15}" 2>&1 || echo "RBF failed")
    if echo "$RESULT" | jq -e '.txid' > /dev/null 2>&1; then
        local NEW_TXID=$(echo "$RESULT" | jq -r '.txid')
        echo "✅ RBF successful: $NEW_TXID"
    else
        echo "❌ RBF failed: $RESULT"
    fi
    pause_test
    
    # Test 3: CPFP Testing
    echo "📍 TEST 3: CPFP (Child-Pays-For-Parent)"
    echo "---------------------------------------"
    echo "Creating low-fee parent transaction..."
    local PARENT_TXID=$(btc_segwit_desc sendtoaddress "$WALLET_ADDRESS" 0.003 "" "" false true 0.00001 "unset")
    echo "Parent transaction: $PARENT_TXID"
    sleep 2
    
    echo "Creating CPFP child transaction..."
    cpfp_for_wallet "segwit-desc" "$PARENT_TXID"
    pause_test
    
    # Test 4: Mempool Purge Testing
    echo "📍 TEST 4: Mempool Purge (Node Restart)"
    echo "---------------------------------------"
    echo "Creating transaction to be purged..."
    btc_segwit_desc sendtoaddress "$WALLET_ADDRESS" 0.001
    sleep 2

    echo "Testing mempool purge via restart..."
    mempool_purge "restart"
    pause_test
    
    # Test 5: Blockchain Reorganization
    echo "📍 TEST 5: Blockchain Reorganization"
    echo "------------------------------------"
    echo "Creating transaction for reorg testing..."
    btc_segwit_desc sendtoaddress "$WALLET_ADDRESS" 0.004
    sleep 2
    
    echo "Mining blocks to confirm transaction..."
    mine_blocks 3
    sleep 2
    
    echo "Testing blockchain reorganization..."
    reorg 2
    pause_test
    
    # Test 6: Confirmation Testing
    echo "📍 TEST 6: Transaction Confirmation"
    echo "-----------------------------------"
    echo "Creating final test transaction..."
    btc_segwit_desc sendtoaddress "$WALLET_ADDRESS" 0.005
    sleep 2
    
    echo "Confirming transaction..."
    mine_blocks 1
    pause_test
    
    # Final Status
    echo "📍 FINAL STATUS"
    echo "==============="
    echo "All tests completed! Your application should now have examples of:"
    echo ""
    echo "✅ Basic mempool transactions"
    echo "✅ RBF (Replace-By-Fee) relationships"
    echo "✅ CPFP (Child-Pays-For-Parent) chains"
    echo "✅ Mempool purge scenarios"
    echo "✅ Blockchain reorganizations"
    echo "✅ Transaction confirmations"
    echo ""
    echo "🔍 Check your application and database for:"
    echo "   - Transaction state changes"
    echo "   - RBF and CPFP relationships"
    echo "   - Proper balance calculations"
    echo "   - Real-time notification updates"
    echo ""
    
    # Show final blockchain and mempool state
    echo "📊 Final blockchain state:"
    btc getblockchaininfo | jq '.blocks, .bestblockhash'
    
    echo ""
    echo "📊 Final mempool state:"
    btc getmempoolinfo
    
    echo ""
    echo "🎉 Test suite completed successfully!"
    echo "Monitor your backend logs and application for all the changes!"
}

# --- Helper function for mining blocks ---
mine_blocks() {
    local BLOCKS=${1:-1}
    btc loadwallet "miner" 2>/dev/null || true
    local ADDRESS=$(btc_miner getnewaddress)
    btc generatetoaddress "$BLOCKS" "$ADDRESS" >/dev/null 2>&1
}

# Kill backend and frontend processes (ports 3000 and 3001)
kill_servers() {
    if lsof -ti:3000,3001 > /dev/null 2>&1; then
        echo "🔪 Stopping backend/frontend (ports 3000, 3001)..."
        lsof -ti:3000,3001 | xargs kill -9 2>/dev/null || true
        sleep 1
    fi
}

# --- New multi-word command parsing for wallet actions ---
if [[ "$1" == "segwit-desc" || "$1" == "legacy-desc" || "$1" == "nested-desc" || "$1" == "taproot-desc" || "$1" == "segwit-empty" || "$1" == "legacy-empty" || "$1" == "nested-empty" || "$1" == "taproot-empty" || "$1" == "charlie" || "$1" == "miner" || "$1" == "legacy-address" || "$1" == "p2sh-address" || "$1" == "segwit-address" || "$1" == "taproot-address" ]]; then
    WALLET="$1"
    SUBCMD="$2"
    shift 2
    case "$SUBCMD" in
        sending)
            DESTINATION_WALLET="$1"
            shift
            AMOUNTS=("$@")
            
            if [ -z "$DESTINATION_WALLET" ] || [ ${#AMOUNTS[@]} -eq 0 ]; then
                echo "Usage: $0 $WALLET sending <destination_wallet> <amount1> [amount2] [amount3] ..."
                echo "       $0 $WALLET sending <destination_wallet> max  # Drain wallet"
                echo "Available destinations: segwit-desc, segwit-empty, legacy-desc, legacy-empty, nested-desc, nested-empty, taproot-desc, taproot-empty, charlie, miner"
                echo "Examples:"
                echo "  $0 $WALLET sending segwit-empty 0.1 0.2 0.05  # Send three separate transactions"
                echo "  $0 $WALLET sending miner max                  # Drain wallet to miner"
                exit 1
            fi
            
            # Validate destination: wallet name or raw Bitcoin address
            RAW_ADDRESS=""
            case "$DESTINATION_WALLET" in
                segwit-desc|legacy-desc|nested-desc|taproot-desc|segwit-empty|legacy-empty|nested-empty|taproot-empty|charlie|miner|legacy-address|p2sh-address|segwit-address|taproot-address)
                    ;;
                bcrt1*|tb1*|bc1*|[13mn]*)
                    # Raw Bitcoin address
                    RAW_ADDRESS="$DESTINATION_WALLET"
                    ;;
                *)
                    echo "❌ Invalid destination: $DESTINATION_WALLET"
                    echo "Use a wallet name or a raw Bitcoin address (bcrt1..., tb1..., bc1...)"
                    echo "Available wallets: segwit-desc, segwit-empty, legacy-desc, legacy-empty, nested-desc, nested-empty, taproot-desc, taproot-empty, charlie, miner, legacy-address, p2sh-address, segwit-address, taproot-address"
                    exit 1
                    ;;
            esac
            
            # Special validation: miner cannot send to itself
            if [ "$WALLET" == "miner" ] && [ "$DESTINATION_WALLET" == "miner" ]; then
                echo "❌ Miner wallet cannot send to itself"
                echo "Miner can send to: segwit-desc, segwit-empty, charlie, etc."
                exit 1
            fi
            
            # Load source and destination wallets
            btc loadwallet "$WALLET" 2>/dev/null || true
            if [ -z "$RAW_ADDRESS" ]; then
                btc loadwallet "$DESTINATION_WALLET" 2>/dev/null || true
            fi

            # Helper: get target address for destination wallet
            # For raw addresses, return as-is
            # For addr-* wallets, reuse the existing address (single-address wallets)
            # For regular wallets, generate a new address each time
            get_target_address() {
                local dest="$1"
                if [ -n "$RAW_ADDRESS" ]; then
                    echo "$RAW_ADDRESS"
                    return
                fi
                case "$dest" in
                    *-address)
                        # Single-address wallets: reuse the existing address
                        local addr_list
                        addr_list=$(btc_wallet "$dest" listreceivedbyaddress 0 true)
                        echo "$addr_list" | jq -r '.[0].address'
                        ;;
                    *)
                        btc_wallet "$dest" getnewaddress "" "$(get_address_type "$dest")"
                        ;;
                esac
            }

            # Helper: send from wallet with correct change address type
            send_from_wallet() {
                local wallet="$1"
                local target="$2"
                local amount="$3"
                local subtract_fee="${4:-false}"
                local addr_type
                addr_type=$(get_address_type "$wallet")

                case "$wallet" in
                    *-address)
                        # Use 'send' RPC with change routed back to the wallet's own address
                        local own_addr
                        own_addr=$(btc_wallet "$wallet" listreceivedbyaddress 0 true | jq -r '.[0].address')
                        local send_opts="{\"change_address\": \"$own_addr\"}"
                        if [ "$subtract_fee" = "true" ]; then
                            send_opts="{\"change_address\": \"$own_addr\", \"subtract_fee_from_outputs\": [0]}"
                        fi
                        btc_wallet "$wallet" send "{\"$target\": $amount}" null "unset" null "$send_opts" | jq -r '.txid'
                        ;;
                    *)
                        if [ "$addr_type" != "bech32" ]; then
                            # Non-default address type: use 'send' RPC with change_type
                            local send_opts="{\"change_type\": \"$addr_type\"}"
                            if [ "$subtract_fee" = "true" ]; then
                                send_opts="{\"change_type\": \"$addr_type\", \"subtract_fee_from_outputs\": [0]}"
                            fi
                            btc_wallet "$wallet" send "{\"$target\": $amount}" null "unset" null "$send_opts" | jq -r '.txid'
                        else
                            if [ "$subtract_fee" = "true" ]; then
                                btc_wallet "$wallet" sendtoaddress "$target" "$amount" "" "" true
                            else
                                btc_wallet "$wallet" sendtoaddress "$target" "$amount"
                            fi
                        fi
                        ;;
                esac
            }

            # Handle max amount (drain wallet) - single transaction
            if [ "${AMOUNTS[0]}" == "max" ] && [ ${#AMOUNTS[@]} -eq 1 ]; then
                # Get target address from destination wallet
                TARGET_ADDRESS=$(get_target_address "$DESTINATION_WALLET")
                # Get current balance
                CURRENT_BALANCE=$(btc_wallet "$WALLET" getbalance)
                echo "🎯 Draining $WALLET wallet ($CURRENT_BALANCE BTC) to $DESTINATION_WALLET address: $TARGET_ADDRESS"
                # Use subtractfeefromamount to send everything minus fees
                TXID=$(send_from_wallet "$WALLET" "$TARGET_ADDRESS" "$CURRENT_BALANCE" true)
                echo "✅ Transaction sent: $TXID"
                echo "💡 Use '$0 mine' to confirm transaction"
                exit 0
            fi

            # Handle multiple amounts - separate transaction for each amount
            echo "🎯 Sending ${#AMOUNTS[@]} separate transactions from $WALLET to $DESTINATION_WALLET"
            TXIDS=()
            for i in "${!AMOUNTS[@]}"; do
                AMOUNT="${AMOUNTS[$i]}"
                # Get target address (reuses existing for addr-* wallets)
                TARGET_ADDRESS=$(get_target_address "$DESTINATION_WALLET")

                echo "  📤 Transaction $((i+1))/${#AMOUNTS[@]}: Sending $AMOUNT BTC to address $TARGET_ADDRESS"
                TXID=$(send_from_wallet "$WALLET" "$TARGET_ADDRESS" "$AMOUNT")
                TXIDS+=("$TXID")
                echo "     ✅ Transaction $((i+1)) sent: $TXID"
            done
            
            echo ""
            echo "🎉 All ${#AMOUNTS[@]} transactions sent successfully:"
            for i in "${!TXIDS[@]}"; do
                echo "  $((i+1)). ${AMOUNTS[$i]} BTC → ${TXIDS[$i]}"
            done
            echo "💡 Use '$0 mine' to confirm all transactions"
            exit 0
            ;;
        sent)
            # Execute sending command first
            $0 $WALLET sending "$@"
            # Then mine a block to confirm
            echo "⛏️  Mining 1 block to confirm all transactions..."
            mine_blocks 1
            echo "✅ All transactions confirmed in block"
            exit 0
            ;;
        balance)
            btc loadwallet "$WALLET" 2>/dev/null || true
            BALANCE=$(btc_wallet "$WALLET" getbalance)
            echo "$WALLET wallet balance: $BALANCE BTC"
            exit 0
            ;;
        address)
            btc loadwallet "$WALLET" 2>/dev/null || true
            ADDRESS=$(btc_wallet "$WALLET" getnewaddress "" "$(get_address_type "$WALLET")")
            echo "New $WALLET address: $ADDRESS"
            exit 0
            ;;
        fund)
            TARGET_ADDRESS="$1"
            AMOUNT="${2:-1.0}"
            if [ -z "$TARGET_ADDRESS" ]; then
                echo "Usage: $0 $WALLET fund <address> [amount=1.0]"
                exit 1
            fi
            btc loadwallet "$WALLET" 2>/dev/null || true
            echo "Funding address $TARGET_ADDRESS with $AMOUNT BTC from $WALLET..."
            TXID=$(btc_wallet "$WALLET" sendtoaddress "$TARGET_ADDRESS" "$AMOUNT")
            echo "Transaction: $TXID"
            echo "💡 Use '$0 mine' to confirm transaction"
            exit 0
            ;;
        rbf)
            TXID="$1"
            if [ -z "$TXID" ]; then
                echo "Usage: $0 $WALLET rbf <txid>"
                exit 1
            fi
            echo "🔄 Bumping fee for transaction $TXID (automatic fee calculation)..."
            btc loadwallet "$WALLET" 2>/dev/null || true
            
            # Use bumpfee without explicit fee_rate to let Bitcoin Core automatically calculate
            # This will increase the fee by the minimum required increment
            RESULT=$(btc_wallet "$WALLET" bumpfee "$TXID" 2>&1)
            
            if echo "$RESULT" | jq -e '.txid' > /dev/null 2>&1; then
                NEW_TXID=$(echo "$RESULT" | jq -r '.txid')
                OLD_FEE=$(echo "$RESULT" | jq -r '.origfee')
                NEW_FEE=$(echo "$RESULT" | jq -r '.fee')
                echo "✅ RBF replacement successful!"
                echo "   Original TXID: $TXID"
                echo "   New TXID: $NEW_TXID"
                echo "   Original fee: $OLD_FEE BTC"
                echo "   New fee: $NEW_FEE BTC"
                echo "💡 Use '$0 mine' to confirm when ready"
            else
                echo "❌ RBF failed: $RESULT"
                echo "💡 Common reasons:"
                echo "   - Transaction already confirmed"
                echo "   - Transaction was not RBF-enabled"
                echo "   - Fee rate not higher than original"
            fi
            exit 0
            ;;
        cpfp)
            PARENT_TXID="$1"
            cpfp_for_wallet "$WALLET" "$PARENT_TXID"
            exit 0
            ;;
        consolidate)
            echo "🔄 Consolidating 2 smallest UTXOs for $WALLET..."
            btc loadwallet "$WALLET" 2>/dev/null || true
            
            # Check if wallet exists
            if ! btc_wallet "$WALLET" getwalletinfo >/dev/null 2>&1; then
                echo "❌ $WALLET wallet not found. Run '$0 init' first"
                exit 1
            fi
            
            # Get UTXOs and sort by amount (smallest first)
            UTXOS=$(btc_wallet "$WALLET" listunspent | jq -r '.[] | "\(.amount) \(.txid) \(.vout)"' | sort -n)
            UTXO_COUNT=$(echo "$UTXOS" | wc -l | tr -d ' ')
            
            if [ "$UTXO_COUNT" -lt 2 ]; then
                echo "❌ $WALLET needs at least 2 UTXOs to consolidate. Current UTXOs: $UTXO_COUNT"
                echo "💡 Fund $WALLET with multiple transactions first"
                exit 1
            fi
            
            # Get the 2 smallest UTXOs
            UTXO1=$(echo "$UTXOS" | head -1)
            UTXO2=$(echo "$UTXOS" | head -2 | tail -1)
            
            AMOUNT1=$(echo "$UTXO1" | cut -d' ' -f1)
            TXID1=$(echo "$UTXO1" | cut -d' ' -f2)
            VOUT1=$(echo "$UTXO1" | cut -d' ' -f3)
            
            AMOUNT2=$(echo "$UTXO2" | cut -d' ' -f1)
            TXID2=$(echo "$UTXO2" | cut -d' ' -f2)
            VOUT2=$(echo "$UTXO2" | cut -d' ' -f3)
            
            echo "   📍 UTXO 1: $AMOUNT1 BTC (txid: $TXID1, vout: $VOUT1)"
            echo "   📍 UTXO 2: $AMOUNT2 BTC (txid: $TXID2, vout: $VOUT2)"
            
            # Calculate total amount minus fee using awk with C locale for proper decimal format
            TOTAL_AMOUNT=$(LC_NUMERIC=C awk "BEGIN {printf \"%.8f\", $AMOUNT1 + $AMOUNT2}")
            CONSOLIDATE_AMOUNT=$(LC_NUMERIC=C awk "BEGIN {printf \"%.8f\", $AMOUNT1 + $AMOUNT2 - 0.0001}")
            
            echo "   💰 Total: $TOTAL_AMOUNT BTC → $CONSOLIDATE_AMOUNT BTC (0.0001 BTC fee)"
            
            # Get new change address for consolidation (internal keychain)
            CONSOLIDATE_ADDRESS=$(btc_wallet "$WALLET" getrawchangeaddress "$(get_address_type "$WALLET")")
            echo "   🎯 Consolidating to: $CONSOLIDATE_ADDRESS"
            
            # Create raw transaction
            INPUTS="[{\"txid\":\"$TXID1\",\"vout\":$VOUT1},{\"txid\":\"$TXID2\",\"vout\":$VOUT2}]"
            OUTPUTS="{\"$CONSOLIDATE_ADDRESS\":$CONSOLIDATE_AMOUNT}"
            
            echo "   🔧 Creating consolidation transaction..."
            RAW_TX=$(btc_wallet "$WALLET" createrawtransaction "$INPUTS" "$OUTPUTS")
            
            if [ -z "$RAW_TX" ]; then
                echo "❌ Failed to create raw transaction"
                exit 1
            fi
            
            # Sign the transaction
            echo "   ✍️  Signing transaction..."
            SIGNED_TX=$(btc_wallet "$WALLET" signrawtransactionwithwallet "$RAW_TX")
            SIGNED_HEX=$(echo "$SIGNED_TX" | jq -r '.hex')
            SIGN_COMPLETE=$(echo "$SIGNED_TX" | jq -r '.complete')
            
            if [ "$SIGN_COMPLETE" != "true" ]; then
                echo "❌ Failed to sign transaction"
                echo "Signing result: $SIGNED_TX"
                exit 1
            fi
            
            # Broadcast the transaction
            echo "   📡 Broadcasting consolidation transaction..."
            CONSOLIDATE_TXID=$(btc sendrawtransaction "$SIGNED_HEX")
            
            if [ -z "$CONSOLIDATE_TXID" ]; then
                echo "❌ Failed to broadcast transaction"
                exit 1
            fi
            
            echo "   ✅ Consolidation transaction created: $CONSOLIDATE_TXID"
            echo "   💰 Consolidated: $CONSOLIDATE_AMOUNT BTC"
            echo "   🎯 Address: $CONSOLIDATE_ADDRESS"
            echo ""
            echo "🔗 Consolidation Summary:"
            echo "   Input 1: $AMOUNT1 BTC from $TXID1:$VOUT1"
            echo "   Input 2: $AMOUNT2 BTC from $TXID2:$VOUT2"
            echo "   Output:  $CONSOLIDATE_AMOUNT BTC to $CONSOLIDATE_ADDRESS"
            echo "   Fee:     0.0001 BTC"
            echo ""
            echo "💡 Use '$0 mine 1' to confirm the consolidation"
            exit 0
            ;;
        *)
            echo "Unknown subcommand for $WALLET: $SUBCMD"
            exit 1
            ;;
    esac
fi
# --- End new multi-word command parsing ---

case "$1" in
    "mode")
        MODE="$2"
        if [[ "$MODE" != "self-hosted" && "$MODE" != "cloud" ]]; then
            echo "Usage: $0 mode [self-hosted|cloud]"
            echo "  self-hosted - Single-user mode without authentication"
            echo "  cloud       - Multi-user mode with authentication and Stripe billing"
            exit 1
        fi

        # Kill running backend and frontend
        kill_servers

        # Set values based on mode
        if [[ "$MODE" == "self-hosted" ]]; then
            DATA_DIR="./database/self-hosted"
        else
            DATA_DIR="./database/cloud"
        fi

        # Update backend .env
        BACKEND_ENV="../backend/.env"
        if [[ -f "$BACKEND_ENV" ]]; then
            sed -i '' "s/^CANARY_MODE=.*/CANARY_MODE=$MODE/" "$BACKEND_ENV"
            sed -i '' "s|^CANARY_DATA_DIR=.*|CANARY_DATA_DIR=$DATA_DIR|" "$BACKEND_ENV"
            echo "Updated $BACKEND_ENV:"
            echo "  CANARY_MODE=$MODE"
            echo "  CANARY_DATA_DIR=$DATA_DIR"
        else
            echo "Warning: $BACKEND_ENV not found"
        fi

        # Update frontend .env.local
        FRONTEND_ENV="../frontend/.env.local"
        if [[ -f "$FRONTEND_ENV" ]]; then
            sed -i '' "s/^NEXT_PUBLIC_CANARY_MODE=.*/NEXT_PUBLIC_CANARY_MODE=$MODE/" "$FRONTEND_ENV"
            echo "Updated $FRONTEND_ENV:"
            echo "  NEXT_PUBLIC_CANARY_MODE=$MODE"
        else
            echo "Warning: $FRONTEND_ENV not found"
        fi

        echo ""
        echo "Mode switched to: $MODE"
        echo "Start backend and frontend to apply changes."
        ;;
    "start")
        echo "Starting Bitcoin regtest environment with Docker..."
        docker-compose up -d
        
        # Wait for Bitcoin Core to be ready
        echo "Waiting for Bitcoin Core to start..."
        timeout=60
        while [ $timeout -gt 0 ]; do
            if docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 getblockchaininfo > /dev/null 2>&1; then
                echo "✅ Bitcoin Core is ready"
                break
            fi
            sleep 1
            timeout=$((timeout-1))
        done
        
        if [ $timeout -eq 0 ]; then
            echo "❌ Bitcoin Core failed to start within 60 seconds"
            echo "Checking logs..."
            docker logs bitcoind-regtest | tail -10
            exit 1
        fi
        
        # Wait for Fulcrum to be ready
        echo "Waiting for Fulcrum Electrum server to start..."
        timeout=120
        while [ $timeout -gt 0 ]; do
            if nc -z localhost 50001 2>/dev/null; then
                echo "✅ Fulcrum Electrum server is ready"
                break
            fi
            sleep 2
            timeout=$((timeout-2))
        done
        
        if [ $timeout -le 0 ]; then
            echo "⚠️  Fulcrum may still be starting (can take a few minutes on first run)"
            echo "Check logs with: ./docker-utils.sh logs fulcrum"
        fi

        # Wait for ntfy server to start
        echo "Waiting for ntfy server to start..."
        timeout=30
        while [ $timeout -gt 0 ]; do
            if curl -s http://localhost:2586/v1/health > /dev/null 2>&1; then
                echo "✅ ntfy server is ready (auth: deny-all)"
                break
            fi
            sleep 1
            timeout=$((timeout-1))
        done

        if [ $timeout -le 0 ]; then
            echo "⚠️  ntfy server may still be starting"
        else
            # Set up ntfy test user and access token
            echo "Setting up ntfy test credentials..."
            printf "testpassword\ntestpassword\n" | docker exec -i ntfy-regtest ntfy user add --role=admin testuser 2>/dev/null || true
            # Check if token already exists, otherwise create one
            NTFY_TOKEN=$(docker exec ntfy-regtest ntfy token list testuser 2>/dev/null | grep -o 'tk_[a-zA-Z0-9_]*' | head -1)
            if [ -z "$NTFY_TOKEN" ]; then
                NTFY_TOKEN=$(docker exec ntfy-regtest ntfy token add -l "Dev token" testuser 2>&1 | grep -o 'tk_[a-zA-Z0-9_]*' | head -1)
            fi
        fi

        echo ""
        echo "🚀 Bitcoin regtest environment is running!"
        echo "Bitcoin RPC: localhost:18443"
        echo "Fulcrum Electrum server: localhost:50001"
        echo "ntfy server: http://localhost:2586"
        echo "  User: testuser / testpassword"
        if [ -n "$NTFY_TOKEN" ]; then
            echo "  Token: $NTFY_TOKEN"
        fi
        echo "Set BITCOIN_NETWORK=regtest in your environment"
        echo ""
        echo "💡 Next: $0 init (creates wallets and adds to backend)"
        ;;
    
    "init")
        # Start infrastructure if not already running
        if ! btc getblockchaininfo > /dev/null 2>&1; then
            echo "🔧 Bitcoin Core not running — starting infrastructure first..."
            $0 start
            echo ""
        fi

        echo "🏦 Setting up development wallets..."

        # Check if Bitcoin Core is running
        if ! btc getblockchaininfo > /dev/null 2>&1; then
            echo "❌ Bitcoin Core is not running."
            exit 1
        fi
        
        # Shared tprv keys for deterministic wallets
        FUNDED_TPRV="tprv8ZgxMBicQKsPe6D3wxZPHxy7JEMBwjxiimYXSvM8aRaqZmDvU7Jec5c8aSB8rCzFmAP6aEjPhUmiXm2KB7XUzkApX2prmDHcQsUQY5DsxJw"
        EMPTY_TPRV="tprv8ZgxMBicQKsPd3nsWkJrKQgXk22UnGFHFPDWNCeTjvzeY9LjRYdi3RNX1NfkCwj4mD1YTsNPCCtGPTTQkxn14oLKNg6vuVkChn55qa5rC7K"

        # Helper: create a descriptor wallet with a given name, tprv, descriptor wrapper, and BIP path
        create_descriptor_wallet() {
            local wallet_name="$1"
            local tprv="$2"
            local wrapper="$3"   # e.g. "wpkh" "pkh" "sh_wpkh" "tr"
            local bip_path="$4"  # e.g. "84h/1h/0h"

            echo "📋 Creating $wallet_name wallet..."
            btc unloadwallet "$wallet_name" 2>/dev/null || true

            set +e
            CREATE_RESULT=$(btc -named createwallet wallet_name="$wallet_name" disable_private_keys=false blank=true passphrase="" avoid_reuse=false descriptors=true 2>&1)
            CREATE_EXIT_CODE=$?
            set -e

            if echo "$CREATE_RESULT" | grep -q "already exists"; then
                echo "   ✅ $wallet_name wallet exists, loading..."
                btc loadwallet "$wallet_name" >/dev/null 2>&1 || true
            elif [ $CREATE_EXIT_CODE -eq 0 ]; then
                echo "   ✅ $wallet_name blank wallet created"

                # Build raw descriptors (without checksum) based on wrapper type
                local ext_raw int_raw
                case "$wrapper" in
                    wpkh)
                        ext_raw="wpkh($tprv/$bip_path/0/*)"
                        int_raw="wpkh($tprv/$bip_path/1/*)"
                        ;;
                    pkh)
                        ext_raw="pkh($tprv/$bip_path/0/*)"
                        int_raw="pkh($tprv/$bip_path/1/*)"
                        ;;
                    sh_wpkh)
                        ext_raw="sh(wpkh($tprv/$bip_path/0/*))"
                        int_raw="sh(wpkh($tprv/$bip_path/1/*))"
                        ;;
                    tr)
                        ext_raw="tr($tprv/$bip_path/0/*)"
                        int_raw="tr($tprv/$bip_path/1/*)"
                        ;;
                esac

                # Compute checksums at runtime
                local ext_checksum int_checksum
                ext_checksum=$(btc getdescriptorinfo "$ext_raw" | jq -r '.checksum')
                int_checksum=$(btc getdescriptorinfo "$int_raw" | jq -r '.checksum')

                btc_wallet "$wallet_name" importdescriptors "[
                  {\"desc\": \"${ext_raw}#${ext_checksum}\", \"timestamp\": \"now\", \"active\": true, \"internal\": false, \"range\": [0, 999]},
                  {\"desc\": \"${int_raw}#${int_checksum}\", \"timestamp\": \"now\", \"active\": true, \"internal\": true, \"range\": [0, 999]}
                ]" >/dev/null 2>&1
                echo "   ✅ $wallet_name wallet seeded with deterministic descriptors"
            else
                echo "   ❌ Failed to create $wallet_name wallet: $CREATE_RESULT"
                exit 1
            fi
        }

        # Helper: extract multipath descriptor for a wallet given its descriptor prefix filter
        get_multipath_descriptor() {
            local wallet_name="$1"
            local jq_filter="$2"  # jq select filter for the receive descriptor

            local descriptors receive_desc multipath_raw checksum_info checksum
            descriptors=$(btc_wallet "$wallet_name" listdescriptors)
            receive_desc=$(echo "$descriptors" | jq -r ".descriptors[] | select($jq_filter) | .desc")
            multipath_raw=$(echo "$receive_desc" | sed 's|/0/\*|/<0;1>/\*|' | sed 's/#[^#]*$//')
            checksum_info=$(btc getdescriptorinfo "$multipath_raw")
            checksum=$(echo "$checksum_info" | jq -r '.checksum')
            echo "$multipath_raw#$checksum"
        }

        # Create funded descriptor wallets (using FUNDED_TPRV)
        create_descriptor_wallet "segwit-desc"  "$FUNDED_TPRV" "wpkh"    "84h/1h/0h"
        create_descriptor_wallet "legacy-desc"  "$FUNDED_TPRV" "pkh"     "44h/1h/0h"
        create_descriptor_wallet "nested-desc"  "$FUNDED_TPRV" "sh_wpkh" "49h/1h/0h"
        create_descriptor_wallet "taproot-desc" "$FUNDED_TPRV" "tr"      "86h/1h/0h"

        # Get multipath descriptors for funded wallets
        SEGWIT_DESC_DESCRIPTOR=$(get_multipath_descriptor "segwit-desc" '.desc | startswith("wpkh(") and contains("/0/*")')
        LEGACY_DESC_DESCRIPTOR=$(get_multipath_descriptor "legacy-desc" '.desc | startswith("pkh(") and contains("/0/*")')
        NESTED_DESC_DESCRIPTOR=$(get_multipath_descriptor "nested-desc" '.desc | startswith("sh(wpkh(") and contains("/0/*")')
        TAPROOT_DESC_DESCRIPTOR=$(get_multipath_descriptor "taproot-desc" '.desc | startswith("tr(") and contains("/0/*")')

        # Create empty descriptor wallets (using EMPTY_TPRV)
        create_descriptor_wallet "segwit-empty"  "$EMPTY_TPRV" "wpkh"    "84h/1h/0h"
        create_descriptor_wallet "legacy-empty"  "$EMPTY_TPRV" "pkh"     "44h/1h/0h"
        create_descriptor_wallet "nested-empty"  "$EMPTY_TPRV" "sh_wpkh" "49h/1h/0h"
        create_descriptor_wallet "taproot-empty" "$EMPTY_TPRV" "tr"      "86h/1h/0h"

        # Get multipath descriptors for empty wallets
        SEGWIT_EMPTY_DESCRIPTOR=$(get_multipath_descriptor "segwit-empty" '.desc | startswith("wpkh(") and contains("/0/*")')
        LEGACY_EMPTY_DESCRIPTOR=$(get_multipath_descriptor "legacy-empty" '.desc | startswith("pkh(") and contains("/0/*")')
        NESTED_EMPTY_DESCRIPTOR=$(get_multipath_descriptor "nested-empty" '.desc | startswith("sh(wpkh(") and contains("/0/*")')
        TAPROOT_EMPTY_DESCRIPTOR=$(get_multipath_descriptor "taproot-empty" '.desc | startswith("tr(") and contains("/0/*")')
        
        # Create Charlie wallet (deterministic)
        echo "📋 Creating Charlie wallet..."
        btc unloadwallet "charlie" 2>/dev/null || true
        
        set +e  # Temporarily disable exit on error
        CREATE_RESULT=$(btc -named createwallet wallet_name="charlie" disable_private_keys=false blank=true passphrase="" avoid_reuse=false descriptors=true 2>&1)
        CREATE_EXIT_CODE=$?
        set -e
        
        if echo "$CREATE_RESULT" | grep -q "already exists"; then
            echo "   ✅ Charlie wallet exists, loading..."
            btc loadwallet "charlie" >/dev/null 2>&1 || true
        elif [ $CREATE_EXIT_CODE -eq 0 ]; then
            echo "   ✅ Charlie blank wallet created"
            
            # Import deterministic descriptors for Charlie (regtest vprv keys)
            btc_charlie importdescriptors '[{"desc": "wpkh(tprv8ZgxMBicQKsPd7Uf69XL1XwhmjHopUGep8GuEiJDZmbQz6o58LninorQAfcKZWARbtRtfnLcJ5MQ2AtHcQJCCRUcMRvmDUjyEmNUWwx8UbK/84h/1h/0h/0/*)#pe5sgqha", "timestamp": "now", "active": true, "internal": false, "range": [0, 999]}, {"desc": "wpkh(tprv8ZgxMBicQKsPd7Uf69XL1XwhmjHopUGep8GuEiJDZmbQz6o58LninorQAfcKZWARbtRtfnLcJ5MQ2AtHcQJCCRUcMRvmDUjyEmNUWwx8UbK/84h/1h/0h/1/*)#sd334489", "timestamp": "now", "active": true, "internal": true, "range": [0, 999]}]' >/dev/null 2>&1
            echo "   ✅ Charlie wallet seeded with deterministic descriptors"
        else
            echo "   ❌ Failed to create Charlie wallet: $CREATE_RESULT"
            exit 1
        fi
        
        # Get Charlie descriptor and address
        CHARLIE_DESCRIPTORS=$(btc_wallet charlie listdescriptors)
        CHARLIE_RECEIVE_DESC=$(echo "$CHARLIE_DESCRIPTORS" | jq -r '.descriptors[] | select(.desc | startswith("wpkh") and contains("/0/*")) | .desc')
        CHARLIE_MULTIPATH_RAW=$(echo "$CHARLIE_RECEIVE_DESC" | sed 's|/0/\*|/<0;1>/\*|' | sed 's/#[^#]*$//')
        # Get proper checksum for multipath descriptor from Bitcoin Core
        CHARLIE_CHECKSUM_INFO=$(btc getdescriptorinfo "$CHARLIE_MULTIPATH_RAW")
        CHARLIE_CHECKSUM=$(echo "$CHARLIE_CHECKSUM_INFO" | jq -r '.checksum')
        CHARLIE_DESCRIPTOR="$CHARLIE_MULTIPATH_RAW#$CHARLIE_CHECKSUM"

        # Create Bacon wallet (deterministic - for demo account)
        echo "📋 Creating Bacon wallet..."
        btc unloadwallet "bacon" 2>/dev/null || true

        set +e  # Temporarily disable exit on error
        CREATE_RESULT=$(btc -named createwallet wallet_name="bacon" disable_private_keys=false blank=true passphrase="" avoid_reuse=false descriptors=true 2>&1)
        CREATE_EXIT_CODE=$?
        set -e

        if echo "$CREATE_RESULT" | grep -q "already exists"; then
            echo "   ✅ Bacon wallet exists, loading..."
            btc loadwallet "bacon" >/dev/null 2>&1 || true
        elif [ $CREATE_EXIT_CODE -eq 0 ]; then
            echo "   ✅ Bacon blank wallet created"

            # Import deterministic descriptors for Bacon (bacon bacon bacon... mnemonic)
            btc_wallet bacon importdescriptors '[
              {
                "desc": "wpkh(tprv8ZgxMBicQKsPeh9dSitM82FU7Fz3ZgPkKmmovAr2aqwauAMVgjcEkZBb2etBtRPZ8XYVm7shxcKwVaDus7T5kauJXVsqAfzM4Tty13rRjAG/84h/1h/0h/0/*)#ggkkr2kq",
                "timestamp": "now",
                "active": true,
                "internal": false,
                "range": [0, 999]
              },
              {
                "desc": "wpkh(tprv8ZgxMBicQKsPeh9dSitM82FU7Fz3ZgPkKmmovAr2aqwauAMVgjcEkZBb2etBtRPZ8XYVm7shxcKwVaDus7T5kauJXVsqAfzM4Tty13rRjAG/84h/1h/0h/1/*)#eunh7lxc",
                "timestamp": "now",
                "active": true,
                "internal": true,
                "range": [0, 999]
              }
            ]' >/dev/null 2>&1
            echo "   ✅ Bacon wallet seeded with deterministic descriptors"
        else
            echo "   ❌ Failed to create Bacon wallet: $CREATE_RESULT"
            exit 1
        fi

        # Get Bacon descriptor
        BACON_DESCRIPTORS=$(btc_wallet bacon listdescriptors)
        BACON_RECEIVE_DESC=$(echo "$BACON_DESCRIPTORS" | jq -r '.descriptors[] | select(.desc | startswith("wpkh") and contains("/0/*")) | .desc')
        BACON_MULTIPATH_RAW=$(echo "$BACON_RECEIVE_DESC" | sed 's|/0/\*|/<0;1>/\*|' | sed 's/#[^#]*$//')
        # Get proper checksum for multipath descriptor from Bitcoin Core
        BACON_CHECKSUM_INFO=$(btc getdescriptorinfo "$BACON_MULTIPATH_RAW")
        BACON_CHECKSUM=$(echo "$BACON_CHECKSUM_INFO" | jq -r '.checksum')
        BACON_DESCRIPTOR="$BACON_MULTIPATH_RAW#$BACON_CHECKSUM"

        # Create Miner wallet
        echo "📋 Creating Miner wallet..."
        btc unloadwallet "miner" 2>/dev/null || true
        
        set +e  # Temporarily disable exit on error
        CREATE_RESULT=$(btc -named createwallet wallet_name="miner" disable_private_keys=false blank=false passphrase="" avoid_reuse=false descriptors=true 2>&1)
        CREATE_EXIT_CODE=$?
        set -e
        
        if echo "$CREATE_RESULT" | grep -q "already exists"; then
            echo "   ✅ Miner wallet exists, loading..."
            btc loadwallet "miner" >/dev/null 2>&1 || true
        elif [ $CREATE_EXIT_CODE -eq 0 ]; then
            echo "   ✅ Miner wallet created"
        else
            echo "   ❌ Failed to create Miner wallet: $CREATE_RESULT"
            exit 1
        fi
        
        # Get Miner descriptor and address (for background operations)
        MINER_DESCRIPTORS=$(btc_wallet miner listdescriptors)
        MINER_RECEIVE_DESC=$(echo "$MINER_DESCRIPTORS" | jq -r '.descriptors[] | select(.desc | startswith("wpkh") and contains("/0/*")) | .desc')
        MINER_MULTIPATH_RAW=$(echo "$MINER_RECEIVE_DESC" | sed 's|/0/\*|/<0;1>/\*|' | sed 's/#[^#]*$//')
        # Get proper checksum for multipath descriptor from Bitcoin Core
        MINER_CHECKSUM_INFO=$(btc getdescriptorinfo "$MINER_MULTIPATH_RAW")
        MINER_CHECKSUM=$(echo "$MINER_CHECKSUM_INFO" | jq -r '.checksum')
        MINER_DESCRIPTOR="$MINER_MULTIPATH_RAW#$MINER_CHECKSUM"
        # Get a fresh address for mining (only used for background operations)
        MINER_ADDRESS=$(btc_wallet miner getnewaddress)
        
        # Fund descriptor wallets
        echo "💰 Funding descriptor wallets..."
        BLOCK_COUNT=$(btc getblockcount 2>/dev/null || echo "0")

        if [ "$BLOCK_COUNT" -lt 104 ]; then
            echo "   ⛏️  Mining blocks and transferring funds..."
            # Mine 103 blocks to Miner (150 BTC total)
            btc generatetoaddress 103 "$MINER_ADDRESS" >/dev/null 2>&1

            # Fund segwit-desc with distributed strategy (1 BTC across 31 addresses)
            echo "   📍 Generating addresses for segwit-desc distributed funding..."
            RECIPIENTS="{"
            SEGWIT_ADDR_5=$(btc_wallet "segwit-desc" getnewaddress)
            RECIPIENTS="${RECIPIENTS}\"$SEGWIT_ADDR_5\":0.5"
            for i in {1..5}; do
                SEGWIT_ADDR_05=$(btc_wallet "segwit-desc" getnewaddress)
                RECIPIENTS="${RECIPIENTS},\"$SEGWIT_ADDR_05\":0.05"
            done
            for i in {1..25}; do
                SEGWIT_ADDR_01=$(btc_wallet "segwit-desc" getnewaddress)
                RECIPIENTS="${RECIPIENTS},\"$SEGWIT_ADDR_01\":0.01"
            done
            RECIPIENTS="${RECIPIENTS}}"
            echo "   💸 Creating single transaction with multiple outputs..."
            echo "   📊 Distribution: 1×0.5 BTC + 5×0.05 BTC + 25×0.01 BTC = 1 BTC across 31 addresses"
            btc_miner sendmany "" "$RECIPIENTS" >/dev/null 2>&1
            btc generatetoaddress 1 "$MINER_ADDRESS" >/dev/null 2>&1
            echo "   ✅ segwit-desc funded with 1 BTC (distributed across 31 addresses)"

            # Fund legacy-desc, nested-desc, taproot-desc with 0.123 BTC each
            for FUNDED_WALLET in legacy-desc nested-desc taproot-desc; do
                FUNDED_ADDR=$(btc_wallet "$FUNDED_WALLET" getnewaddress "" "$(get_address_type "$FUNDED_WALLET")")
                btc_miner sendtoaddress "$FUNDED_ADDR" 0.123 >/dev/null 2>&1
                echo "   ✅ $FUNDED_WALLET funded with 0.123 BTC"
            done
            btc generatetoaddress 1 "$MINER_ADDRESS" >/dev/null 2>&1
        else
            echo "   ✅ Descriptor wallets already funded"
        fi
        
        # Fund Charlie wallet at index 250
        echo "💰 Funding Charlie wallet at index 250..."
        CHARLIE_BALANCE=$(btc_wallet charlie getbalance 2>/dev/null || echo "0")
        
        if [ "$(echo "$CHARLIE_BALANCE == 0" | bc -l 2>/dev/null || echo "1")" -eq 1 ]; then
            echo "   📍 Generating addresses up to index 250..."
            
            # Generate addresses up to index 250 (0-250 = 251 addresses)
            for i in {0..250}; do
                ADDR=$(btc_charlie getnewaddress 2>/dev/null)
                if [ $i -eq 250 ]; then
                    CHARLIE_ADDR_250="$ADDR"
                    echo "   🎯 Address at index 250: $CHARLIE_ADDR_250"
                fi
                # Show progress every 50 addresses
                if [ $((i % 50)) -eq 0 ] && [ $i -gt 0 ]; then
                    echo "   📍 Generated addresses 0-$i..."
                fi
            done
            
            # Send 0.5 BTC to Charlie's address at index 250
            echo "   💸 Sending 0.5 BTC to Charlie at index 250..."
            CHARLIE_TXID=$(btc_miner sendtoaddress "$CHARLIE_ADDR_250" 0.5)
            
            # Mine 1 block to confirm Charlie's transaction
            btc generatetoaddress 1 "$MINER_ADDRESS" >/dev/null 2>&1
            echo "   ✅ Charlie funded with 0.5 BTC at index 250"
            echo "   📋 Transaction: $CHARLIE_TXID"
        else
            echo "   ✅ Charlie already funded"
        fi

        # Fund Bacon wallet and create transaction history
        echo "💰 Funding Bacon wallet (for demo account)..."
        BACON_BALANCE=$(btc_wallet bacon getbalance 2>/dev/null || echo "0")

        if [ "$(echo "$BACON_BALANCE == 0" | bc -l 2>/dev/null || echo "1")" -eq 1 ]; then
            echo "   💸 Sending 0.1 BTC to Bacon wallet..."
            BACON_ADDR=$(btc_wallet bacon getnewaddress)
            BACON_TX1=$(btc_miner sendtoaddress "$BACON_ADDR" 0.1)
            btc generatetoaddress 1 "$MINER_ADDRESS" >/dev/null 2>&1
            echo "   ✅ Bacon funded with 0.1 BTC"

            # Create transaction history by exchanging with segwit-desc
            echo "   📜 Creating transaction history..."

            # Bacon sends 0.02 BTC to segwit-desc
            SEGWIT_ADDR=$(btc_wallet "segwit-desc" getnewaddress)
            BACON_TX2=$(btc_wallet bacon sendtoaddress "$SEGWIT_ADDR" 0.02)
            btc generatetoaddress 1 "$MINER_ADDRESS" >/dev/null 2>&1
            echo "   ✅ Bacon → segwit-desc: 0.02 BTC"

            # segwit-desc sends 0.015 BTC back to Bacon
            BACON_ADDR2=$(btc_wallet bacon getnewaddress)
            SEGWIT_TX1=$(btc_wallet "segwit-desc" sendtoaddress "$BACON_ADDR2" 0.015)
            btc generatetoaddress 1 "$MINER_ADDRESS" >/dev/null 2>&1
            echo "   ✅ segwit-desc → Bacon: 0.015 BTC"

            # Bacon sends 0.01 BTC to segwit-desc again
            SEGWIT_ADDR2=$(btc_wallet "segwit-desc" getnewaddress)
            BACON_TX3=$(btc_wallet bacon sendtoaddress "$SEGWIT_ADDR2" 0.01)
            btc generatetoaddress 1 "$MINER_ADDRESS" >/dev/null 2>&1
            echo "   ✅ Bacon → segwit-desc: 0.01 BTC"

            echo "   ✅ Transaction history created (4 transactions)"
        else
            echo "   ✅ Bacon already funded"
        fi

        # Create Satoshi (Genesis) wallet (deterministic single-address - for sample wallet onboarding)
        # Mimics prod where the raw Satoshi pubkey creates a single-address pk() wallet
        SATOSHI_GENESIS_TPRV="tprv8ZgxMBicQKsPeZjnkSokuUQsdrWJ83bXz4Eqm1aVDkDSSJ9BqHGMsjxpBEb3n6V9X3u6ThQQ1dmsvigtXWxvP8YJL9FST4DighMqnHtmFTo"
        # Deterministic first address derived from this tprv (BIP84 path 84h/1h/0h/0/0)
        SATOSHI_GENESIS_ADDRESS="bcrt1q20lu6ldqtssq7y7ewarlamlzldnmyk5w4n3e97"
        echo "📋 Creating Satoshi (Genesis) wallet..."
        btc unloadwallet "satoshi-genesis" 2>/dev/null || true

        set +e
        CREATE_RESULT=$(btc -named createwallet wallet_name="satoshi-genesis" disable_private_keys=false blank=true passphrase="" avoid_reuse=false descriptors=true 2>&1)
        CREATE_EXIT_CODE=$?
        set -e

        if echo "$CREATE_RESULT" | grep -q "already exists"; then
            echo "   ✅ Satoshi (Genesis) wallet exists, loading..."
            btc loadwallet "satoshi-genesis" >/dev/null 2>&1 || true
        elif [ $CREATE_EXIT_CODE -eq 0 ]; then
            echo "   ✅ Satoshi (Genesis) blank wallet created"

            # Import deterministic descriptors (needed to own the address for funding)
            SATOSHI_EXT_RAW="wpkh($SATOSHI_GENESIS_TPRV/84h/1h/0h/0/*)"
            SATOSHI_INT_RAW="wpkh($SATOSHI_GENESIS_TPRV/84h/1h/0h/1/*)"
            SATOSHI_EXT_CHECKSUM=$(btc getdescriptorinfo "$SATOSHI_EXT_RAW" | jq -r '.checksum')
            SATOSHI_INT_CHECKSUM=$(btc getdescriptorinfo "$SATOSHI_INT_RAW" | jq -r '.checksum')

            btc_wallet "satoshi-genesis" importdescriptors "[
              {\"desc\": \"${SATOSHI_EXT_RAW}#${SATOSHI_EXT_CHECKSUM}\", \"timestamp\": \"now\", \"active\": true, \"internal\": false, \"range\": [0, 999]},
              {\"desc\": \"${SATOSHI_INT_RAW}#${SATOSHI_INT_CHECKSUM}\", \"timestamp\": \"now\", \"active\": true, \"internal\": true, \"range\": [0, 999]}
            ]" >/dev/null 2>&1
            echo "   ✅ Satoshi (Genesis) wallet seeded with deterministic descriptors"
        else
            echo "   ❌ Failed to create Satoshi (Genesis) wallet: $CREATE_RESULT"
            exit 1
        fi

        # Fund Satoshi (Genesis) wallet at its deterministic first address
        echo "💰 Funding Satoshi (Genesis) wallet..."
        SATOSHI_GENESIS_BALANCE=$(btc_wallet "satoshi-genesis" getbalance 2>/dev/null || echo "0")

        if [ "$(echo "$SATOSHI_GENESIS_BALANCE == 0" | bc -l 2>/dev/null || echo "1")" -eq 1 ]; then
            echo "   💸 Sending 0.5 BTC to Satoshi (Genesis) address..."
            btc_miner sendtoaddress "$SATOSHI_GENESIS_ADDRESS" 0.5 >/dev/null 2>&1
            btc generatetoaddress 1 "$MINER_ADDRESS" >/dev/null 2>&1
            echo "   ✅ Satoshi (Genesis) funded with 0.5 BTC at $SATOSHI_GENESIS_ADDRESS"
        else
            echo "   ✅ Satoshi (Genesis) already funded"
        fi

        # Create single-address wallets (one per address type, for testing address monitoring)
        echo "📋 Creating single-address wallets..."
        ADDR_TYPES=("legacy" "p2sh-segwit" "bech32" "bech32m")
        ADDR_LABELS=("P2PKH legacy" "P2SH nested segwit" "P2WPKH native segwit" "P2TR taproot")
        ADDR_WALLET_NAMES=("legacy-address" "p2sh-address" "segwit-address" "taproot-address")

        for i in "${!ADDR_TYPES[@]}"; do
            ADDR_TYPE="${ADDR_TYPES[$i]}"
            ADDR_LABEL="${ADDR_LABELS[$i]}"
            ADDR_WALLET_NAME="${ADDR_WALLET_NAMES[$i]}"

            set +e
            CREATE_RESULT=$(btc -named createwallet wallet_name="$ADDR_WALLET_NAME" descriptors=true 2>&1)
            CREATE_EXIT_CODE=$?
            set -e

            if echo "$CREATE_RESULT" | grep -q "already exists"; then
                btc loadwallet "$ADDR_WALLET_NAME" >/dev/null 2>&1 || true
            elif [ $CREATE_EXIT_CODE -ne 0 ]; then
                echo "   ❌ Failed to create $ADDR_WALLET_NAME: $CREATE_RESULT"
                continue
            fi

            # Check if already funded
            ADDR_BALANCE=$(btc_wallet "$ADDR_WALLET_NAME" getbalance 2>/dev/null || echo "0")
            if [ "$(echo "$ADDR_BALANCE == 0" | bc -l 2>/dev/null || echo "1")" -eq 1 ]; then
                ADDRESS=$(btc_wallet "$ADDR_WALLET_NAME" getnewaddress "" "$ADDR_TYPE")
                btc_miner sendtoaddress "$ADDRESS" 0.123 > /dev/null
                echo "   ✅ $ADDR_WALLET_NAME ($ADDR_LABEL): $ADDRESS — funded 0.123 BTC"
            else
                echo "   ✅ $ADDR_WALLET_NAME already funded ($ADDR_BALANCE BTC)"
            fi
        done

        # Mine to confirm address wallet funding
        btc generatetoaddress 1 "$MINER_ADDRESS" >/dev/null 2>&1

        # Show final balances
        echo ""
        echo "🎉 All wallets setup complete!"
        echo ""
        echo "📱 Funded descriptor wallets:"
        echo "   segwit-desc  (wpkh, 1 BTC distributed):  $SEGWIT_DESC_DESCRIPTOR"
        echo "   legacy-desc  (pkh, 0.123 BTC):           $LEGACY_DESC_DESCRIPTOR"
        echo "   nested-desc  (sh(wpkh), 0.123 BTC):      $NESTED_DESC_DESCRIPTOR"
        echo "   taproot-desc (tr, 0.123 BTC):             $TAPROOT_DESC_DESCRIPTOR"
        echo ""
        echo "📱 Empty descriptor wallets:"
        echo "   segwit-empty  (wpkh):     $SEGWIT_EMPTY_DESCRIPTOR"
        echo "   legacy-empty  (pkh):      $LEGACY_EMPTY_DESCRIPTOR"
        echo "   nested-empty  (sh(wpkh)): $NESTED_EMPTY_DESCRIPTOR"
        echo "   taproot-empty (tr):       $TAPROOT_EMPTY_DESCRIPTOR"
        echo ""
        echo "📱 Other wallets:"
        echo "   🎭 Charlie (funded - 0.5 BTC at index 250):  $CHARLIE_DESCRIPTOR"
        echo "   🥓 Bacon (demo - ~0.08 BTC):                 $BACON_DESCRIPTOR"
        echo "   🪙 Satoshi Genesis (sample - 0.5 BTC):       $SATOSHI_GENESIS_ADDRESS"
        echo ""
        echo "📍 Single addresses (for address monitoring):"
        for i in "${!ADDR_WALLET_NAMES[@]}"; do
            ADDR_WALLET_NAME="${ADDR_WALLET_NAMES[$i]}"
            ADDR_LABEL="${ADDR_LABELS[$i]}"
            btc loadwallet "$ADDR_WALLET_NAME" >/dev/null 2>&1 || true
            ADDR_LIST=$(btc_wallet "$ADDR_WALLET_NAME" listreceivedbyaddress 0 true)
            ADDRESS=$(echo "$ADDR_LIST" | jq -r '.[0].address')
            ADDR_BALANCE=$(btc_wallet "$ADDR_WALLET_NAME" getbalance)
            echo "   🔍 $ADDR_WALLET_NAME ($ADDR_LABEL): $ADDRESS ($ADDR_BALANCE BTC)"
        done
        echo ""

        # Set up BTCPay Server (before backend starts, so .env has correct values)
        echo ""
        $0 btcpay-setup

        # Add wallets to backend
        BACKEND_URL="http://localhost:3000"
        echo ""
        if curl -s --connect-timeout 2 --max-time 5 "$BACKEND_URL/api/wallets" > /dev/null 2>&1; then
            read -p "Add wallets to backend? (self-hosted mode only) (Y/n): " -n 1 -r
            echo
        else
            echo "⚠️  Backend not running at $BACKEND_URL — it must be running to add wallets."
            echo "   Start it with:  cd ../backend && cargo run"
            echo "   Note: This only works in self-hosted mode (unauthenticated API)."
            echo ""
            read -p "Press Enter when the backend is running, or type 'n' to skip: " -n 1 -r
            echo
        fi

        if [[ $REPLY =~ ^[Nn]$ ]]; then
            echo "💡 You can add wallets later with: $0 add-wallets-to-backend"
            exit 0
        fi

        echo "🔍 Checking backend at $BACKEND_URL..."
        if curl -s --connect-timeout 5 --max-time 10 "$BACKEND_URL/api/wallets" > /dev/null 2>&1; then
            echo "✅ Backend is running — adding wallets..."
            echo ""

            # Helper to add a wallet to the backend
            add_wallet_to_backend() {
                local name="$1"
                local descriptor="$2"
                local emoji="$3"

                RESPONSE=$(curl -s -X POST "$BACKEND_URL/api/wallets" \
                    -H "Content-Type: application/json" \
                    -d "{\"name\":\"$name\",\"descriptor\":\"$descriptor\"}")

                if echo "$RESPONSE" | jq -e '.wallet.checksum' > /dev/null 2>&1; then
                    CHECKSUM=$(echo "$RESPONSE" | jq -r '.wallet.checksum')
                    echo "   $emoji $name added (checksum: $CHECKSUM)"
                    return 0
                else
                    ERROR_MSG=$(echo "$RESPONSE" | jq -r '.error // "unknown error"')
                    echo "   $emoji $name: $ERROR_MSG"
                    return 1
                fi
            }

            add_wallet_to_backend "segwit-desc" "$SEGWIT_DESC_DESCRIPTOR" "📱"
            add_wallet_to_backend "legacy-desc" "$LEGACY_DESC_DESCRIPTOR" "📱"
            add_wallet_to_backend "nested-desc" "$NESTED_DESC_DESCRIPTOR" "📱"
            add_wallet_to_backend "taproot-desc" "$TAPROOT_DESC_DESCRIPTOR" "📱"
            add_wallet_to_backend "segwit-empty" "$SEGWIT_EMPTY_DESCRIPTOR" "📱"
            add_wallet_to_backend "legacy-empty" "$LEGACY_EMPTY_DESCRIPTOR" "📱"
            add_wallet_to_backend "nested-empty" "$NESTED_EMPTY_DESCRIPTOR" "📱"
            add_wallet_to_backend "taproot-empty" "$TAPROOT_EMPTY_DESCRIPTOR" "📱"
            add_wallet_to_backend "Charlie" "$CHARLIE_DESCRIPTOR" "🎭"

            # Add single-address wallets
            for i in "${!ADDR_WALLET_NAMES[@]}"; do
                ADDR_WALLET_NAME="${ADDR_WALLET_NAMES[$i]}"
                ADDR_LABEL="${ADDR_LABELS[$i]}"
                btc loadwallet "$ADDR_WALLET_NAME" >/dev/null 2>&1 || true
                ADDR_LIST=$(btc_wallet "$ADDR_WALLET_NAME" listreceivedbyaddress 0 true)
                ADDRESS=$(echo "$ADDR_LIST" | jq -r '.[0].address')
                add_wallet_to_backend "$ADDR_WALLET_NAME" "$ADDRESS" "🔍"
            done

            echo ""
            echo "🎉 Init complete! Check http://localhost:3001"
        else
            echo "⚠️  Backend not running — wallets not added to database"
            echo "💡 Start the backend and run: $0 add-wallets-to-backend"
        fi
        ;;
        
    "stop")
        echo "Stopping Bitcoin regtest environment..."
        docker-compose down
        echo "✅ Environment stopped"
        ;;
        
    "restart")
        echo "Restarting Bitcoin regtest environment..."
        docker-compose restart
        ;;
        
    "reset")
        echo "⚠️  This will stop containers and delete all blockchain data AND database data!"
        read -p "Are you sure? (y/N): " -n 1 -r
        echo
        if [[ $REPLY =~ ^[Yy]$ ]]; then
            echo "Resetting environment..."

            # Kill backend and frontend first so they don't hold database locks
            kill_servers

            # Try to unload wallets before reset (if Bitcoin is running)
            if btc getblockchaininfo > /dev/null 2>&1; then
                echo "Unloading test wallets..."
                btc unloadwallet "segwit-desc" 2>/dev/null || true
                btc unloadwallet "legacy-desc" 2>/dev/null || true
                btc unloadwallet "nested-desc" 2>/dev/null || true
                btc unloadwallet "taproot-desc" 2>/dev/null || true
                btc unloadwallet "segwit-empty" 2>/dev/null || true
                btc unloadwallet "legacy-empty" 2>/dev/null || true
                btc unloadwallet "nested-empty" 2>/dev/null || true
                btc unloadwallet "taproot-empty" 2>/dev/null || true
                btc unloadwallet "charlie" 2>/dev/null || true
                btc unloadwallet "bacon" 2>/dev/null || true
                btc unloadwallet "satoshi-genesis" 2>/dev/null || true
                btc unloadwallet "miner" 2>/dev/null || true
            fi
            
            # Stop containers and remove all volumes (includes wallet data)
            docker-compose down -v
            
            # Clean up regtest database folders completely
            echo "Cleaning up regtest databases..."
            FOUND_DB=false

            if [ -d "../backend/database/cloud/regtest" ]; then
                rm -rf ../backend/database/cloud/regtest
                echo "✅ Cloud regtest database folder removed"
                FOUND_DB=true
            fi

            if [ -d "../backend/database/self-hosted/regtest" ]; then
                rm -rf ../backend/database/self-hosted/regtest
                echo "✅ Self-hosted regtest database folder removed"
                FOUND_DB=true
            fi

            if [ "$FOUND_DB" = false ]; then
                echo "⚠️  No regtest database folders found (this is normal for first run)"
            fi
            
            echo "✅ Environment reset complete (all wallets, blockchain data, and database wiped)"
        else
            echo "Reset cancelled"
        fi
        ;;
        
    "logs")
        SERVICE=${2:-""}
        if [ -n "$SERVICE" ]; then
            docker-compose logs -f "$SERVICE"
        else
            docker-compose logs -f
        fi
        ;;
        
    "mine")
        BLOCKS=${2:-1}
        echo "Mining $BLOCKS block(s) to Miner wallet..."
        btc loadwallet "miner" 2>/dev/null || true
        ADDRESS=$(btc_miner getnewaddress)
        btc generatetoaddress "$BLOCKS" "$ADDRESS"
        echo "✅ Mined $BLOCKS block(s) to Miner wallet"
        ;;
    

    
    "reconsider-block")
        BLOCK_HASH="$2"
        if [ -z "$BLOCK_HASH" ]; then
            echo "Usage: $0 reconsider-block <block_hash>"
            echo "Example: $0 reconsider-block 1a2b3c4d5e6f..."
            exit 1
        fi
        
        echo "🔄 Reconsidering block $BLOCK_HASH..."
        
        # Get current state
        OLD_TIP_HEIGHT=$(btc getblockcount)
        
        # Reconsider the block
        btc reconsiderblock "$BLOCK_HASH"
        
        # Show new state
        NEW_TIP_HASH=$(btc getbestblockhash)
        NEW_TIP_HEIGHT=$(btc getblockcount)
        
        echo "   ✅ Block reconsidered!"
        echo "   New tip: $NEW_TIP_HASH (height: $NEW_TIP_HEIGHT)"
        echo ""
        if [ "$NEW_TIP_HEIGHT" -gt "$OLD_TIP_HEIGHT" ]; then
            echo "📊 Effect:"
            echo "   - Block was reactivated and became the new tip"
            echo "   - Blockchain height increased from $OLD_TIP_HEIGHT to $NEW_TIP_HEIGHT"
            echo "   - Transactions in the block are now confirmed again"
        else
            echo "📊 Effect:"
            echo "   - Block was reconsidered but did not become the new tip"
            echo "   - Another chain may have more work"
        fi
        ;;
    
    
    
    "get-mempool-txid")
        INDEX=${2:-0}
        TXID=$(get_mempool_txid "$INDEX")
        if [ $? -eq 0 ]; then
            echo "$TXID"
        fi
        ;;
    
    "mempool-purge")
        METHOD=${2:-restart}
        mempool_purge "$METHOD"
        ;;
    
    "reorg")
        reorg
        ;;
    
    "run-tests")
        WALLET_ADDRESS="$2"
        run_tests "$WALLET_ADDRESS"
        ;;
    
    "mempool-status")
        echo "=== Mempool Status ==="
        if btc getblockchaininfo > /dev/null 2>&1; then
            MEMPOOL_SIZE=$(btc getmempoolinfo | grep '"size"' | cut -d':' -f2 | tr -d ' ,')
            MEMPOOL_BYTES=$(btc getmempoolinfo | grep '"bytes"' | cut -d':' -f2 | tr -d ' ,')
            echo "Mempool transactions: $MEMPOOL_SIZE"
            echo "Mempool size: $MEMPOOL_BYTES bytes"
            if [ "$MEMPOOL_SIZE" -gt 0 ]; then
                echo "Pending transactions:"
                btc getrawmempool | grep -E '"[a-f0-9]{64}"' | head -5
                if [ "$MEMPOOL_SIZE" -gt 5 ]; then
                    echo "... and $((MEMPOOL_SIZE - 5)) more"
                fi
            else
                echo "No pending transactions"
            fi
        else
            echo "Bitcoin Core: ❌ Not running"
        fi
        ;;
    
    
    
    
    "status")
        echo "=== Bitcoin regtest Status ==="
        if btc getblockchaininfo > /dev/null 2>&1; then
            echo "Bitcoin Core: ✅ Running"
            echo "Block count: $(btc getblockcount)"
            btc loadwallet "segwit-desc" 2>/dev/null || true
            btc loadwallet "legacy-desc" 2>/dev/null || true
            btc loadwallet "nested-desc" 2>/dev/null || true
            btc loadwallet "taproot-desc" 2>/dev/null || true
            btc loadwallet "segwit-empty" 2>/dev/null || true
            btc loadwallet "charlie" 2>/dev/null || true
            btc loadwallet "miner" 2>/dev/null || true
            echo "Funded descriptor wallets:"
            echo "  segwit-desc:  $(btc_segwit_desc getbalance) BTC (wpkh - distributed)"
            echo "  legacy-desc:  $(btc_legacy_desc getbalance) BTC (pkh)"
            echo "  nested-desc:  $(btc_nested_desc getbalance) BTC (sh(wpkh))"
            echo "  taproot-desc: $(btc_taproot_desc getbalance) BTC (tr)"
            echo "Empty descriptor wallets:"
            echo "  segwit-empty:  $(btc_segwit_empty getbalance) BTC (wpkh)"
            echo "Other wallets:"
            echo "  charlie: $(btc_charlie getbalance) BTC (funded - 0.5 BTC at index 250)"
            echo "  miner:   $(btc_miner getbalance) BTC (background infrastructure)"
            echo "Network: $(btc getblockchaininfo | grep '"chain"' | cut -d'"' -f4)"
            
            # Add mempool info to status
            MEMPOOL_SIZE=$(btc getmempoolinfo | grep '"size"' | cut -d':' -f2 | tr -d ' ,')
            echo "Mempool transactions: $MEMPOOL_SIZE"
        else
            echo "Bitcoin Core: ❌ Not running"
        fi
        
        echo ""
        echo "=== Fulcrum Status ==="
        if nc -z localhost 50001 2>/dev/null; then
            echo "Fulcrum Electrum server: ✅ Running on port 50001"
        else
            echo "Fulcrum Electrum server: ❌ Not ready (may still be starting)"
        fi

        echo ""
        echo "=== ntfy Server Status ==="
        if curl -s http://localhost:2586/v1/health > /dev/null 2>&1; then
            echo "ntfy server: ✅ Running on http://localhost:2586"
            echo "  Auth: deny-all (user: testuser / testpassword)"
            NTFY_TOKEN=$(docker exec ntfy-regtest ntfy token list testuser 2>/dev/null | grep -o 'tk_[a-zA-Z0-9_]*' | head -1)
            if [ -n "$NTFY_TOKEN" ]; then
                echo "  Token: $NTFY_TOKEN"
            fi
        else
            echo "ntfy server: ❌ Not running"
        fi

        echo ""
        echo "=== Docker Containers ==="
        docker-compose ps
        ;;
    
    "add-wallets-to-backend")
        BACKEND_URL=${2:-"http://localhost:3000"}
        echo "Adding descriptor wallets to backend at $BACKEND_URL..."

        # Check if backend is running
        echo "🔍 Checking if backend is running..."
        if ! curl -s --connect-timeout 5 --max-time 10 "$BACKEND_URL/api/wallets" > /dev/null 2>&1; then
            echo "❌ Backend is not running at $BACKEND_URL"
            echo ""
            echo "💡 To start the backend:"
            echo "   1. Open a new terminal"
            echo "   2. cd ../backend"
            echo "   3. BITCOIN_NETWORK=regtest cargo run"
            echo ""
            echo "   Then run this command again."
            exit 1
        fi
        echo "✅ Backend is running"

        # Helper: extract multipath descriptor for a wallet given its descriptor prefix filter
        _get_multipath_descriptor() {
            local wallet_name="$1"
            local jq_filter="$2"
            local descriptors receive_desc multipath_raw checksum_info checksum
            descriptors=$(btc_wallet "$wallet_name" listdescriptors)
            receive_desc=$(echo "$descriptors" | jq -r ".descriptors[] | select($jq_filter) | .desc")
            multipath_raw=$(echo "$receive_desc" | sed 's|/0/\*|/<0;1>/\*|' | sed 's/#[^#]*$//')
            checksum_info=$(btc getdescriptorinfo "$multipath_raw")
            checksum=$(echo "$checksum_info" | jq -r '.checksum')
            echo "$multipath_raw#$checksum"
        }

        # Helper: add wallet to backend
        _add_wallet() {
            local name="$1"
            local descriptor="$2"
            echo "📤 Adding $name..."
            local response
            response=$(curl -s -X POST "$BACKEND_URL/api/wallets" \
                -H "Content-Type: application/json" \
                -d "{\"name\":\"$name\",\"descriptor\":\"$descriptor\"}")
            if echo "$response" | jq -e '.wallet.checksum' > /dev/null 2>&1; then
                local checksum
                checksum=$(echo "$response" | jq -r '.wallet.checksum')
                echo "   ✅ $name added (checksum: $checksum)"
                return 0
            else
                local error_msg
                error_msg=$(echo "$response" | jq -r '.error // "unknown error"')
                echo "   ❌ $name: $error_msg"
                return 1
            fi
        }

        # Load and extract descriptors for all wallets
        echo "📋 Getting wallet descriptors..."
        WALLET_CONFIGS=(
            "segwit-desc|.desc | startswith(\"wpkh(\") and contains(\"/0/*\")"
            "legacy-desc|.desc | startswith(\"pkh(\") and contains(\"/0/*\")"
            "nested-desc|.desc | startswith(\"sh(wpkh(\") and contains(\"/0/*\")"
            "taproot-desc|.desc | startswith(\"tr(\") and contains(\"/0/*\")"
            "segwit-empty|.desc | startswith(\"wpkh(\") and contains(\"/0/*\")"
            "legacy-empty|.desc | startswith(\"pkh(\") and contains(\"/0/*\")"
            "nested-empty|.desc | startswith(\"sh(wpkh(\") and contains(\"/0/*\")"
            "taproot-empty|.desc | startswith(\"tr(\") and contains(\"/0/*\")"
        )

        SUCCESS_COUNT=0
        TOTAL_COUNT=${#WALLET_CONFIGS[@]}

        for config in "${WALLET_CONFIGS[@]}"; do
            IFS='|' read -r wallet_name jq_filter <<< "$config"
            btc loadwallet "$wallet_name" 2>/dev/null || true
            DESCRIPTOR=$(_get_multipath_descriptor "$wallet_name" "$jq_filter")
            echo "   $wallet_name: $DESCRIPTOR"
            if _add_wallet "$wallet_name" "$DESCRIPTOR"; then
                SUCCESS_COUNT=$((SUCCESS_COUNT + 1))
            fi
        done

        # Also add Charlie
        btc loadwallet "charlie" 2>/dev/null || true
        CHARLIE_DESCRIPTOR=$(_get_multipath_descriptor "charlie" '.desc | startswith("wpkh(") and contains("/0/*")')
        echo "   charlie: $CHARLIE_DESCRIPTOR"
        if _add_wallet "Charlie" "$CHARLIE_DESCRIPTOR"; then
            SUCCESS_COUNT=$((SUCCESS_COUNT + 1))
        fi
        TOTAL_COUNT=$((TOTAL_COUNT + 1))

        echo ""
        if [ "$SUCCESS_COUNT" -eq "$TOTAL_COUNT" ]; then
            echo "🎉 All $TOTAL_COUNT wallets have been added to the backend!"
            echo "Check your frontend at http://localhost:3001 to see them."
        elif [ "$SUCCESS_COUNT" -gt 0 ]; then
            echo "⚠️  $SUCCESS_COUNT/$TOTAL_COUNT wallets added successfully."
            echo "Check your frontend at http://localhost:3001 to see what was added."
        else
            echo "❌ Failed to add wallets to the backend."
            echo "Please check the backend logs and try again."
        fi
        ;;
    
    "create-stress-wallet")
        TX_COUNT=${2:-""}
        if [ -z "$TX_COUNT" ]; then
            echo "Usage: $0 create-stress-wallet <tx_count>"
            echo "Example: $0 create-stress-wallet 1000"
            exit 1
        fi
        WALLET_NAME="stress-${TX_COUNT}tx"

        echo "Creating stress-test wallet '$WALLET_NAME' with $TX_COUNT transactions..."
        echo ""

        # Create the stress wallet (non-blank, Bitcoin Core generates keys)
        echo "1/4 Creating wallet..."
        btc unloadwallet "$WALLET_NAME" 2>/dev/null || true

        set +e
        CREATE_RESULT=$(btc -named createwallet wallet_name="$WALLET_NAME" disable_private_keys=false blank=false passphrase="" avoid_reuse=false descriptors=true 2>&1)
        CREATE_EXIT_CODE=$?
        set -e

        if echo "$CREATE_RESULT" | grep -q "already exists"; then
            echo "   Wallet exists, loading..."
            btc loadwallet "$WALLET_NAME" >/dev/null 2>&1 || true
        elif [ $CREATE_EXIT_CODE -eq 0 ]; then
            echo "   Wallet created"
        else
            echo "   Failed to create wallet: $CREATE_RESULT"
            exit 1
        fi

        # Get multipath descriptor for backend registration
        STRESS_DESCRIPTORS=$(btc_wallet "$WALLET_NAME" listdescriptors)
        STRESS_RECEIVE_DESC=$(echo "$STRESS_DESCRIPTORS" | jq -r '.descriptors[] | select(.desc | startswith("wpkh(") and contains("/0/*")) | .desc')
        STRESS_MULTIPATH_RAW=$(echo "$STRESS_RECEIVE_DESC" | sed 's|/0/\*|/<0;1>/\*|' | sed 's/#[^#]*$//')
        STRESS_CHECKSUM_INFO=$(btc getdescriptorinfo "$STRESS_MULTIPATH_RAW")
        STRESS_CHECKSUM=$(echo "$STRESS_CHECKSUM_INFO" | jq -r '.checksum')
        STRESS_DESCRIPTOR="$STRESS_MULTIPATH_RAW#$STRESS_CHECKSUM"
        echo "   Descriptor: $STRESS_DESCRIPTOR"

        # Ensure miner wallet is loaded and funded
        echo ""
        echo "2/4 Ensuring miner is funded..."
        btc loadwallet "miner" 2>/dev/null || true
        MINER_ADDRESS=$(btc_miner getnewaddress)
        MINER_BALANCE=$(btc_miner getbalance)
        echo "   Miner balance: $MINER_BALANCE BTC"

        # We need enough funds: ~TX_COUNT * 0.001 BTC plus fees
        # Mine more blocks if needed (each block gives 50 BTC on regtest)
        NEEDED_BTC=$(echo "scale=2; $TX_COUNT * 0.002" | bc -l)
        if [ "$(echo "$MINER_BALANCE < $NEEDED_BTC" | bc -l)" -eq 1 ]; then
            BLOCKS_NEEDED=$(echo "($NEEDED_BTC - $MINER_BALANCE) / 50 + 2" | bc)
            echo "   Mining $BLOCKS_NEEDED more blocks for funds..."
            btc generatetoaddress "$BLOCKS_NEEDED" "$MINER_ADDRESS" >/dev/null 2>&1
            MINER_BALANCE=$(btc_miner getbalance)
            echo "   Miner balance now: $MINER_BALANCE BTC"
        fi

        # Initial funding: send a chunk to the stress wallet
        echo ""
        echo "3/4 Initial funding..."
        INITIAL_FUND=$(echo "scale=8; $TX_COUNT * 0.002" | bc -l)
        STRESS_ADDR=$(btc_wallet "$WALLET_NAME" getnewaddress)
        btc_miner sendtoaddress "$STRESS_ADDR" "$INITIAL_FUND" >/dev/null 2>&1
        btc generatetoaddress 1 "$MINER_ADDRESS" >/dev/null 2>&1
        echo "   Funded with $INITIAL_FUND BTC"

        # Generate transactions in batches
        echo ""
        echo "4/4 Generating $TX_COUNT transactions..."
        echo "   (mining a block every 25 transactions to keep UTXOs confirmed)"
        echo ""

        COMPLETED=0
        BATCH_SIZE=25
        START_TIME=$(date +%s)

        while [ $COMPLETED -lt $TX_COUNT ]; do
            # Calculate how many txs in this batch
            REMAINING=$((TX_COUNT - COMPLETED))
            CURRENT_BATCH=$((REMAINING < BATCH_SIZE ? REMAINING : BATCH_SIZE))

            # Send transactions: stress wallet sends small amounts back to miner
            for i in $(seq 1 $CURRENT_BATCH); do
                DEST_ADDR=$(btc_miner getnewaddress)
                # Send a small amount (0.0001 BTC = 10,000 sats), subtract fee from amount
                if ! btc_wallet "$WALLET_NAME" sendtoaddress "$DEST_ADDR" 0.0001 "" "" true >/dev/null 2>&1; then
                    # If send fails (insufficient funds), refund from miner
                    REFUND_ADDR=$(btc_wallet "$WALLET_NAME" getnewaddress)
                    btc_miner sendtoaddress "$REFUND_ADDR" 0.5 >/dev/null 2>&1
                    btc generatetoaddress 1 "$MINER_ADDRESS" >/dev/null 2>&1
                    # Retry the send
                    btc_wallet "$WALLET_NAME" sendtoaddress "$DEST_ADDR" 0.0001 "" "" true >/dev/null 2>&1 || true
                fi
            done

            # Mine a block to confirm and make change outputs spendable
            btc generatetoaddress 1 "$MINER_ADDRESS" >/dev/null 2>&1

            COMPLETED=$((COMPLETED + CURRENT_BATCH))
            ELAPSED=$(($(date +%s) - START_TIME))
            if [ $ELAPSED -gt 0 ]; then
                RATE=$((COMPLETED / ELAPSED))
                if [ $RATE -gt 0 ]; then
                    ETA=$(( (TX_COUNT - COMPLETED) / RATE ))
                else
                    ETA="?"
                fi
                echo "   [${COMPLETED}/${TX_COUNT}] ~${RATE} tx/s, ETA: ${ETA}s"
            else
                echo "   [${COMPLETED}/${TX_COUNT}]"
            fi
        done

        TOTAL_TIME=$(($(date +%s) - START_TIME))
        FINAL_BALANCE=$(btc_wallet "$WALLET_NAME" getbalance)
        TX_LIST_COUNT=$(btc_wallet "$WALLET_NAME" listtransactions "*" 999999 | jq 'length')

        echo ""
        echo "Stress wallet '$WALLET_NAME' ready!"
        echo "   Transactions: $TX_LIST_COUNT"
        echo "   Balance: $FINAL_BALANCE BTC"
        echo "   Time: ${TOTAL_TIME}s"
        echo "   Descriptor: $STRESS_DESCRIPTOR"
        echo ""
        echo "To add to backend:"
        echo "   curl -X POST http://localhost:3000/api/wallets -H 'Content-Type: application/json' -d '{\"name\":\"$WALLET_NAME\",\"descriptor\":\"$STRESS_DESCRIPTOR\"}'"
        ;;

    "remove-wallets-from-backend")
        BACKEND_URL=${2:-"http://localhost:3000"}
        echo "Removing regtest wallets from backend at $BACKEND_URL..."

        # Get all wallets from backend
        WALLETS_RESPONSE=$(curl -s "$BACKEND_URL/api/wallets")
        
        if echo "$WALLETS_RESPONSE" | jq -e '.wallets' > /dev/null 2>&1; then
            # Find and delete regtest wallets
            echo "$WALLETS_RESPONSE" | jq -r '.wallets[] | select(.name | test("Regtest")) | .checksum' | while read -r wallet_id; do
                if [ -n "$wallet_id" ]; then
                    echo "🗑️  Deleting wallet $wallet_id..."
                    DELETE_RESPONSE=$(curl -s -X DELETE "$BACKEND_URL/api/wallets/$wallet_id")
                    if echo "$DELETE_RESPONSE" | jq -e '.message' > /dev/null 2>&1; then
                        echo "✅ Wallet $wallet_id deleted successfully"
                    else
                        echo "❌ Failed to delete wallet $wallet_id: $DELETE_RESPONSE"
                    fi
                fi
            done
            echo "🎉 Regtest wallets removed from backend!"
        else
            echo "❌ Failed to get wallets from backend: $WALLETS_RESPONSE"
        fi
        ;;
        
    "wipe-database")
        echo "🗑️  Wiping all SQLite databases..."
        FOUND_DB=false

        # Remove cloud mode databases
        if [ -d "../backend/database/cloud" ]; then
            rm -rf ../backend/database/cloud
            echo "✅ Cloud database folder removed"
            FOUND_DB=true
        fi

        # Remove self-hosted mode databases
        if [ -d "../backend/database/self-hosted" ]; then
            rm -rf ../backend/database/self-hosted
            echo "✅ Self-hosted database folder removed"
            FOUND_DB=true
        fi

        if [ "$FOUND_DB" = true ]; then
            echo "💡 Databases will be recreated when the backend starts"
        else
            echo "⚠️  No database folders found"
        fi
        ;;
        
    "kill")
        kill_servers
        echo "🎯 Port cleanup complete"
        ;;

    "btcpay-setup")
        BTCPAY=http://localhost:14142

        echo "Waiting for BTCPay Server to be ready..."
        for i in $(seq 1 60); do
            if curl -sf "$BTCPAY/api/v1/health" > /dev/null 2>&1; then
                echo "✅ BTCPay Server is ready"
                break
            fi
            if [ "$i" -eq 60 ]; then
                echo "❌ BTCPay Server did not become ready in time"
                echo "   Check logs with: docker-compose logs btcpayserver"
                exit 1
            fi
            sleep 2
        done

        # Give BTCPay a moment to fully initialize after health check passes
        sleep 3

        # Create admin user (first user on fresh instance, no auth needed)
        echo "Creating admin user..."
        USER_RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$BTCPAY/api/v1/users" \
            -H "Content-Type: application/json" \
            -d '{"email":"admin@test.com","password":"password123","isAdministrator":true}')
        USER_HTTP_CODE=$(echo "$USER_RESPONSE" | tail -1)
        USER_BODY=$(echo "$USER_RESPONSE" | sed '$d')
        if [ "$USER_HTTP_CODE" -ge 200 ] && [ "$USER_HTTP_CODE" -lt 300 ]; then
            echo "✅ Admin user created"
        elif [ "$USER_HTTP_CODE" -eq 422 ]; then
            echo "✅ Admin user already exists, continuing..."
        else
            echo "❌ Failed to create admin user (HTTP $USER_HTTP_CODE)"
            echo "   Response: $USER_BODY"
            exit 1
        fi

        # Create API key with needed permissions
        echo "Creating API key..."
        APIKEY_RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$BTCPAY/api/v1/api-keys" \
            -H "Content-Type: application/json" \
            -u "admin@test.com:password123" \
            -d '{"permissions":["unrestricted"]}')
        APIKEY_HTTP_CODE=$(echo "$APIKEY_RESPONSE" | tail -1)
        APIKEY_BODY=$(echo "$APIKEY_RESPONSE" | sed '$d')
        if [ "$APIKEY_HTTP_CODE" -lt 200 ] || [ "$APIKEY_HTTP_CODE" -ge 300 ]; then
            echo "❌ Failed to create API key (HTTP $APIKEY_HTTP_CODE)"
            echo "   Response: $APIKEY_BODY"
            exit 1
        fi
        API_KEY=$(echo "$APIKEY_BODY" | python3 -c "import sys,json; print(json.load(sys.stdin)['apiKey'])")
        echo "✅ API key created"

        # Create store
        echo "Creating store..."
        STORE_RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$BTCPAY/api/v1/stores" \
            -H "Content-Type: application/json" \
            -H "Authorization: token $API_KEY" \
            -d '{"name":"Canary Dev Store"}')
        STORE_HTTP_CODE=$(echo "$STORE_RESPONSE" | tail -1)
        STORE_BODY=$(echo "$STORE_RESPONSE" | sed '$d')
        if [ "$STORE_HTTP_CODE" -lt 200 ] || [ "$STORE_HTTP_CODE" -ge 300 ]; then
            echo "❌ Failed to create store (HTTP $STORE_HTTP_CODE)"
            echo "   Response: $STORE_BODY"
            exit 1
        fi
        STORE_ID=$(echo "$STORE_BODY" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])")
        echo "✅ Store created: $STORE_ID"

        # Generate on-chain wallet for the store
        echo "Generating store wallet..."
        WALLET_RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$BTCPAY/api/v1/stores/$STORE_ID/payment-methods/BTC-CHAIN/wallet/generate" \
            -H "Content-Type: application/json" \
            -H "Authorization: token $API_KEY" \
            -d '{"savePrivateKeys":true,"scriptPubKeyType":"Segwit"}')
        WALLET_HTTP_CODE=$(echo "$WALLET_RESPONSE" | tail -1)
        WALLET_BODY=$(echo "$WALLET_RESPONSE" | sed '$d')
        if [ "$WALLET_HTTP_CODE" -lt 200 ] || [ "$WALLET_HTTP_CODE" -ge 300 ]; then
            echo "❌ Failed to generate store wallet (HTTP $WALLET_HTTP_CODE)"
            echo "   Response: $WALLET_BODY"
            exit 1
        fi
        echo "✅ Store wallet generated"

        # Create offering for recurring donations
        echo "Creating subscription offering..."
        OFFERING_RESPONSE=$(curl -sf -X POST "$BTCPAY/api/v1/stores/$STORE_ID/offerings" \
            -H "Content-Type: application/json" \
            -H "Authorization: token $API_KEY" \
            -d '{"appName":"Canary Donations"}')
        if [ $? -ne 0 ]; then
            echo "⚠️  Failed to create offering (subscription API may not be available in this BTCPay version)"
            echo "   One-time donations will still work. Recurring donations require manual BTCPay setup."
            OFFERING_ID=""
            PLAN_ID=""
        else
            OFFERING_ID=$(echo "$OFFERING_RESPONSE" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])")
            echo "✅ Offering created: $OFFERING_ID"

            # Create plan under offering
            echo "Creating subscription plan..."
            PLAN_RESPONSE=$(curl -sf -X POST "$BTCPAY/api/v1/stores/$STORE_ID/offerings/$OFFERING_ID/plans" \
                -H "Content-Type: application/json" \
                -H "Authorization: token $API_KEY" \
                -d '{"name":"Monthly Supporter","currency":"USD","price":"5","recurringType":"Monthly"}')
            if [ $? -ne 0 ]; then
                echo "⚠️  Failed to create plan"
                PLAN_ID=""
            else
                PLAN_ID=$(echo "$PLAN_RESPONSE" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])")
                echo "✅ Plan created: $PLAN_ID"
            fi
        fi

        # Write BTCPay env vars to the active backend .env
        BACKEND_ENV="../backend/.env"
        if [[ -f "$BACKEND_ENV" ]]; then
            # Remove any existing BTCPAY_ lines
            sed -i '' '/^BTCPAY_/d' "$BACKEND_ENV"
            sed -i '' '/^# BTCPay Server (auto-configured/d' "$BACKEND_ENV"

            # Append new values
            echo "" >> "$BACKEND_ENV"
            echo "# BTCPay Server (auto-configured by dev.sh btcpay-setup)" >> "$BACKEND_ENV"
            echo "BTCPAY_URL=http://localhost:14142" >> "$BACKEND_ENV"
            echo "BTCPAY_API_KEY=$API_KEY" >> "$BACKEND_ENV"
            echo "BTCPAY_STORE_ID=$STORE_ID" >> "$BACKEND_ENV"
            if [ -n "$OFFERING_ID" ]; then
                echo "BTCPAY_OFFERING_ID=$OFFERING_ID" >> "$BACKEND_ENV"
            fi
            if [ -n "$PLAN_ID" ]; then
                echo "BTCPAY_PLAN_ID=$PLAN_ID" >> "$BACKEND_ENV"
            fi

            echo "✅ BTCPay config written to $BACKEND_ENV"
        else
            echo "⚠️  $BACKEND_ENV not found — printing env vars instead:"
            echo ""
            echo "BTCPAY_URL=http://localhost:14142"
            echo "BTCPAY_API_KEY=$API_KEY"
            echo "BTCPAY_STORE_ID=$STORE_ID"
            if [ -n "$OFFERING_ID" ]; then
                echo "BTCPAY_OFFERING_ID=$OFFERING_ID"
            fi
            if [ -n "$PLAN_ID" ]; then
                echo "BTCPAY_PLAN_ID=$PLAN_ID"
            fi
        fi

        echo ""
        echo "BTCPay admin UI: http://localhost:14142 (admin@test.com / password123)"
        ;;

    *)
        echo "Bitcoin regtest Docker development utilities"
        echo ""
        echo "Usage: $0 <command> [args...]"
        echo ""
        echo "Environment Commands:"
        echo "  start               Start Bitcoin + Electrum + ntfy containers"
        echo "  init                Start infra, create wallets, add to backend"
        echo "  stop                Stop all containers"
        echo "  restart             Restart all containers"
        echo "  reset               Stop containers and delete all data (includes database)"
        echo "  wipe-database       Remove all database folders (cloud & self-hosted)"
        echo "  kill                Kill processes on localhost ports 3000 and 3001"
        echo "  logs [service]      Show logs (bitcoin/electrum or all)"
        echo "  status              Show environment status"
        echo "  mode <mode>         Switch between self-hosted and cloud modes"
        echo "  btcpay-setup        Set up BTCPay Server (admin, store, API key)"
        echo ""
        echo "Funded Descriptor Wallets:"
        echo "  segwit-desc  — wpkh (Native SegWit), 1 BTC distributed across 31 addresses"
        echo "  legacy-desc  — pkh (Legacy P2PKH), 0.123 BTC"
        echo "  nested-desc  — sh(wpkh) (Nested SegWit), 0.123 BTC"
        echo "  taproot-desc — tr (Taproot), 0.123 BTC"
        echo ""
        echo "Empty Descriptor Wallets:"
        echo "  segwit-empty  — wpkh (Native SegWit), unfunded"
        echo "  legacy-empty  — pkh (Legacy P2PKH), unfunded"
        echo "  nested-empty  — sh(wpkh) (Nested SegWit), unfunded"
        echo "  taproot-empty — tr (Taproot), unfunded"
        echo ""
        echo "Wallet Commands (works for any wallet above, plus charlie and miner):"
        echo "  <wallet> balance                         Show wallet balance"
        echo "  <wallet> address                         Generate new address"
        echo "  <wallet> sending <dest> <amt1> [amt2...] Send Bitcoin (separate tx per amount)"
        echo "  <wallet> sending <dest> max              Drain wallet to destination"
        echo "  <wallet> sent <dest> <amt1> [amt2...]    Send and mine block to confirm"
        echo "  <wallet> sent <dest> max                 Drain wallet and mine block to confirm"
        echo "  <wallet> fund <addr> [amt]               Fund address (default: 1.0)"
        echo "  <wallet> rbf <txid>                      Replace transaction with higher fee"
        echo "  <wallet> cpfp <txid>                     Create CPFP child transaction"
        echo "  <wallet> consolidate                     Consolidate 2 smallest UTXOs"
        echo ""
        echo "Other Wallets:"
        echo "  charlie — wpkh, funded 0.5 BTC at address index 250 (deep scan testing)"
        echo "  miner   — heavily funded, for refunding drained wallets"
        echo ""
        echo "Single-Address Wallets (created by init, one address per type):"
        echo "  legacy-address sending <wallet> <amt>  Send from P2PKH address"
        echo "  p2sh-address sending <wallet> <amt>    Send from P2SH-P2WPKH address"
        echo "  segwit-address sending <wallet> <amt>  Send from P2WPKH address"
        echo "  taproot-address sending <wallet> <amt> Send from P2TR address"
        echo "  *-address balance                      Show balance"
        echo "  *-address address                      Show address"
        echo ""
        echo "Mining Commands:"
        echo "  mine [blocks]           Mine blocks to Miner wallet (default: 1)"
        echo "  reconsider-block <hash> Reconsider invalidated block"
        echo ""
        echo "Mempool Commands:"
        echo "  mempool-status               Show mempool transaction count and details"
        echo "  get-mempool-txid [index]     Get TXID from mempool by index (default: 0)"
        echo "  mempool-purge [method]       Purge mempool using method (restart/double-spend/low-fee)"
        echo "  reorg                        Blockchain reorganization (1 block)"
        echo "  run-tests <address>          Run comprehensive test suite with wallet address"
        echo ""
        echo "Stress Testing:"
        echo "  create-stress-wallet <count>    Create wallet with N transactions (e.g. 1000)"
        echo ""
        echo "Backend Integration:"
        echo "  add-wallets-to-backend [url]    Add wallets to backend (also done by init)"
        echo "  remove-wallets-from-backend [url] Remove regtest wallets from backend"
        echo ""
        echo "Examples:"
        echo "  $0 start                                        # Start the environment"
        echo "  $0 init                                         # Create wallets + add to backend"
        echo "  $0 mine 6                                       # Mine 6 blocks"
        echo "  $0 segwit-desc sending segwit-empty 0.5         # Send 0.5 BTC"
        echo "  $0 segwit-desc sending segwit-empty 0.1 0.2     # Send two separate transactions"
        echo "  $0 segwit-desc sent segwit-empty 0.5            # Send and mine block to confirm"
        echo "  $0 segwit-desc sending miner max                # Drain segwit-desc wallet to miner"
        echo "  $0 miner sending segwit-desc 1.0                # Refund segwit-desc from miner"
        echo "  $0 segwit-desc rbf <txid>                       # Replace transaction with fee bump"
        echo "  $0 segwit-desc cpfp <txid>                      # Create CPFP child transaction"
        echo "  $0 segwit-desc consolidate                      # Consolidate 2 smallest UTXOs"
        echo "  $0 legacy-desc sending legacy-empty 0.01        # Send between legacy wallets"
        echo "  $0 charlie sending segwit-desc 0.1 0.05 0.02   # Send from Charlie (tests high index)"
        echo "  $0 mine 1                                       # Mine 1 block (confirms pending)"
        echo "  $0 mempool-status                               # Check mempool"
        echo "  $0 get-mempool-txid 0                           # Get first transaction from mempool"
        echo "  $0 mempool-purge restart                        # Purge mempool via node restart"
        echo "  $0 reorg                                        # 1-block reorganization"
        echo "  $0 run-tests bcrt1q...                          # Run full test suite with wallet address"
        echo "  $0 reset                                        # Reset everything (includes backend cleanup)"
        ;;
esac