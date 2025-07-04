#!/bin/bash

# Bitcoin Regtest Development Utilities
# 
# This script provides a complete Bitcoin regtest development environment with:
# - Docker-based Bitcoin Core + Fulcrum Electrum server
# - Alice (funded with 1 BTC distributed) and Bob (unfunded) wallet creation
# - Backend integration for Output Descriptor Monitor
# - Advanced Bitcoin transaction testing (RBF, CPFP, mempool operations)
#
# Key Commands:
#   start           - Start infrastructure (Bitcoin Core + Fulcrum)
#   create-wallets  - Create Alice (1 BTC distributed) and Bob (unfunded) wallets
#   add-wallets-to-backend - Integrate wallets with backend API
#
# Workflow:
#   1. ./docker-utils.sh start
#   2. ./docker-utils.sh create-wallets  (Alice gets 1 BTC distributed automatically)
#   3. cd ../backend && BITCOIN_NETWORK=regtest cargo run
#   4. cd ../regtest-env && ./docker-utils.sh add-wallets-to-backend

# Function to run bitcoin-cli against the Docker container  
btc() {
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 "$@"
}

# Function to run bitcoin-cli with Alice wallet against the Docker container
btc_alice() {
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 -rpcwallet=alice "$@"
}

# Function to run bitcoin-cli with Bob wallet against the Docker container
btc_bob() {
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 -rpcwallet=bob "$@"
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

# --- CPFP logic as a function for both Alice and Bob ---
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
        echo "❌ $WALLET wallet not found. Run '$0 create-wallets' first"
        exit 1
    fi
    # Check wallet has sufficient confirmed funds for fees
    WALLET_BALANCE=$(btc_wallet "$WALLET" getbalance)
    echo "💰 $WALLET wallet balance: $WALLET_BALANCE BTC (confirmed)"
    if [ "$(echo "$WALLET_BALANCE < 0.001" | bc -l)" -eq 1 ]; then
        echo "❌ $WALLET needs confirmed funds for CPFP fees. Current balance: $WALLET_BALANCE BTC"
        echo "💡 Fund $WALLET first with: $0 $([[ "$WALLET" == "alice" ]] && echo "bob" || echo "alice") send 0.01 && $0 mine 1"
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
    # Calculate child amount (leave high fee for CPFP acceleration)
    CHILD_AMOUNT_RAW=$(echo "scale=8; $TOTAL_WALLET_AMOUNT - 0.005" | bc -l)  # 0.005 BTC fee
    # Ensure proper decimal formatting for JSON (must start with 0. not .)
    CHILD_AMOUNT=$(echo "$CHILD_AMOUNT_RAW" | sed 's/^\./0./')
    if [ "$(echo "$CHILD_AMOUNT < 0.0001" | bc -l)" -eq 1 ]; then
        echo "❌ Child amount too small: $CHILD_AMOUNT BTC (need at least 0.0001 BTC after fees)"
        exit 1
    fi
    # Get change address for the child transaction
    CHANGE_ADDRESS=$(btc_wallet "$WALLET" getnewaddress)
    echo "   🔍 Creating CPFP child spending $TOTAL_WALLET_AMOUNT BTC → $CHILD_AMOUNT BTC (0.005 BTC fee)"
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
    echo "   💰 Amount: $CHILD_AMOUNT BTC (high fee: 0.005 BTC)"
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
                local NEW_ADDRESS=$(btc_alice getnewaddress)
                btc loadwallet "alice" 2>/dev/null || true
                btc_alice sendtoaddress "$NEW_ADDRESS" 0.001
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
            btc loadwallet "alice" 2>/dev/null || true
            local UTXOS=$(btc_alice listunspent 1)
            
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
            local ADDRESS1=$(btc_alice getnewaddress)
            local ADDRESS2=$(btc_alice getnewaddress)
            
            # Send amount split for fees
            local SEND_AMOUNT=$(echo "scale=8; $UTXO_AMOUNT - 0.001" | bc -l)
            
            echo "🚀 Creating first transaction to $ADDRESS1..."
            # Create raw transaction manually to ensure we use the same UTXO
            local RAW_TX1=$(btc createrawtransaction "[{\"txid\":\"$UTXO_TXID\",\"vout\":$UTXO_VOUT}]" "{\"$ADDRESS1\":$SEND_AMOUNT}")
            local SIGNED_TX1=$(btc_alice signrawtransactionwithwallet "$RAW_TX1" | jq -r '.hex')
            local TXID1=$(btc sendrawtransaction "$SIGNED_TX1")
            
            echo "✅ First transaction: $TXID1"
            
            echo "🚀 Creating conflicting transaction to $ADDRESS2..."
            # Create conflicting transaction using same UTXO
            local RAW_TX2=$(btc createrawtransaction "[{\"txid\":\"$UTXO_TXID\",\"vout\":$UTXO_VOUT}]" "{\"$ADDRESS2\":$SEND_AMOUNT}")
            local SIGNED_TX2=$(btc_alice signrawtransactionwithwallet "$RAW_TX2" | jq -r '.hex')
            
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
            btc loadwallet "alice" 2>/dev/null || true
            local NEW_ADDRESS=$(btc_alice getnewaddress)
            
            # Create transaction with extremely low fee (1 sat/kB)
            local LOW_FEE_RATE=0.00000001
            
            local TXID=$(btc_alice sendtoaddress "$NEW_ADDRESS" 0.001 "" "" false true "$LOW_FEE_RATE" "unset" 2>/dev/null || echo "")
            
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
                echo "   for i in {1..10}; do $0 alice fund \$(btc_alice getnewaddress) 0.001; done"
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
    
    # Create test transaction before reorg (Alice sends to Bob)
    echo ""
    echo "💰 Creating test transaction before reorg..."
    btc loadwallet "alice" 2>/dev/null || true
    btc loadwallet "bob" 2>/dev/null || true
    local BOB_ADDRESS=$(btc_bob getnewaddress)
    local TEST_TXID=$(btc_alice sendtoaddress "$BOB_ADDRESS" 0.001)
    echo "✅ Test transaction: $TEST_TXID"
    echo "   👩 Alice → 👨 Bob: 0.001 BTC"
    echo "   🎯 Bob's address: $BOB_ADDRESS"
    
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
    local TX_INFO=$(btc_alice gettransaction "$TEST_TXID")
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
    local TX_INFO_AFTER=$(btc_alice gettransaction "$TEST_TXID" 2>/dev/null || echo "{}")
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
    local TX_INFO_FINAL=$(btc_alice gettransaction "$TEST_TXID" 2>/dev/null || echo "{}")
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
    
    echo "🧪 TxRay - Comprehensive Bitcoin Test Suite"
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
    btc loadwallet "alice" 2>/dev/null || true
    btc_alice sendtoaddress "$WALLET_ADDRESS" 0.001
    pause_test
    
    # Test 2: RBF Testing
    echo "📍 TEST 2: RBF (Replace-By-Fee)"
    echo "-------------------------------"
    echo "Creating low-fee transaction for RBF testing..."
    local FIRST_TXID=$(btc_alice sendtoaddress "$WALLET_ADDRESS" 0.002 "" "" false true 0.00001 "unset")
    echo "First transaction: $FIRST_TXID"
    sleep 2
    
    echo "Attempting RBF replacement with bumpfee..."
    local RESULT=$(btc_alice bumpfee "$FIRST_TXID" "{\"fee_rate\": 15}" 2>&1 || echo "RBF failed")
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
    local PARENT_TXID=$(btc_alice sendtoaddress "$WALLET_ADDRESS" 0.003 "" "" false true 0.00001 "unset")
    echo "Parent transaction: $PARENT_TXID"
    sleep 2
    
    echo "Creating CPFP child transaction..."
    cpfp_for_wallet "alice" "$PARENT_TXID"
    pause_test
    
    # Test 4: Mempool Purge Testing
    echo "📍 TEST 4: Mempool Purge (Node Restart)"
    echo "---------------------------------------"
    echo "Creating transaction to be purged..."
    btc_alice sendtoaddress "$WALLET_ADDRESS" 0.001
    sleep 2
    
    echo "Testing mempool purge via restart..."
    mempool_purge "restart"
    pause_test
    
    # Test 5: Blockchain Reorganization
    echo "📍 TEST 5: Blockchain Reorganization"
    echo "------------------------------------"
    echo "Creating transaction for reorg testing..."
    btc_alice sendtoaddress "$WALLET_ADDRESS" 0.004
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
    btc_alice sendtoaddress "$WALLET_ADDRESS" 0.005
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
    echo "   - Real-time SMS updates"
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

# --- New multi-word command parsing for wallet actions ---
if [[ "$1" == "alice" || "$1" == "bob" ]]; then
    WALLET="$1"
    SUBCMD="$2"
    shift 2
    case "$SUBCMD" in
        send)
            AMOUNT="$1"
            if [ -z "$AMOUNT" ]; then
                echo "Usage: $0 $WALLET send <amount>"
                exit 1
            fi
            btc loadwallet "$WALLET" 2>/dev/null || true
            if [ "$WALLET" == "alice" ]; then
                btc loadwallet "bob" 2>/dev/null || true
                TARGET_ADDRESS=$(btc_bob getnewaddress)
                echo "🎯 Sending $AMOUNT BTC from Alice to Bob address: $TARGET_ADDRESS"
                TXID=$(btc_alice sendtoaddress "$TARGET_ADDRESS" "$AMOUNT")
            else
                btc loadwallet "alice" 2>/dev/null || true
                TARGET_ADDRESS=$(btc_alice getnewaddress)
                echo "🎯 Sending $AMOUNT BTC from Bob to Alice address: $TARGET_ADDRESS"
                TXID=$(btc_bob sendtoaddress "$TARGET_ADDRESS" "$AMOUNT")
            fi
            echo "✅ Transaction sent: $TXID"
            echo "💡 Use '$0 mine' to confirm transaction"
            exit 0
            ;;
        rbf)
            TXID="$1"
            FEE_RATE=${2:-10}
            if [ -z "$TXID" ]; then
                echo "Usage: $0 $WALLET rbf <txid> [fee_rate_sat_per_byte]"
                exit 1
            fi
            echo "🔄 Bumping fee for transaction $TXID to $FEE_RATE sat/byte..."
            btc loadwallet "$WALLET" 2>/dev/null || true
            if [ "$WALLET" == "alice" ]; then
                RESULT=$(btc_alice bumpfee "$TXID" "{\"fee_rate\": $FEE_RATE}" 2>&1)
            else
                RESULT=$(btc_bob bumpfee "$TXID" "{\"fee_rate\": $FEE_RATE}" 2>&1)
            fi
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
                echo "❌ $WALLET wallet not found. Run '$0 create-wallets' first"
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
            CONSOLIDATE_ADDRESS=$(btc_wallet "$WALLET" getrawchangeaddress)
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
        
        echo ""
        echo "🚀 Bitcoin regtest environment is running!"
        echo "Bitcoin RPC: localhost:18443"
        echo "Fulcrum Electrum server: localhost:50001"
        echo "Set BITCOIN_NETWORK=regtest in your environment"
        echo ""
        echo "💡 Next: $0 create-wallets (creates Alice with 1 BTC distributed)"
        ;;
    
    "create-wallets")
        echo "🏦 Setting up Alice, Bob and Miner wallets..."
        
        # Check if Bitcoin Core is running
        if ! btc getblockchaininfo > /dev/null 2>&1; then
            echo "❌ Bitcoin Core is not running. Run '$0 start' first."
            exit 1
        fi
        
        # Create Alice wallet (deterministic)
        echo "📋 Creating Alice wallet..."
        btc unloadwallet "alice" 2>/dev/null || true
        
        set +e  # Temporarily disable exit on error
        CREATE_RESULT=$(btc -named createwallet wallet_name="alice" disable_private_keys=false blank=true passphrase="" avoid_reuse=false descriptors=true 2>&1)
        CREATE_EXIT_CODE=$?
        set -e
        
        if echo "$CREATE_RESULT" | grep -q "already exists"; then
            echo "   ✅ Alice wallet exists, loading..."
            btc loadwallet "alice" >/dev/null 2>&1 || true
        elif [ $CREATE_EXIT_CODE -eq 0 ]; then
            echo "   ✅ Alice blank wallet created"
            
            # Import deterministic descriptors for Alice (regtest vprv keys)
            btc_alice importdescriptors '[
              {
                "desc": "wpkh(tprv8ZgxMBicQKsPe6D3wxZPHxy7JEMBwjxiimYXSvM8aRaqZmDvU7Jec5c8aSB8rCzFmAP6aEjPhUmiXm2KB7XUzkApX2prmDHcQsUQY5DsxJw/84h/1h/0h/0/*)#5asejmkj",
                "timestamp": "now",
                "active": true,
                "internal": false,
                "range": [0, 999]
              },
              {
                "desc": "wpkh(tprv8ZgxMBicQKsPe6D3wxZPHxy7JEMBwjxiimYXSvM8aRaqZmDvU7Jec5c8aSB8rCzFmAP6aEjPhUmiXm2KB7XUzkApX2prmDHcQsUQY5DsxJw/84h/1h/0h/1/*)#9f4c0wx2",
                "timestamp": "now",
                "active": true,
                "internal": true,
                "range": [0, 999]
              }
            ]' >/dev/null 2>&1
            echo "   ✅ Alice wallet seeded with deterministic descriptors"
        else
            echo "   ❌ Failed to create Alice wallet: $CREATE_RESULT"
            exit 1
        fi
        
        # Get Alice descriptor and convert to multipath format
        # This creates a descriptor compatible with the backend API requirement
        ALICE_DESCRIPTORS=$(btc_alice listdescriptors)
        ALICE_RECEIVE_DESC=$(echo "$ALICE_DESCRIPTORS" | jq -r '.descriptors[] | select(.desc | startswith("wpkh") and contains("/0/*")) | .desc')
        ALICE_MULTIPATH_RAW=$(echo "$ALICE_RECEIVE_DESC" | sed 's|/0/\*|/<0;1>/\*|' | sed 's/#[^#]*$//')
        # Get proper checksum for multipath descriptor from Bitcoin Core
        ALICE_CHECKSUM_INFO=$(btc getdescriptorinfo "$ALICE_MULTIPATH_RAW")
        ALICE_CHECKSUM=$(echo "$ALICE_CHECKSUM_INFO" | jq -r '.checksum')
        ALICE_DESCRIPTOR="$ALICE_MULTIPATH_RAW#$ALICE_CHECKSUM"
        
        # Create Bob wallet (deterministic)
        echo "📋 Creating Bob wallet..."
        btc unloadwallet "bob" 2>/dev/null || true
        
        set +e  # Temporarily disable exit on error
        CREATE_RESULT=$(btc -named createwallet wallet_name="bob" disable_private_keys=false blank=true passphrase="" avoid_reuse=false descriptors=true 2>&1)
        CREATE_EXIT_CODE=$?
        set -e
        
        if echo "$CREATE_RESULT" | grep -q "already exists"; then
            echo "   ✅ Bob wallet exists, loading..."
            btc loadwallet "bob" >/dev/null 2>&1 || true
        elif [ $CREATE_EXIT_CODE -eq 0 ]; then
            echo "   ✅ Bob blank wallet created"
            
            # Import deterministic descriptors for Bob (regtest vprv keys)
            btc_bob importdescriptors '[
              {
                "desc": "wpkh(tprv8ZgxMBicQKsPd3nsWkJrKQgXk22UnGFHFPDWNCeTjvzeY9LjRYdi3RNX1NfkCwj4mD1YTsNPCCtGPTTQkxn14oLKNg6vuVkChn55qa5rC7K/84h/1h/0h/0/*)#y872gtkp",
                "timestamp": "now",
                "active": true,
                "internal": false,
                "range": [0, 999]
              },
              {
                "desc": "wpkh(tprv8ZgxMBicQKsPd3nsWkJrKQgXk22UnGFHFPDWNCeTjvzeY9LjRYdi3RNX1NfkCwj4mD1YTsNPCCtGPTTQkxn14oLKNg6vuVkChn55qa5rC7K/84h/1h/0h/1/*)#4nmt47xe",
                "timestamp": "now",
                "active": true,
                "internal": true,
                "range": [0, 999]
              }
            ]' >/dev/null 2>&1
            echo "   ✅ Bob wallet seeded with deterministic descriptors"
        else
            echo "   ❌ Failed to create Bob wallet: $CREATE_RESULT"
            exit 1
        fi
        
        # Get Bob descriptor and address
        BOB_DESCRIPTORS=$(btc_wallet bob listdescriptors)
        BOB_RECEIVE_DESC=$(echo "$BOB_DESCRIPTORS" | jq -r '.descriptors[] | select(.desc | startswith("wpkh") and contains("/0/*")) | .desc')
        BOB_MULTIPATH_RAW=$(echo "$BOB_RECEIVE_DESC" | sed 's|/0/\*|/<0;1>/\*|' | sed 's/#[^#]*$//')
        # Get proper checksum for multipath descriptor from Bitcoin Core
        BOB_CHECKSUM_INFO=$(btc getdescriptorinfo "$BOB_MULTIPATH_RAW")
        BOB_CHECKSUM=$(echo "$BOB_CHECKSUM_INFO" | jq -r '.checksum')
        BOB_DESCRIPTOR="$BOB_MULTIPATH_RAW#$BOB_CHECKSUM"
        
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
        
        # Fund Alice wallet with distributed strategy
        echo "💰 Funding Alice wallet..."
        BLOCK_COUNT=$(btc getblockcount 2>/dev/null || echo "0")
        
        if [ "$BLOCK_COUNT" -lt 104 ]; then
            echo "   ⛏️  Mining blocks and transferring funds to Alice..."
            # Mine 103 blocks to Miner (150 BTC total)
            btc generatetoaddress 103 "$MINER_ADDRESS" >/dev/null 2>&1
            
            # Generate Alice addresses for distributed funding
            echo "   📍 Generating addresses for distributed funding..."
            
            # Build sendmany recipients object
            RECIPIENTS="{"
            
            # 1 address with 0.5 BTC
            ALICE_ADDR_5=$(btc_wallet alice getnewaddress)
            RECIPIENTS="${RECIPIENTS}\"$ALICE_ADDR_5\":0.5"
            
            # 5 addresses with 0.05 BTC each
            for i in {1..5}; do
                ALICE_ADDR_05=$(btc_wallet alice getnewaddress)
                RECIPIENTS="${RECIPIENTS},\"$ALICE_ADDR_05\":0.05"
            done
            
            # 25 addresses with 0.01 BTC each
            for i in {1..25}; do
                ALICE_ADDR_01=$(btc_wallet alice getnewaddress)
                RECIPIENTS="${RECIPIENTS},\"$ALICE_ADDR_01\":0.01"
            done
            
            RECIPIENTS="${RECIPIENTS}}"
            
            # Send 1 BTC distributed across multiple addresses in one transaction
            echo "   💸 Creating single transaction with multiple outputs..."
            echo "   📊 Distribution: 1×0.5 BTC + 5×0.05 BTC + 25×0.01 BTC = 1 BTC across 31 addresses"
            TXID=$(btc_miner sendmany "" "$RECIPIENTS")
            
            # Mine 1 block to confirm Alice's transaction
            btc generatetoaddress 1 "$MINER_ADDRESS" >/dev/null 2>&1
            echo "   ✅ Alice funded with 1 BTC (distributed across 31 addresses)"
        else
            echo "   ✅ Alice already funded"
        fi
        
        # Show final balances
        ALICE_BALANCE=$(btc_wallet alice getbalance)
        echo "   💰 Alice balance: $ALICE_BALANCE BTC"
        
        echo ""
        echo "🎉 Alice and Bob wallets setup complete!"
        echo ""
        echo "📱 Add these descriptors to your wallet app to follow along:"
        echo "   👩 Alice Wallet (funded - 1 BTC):  $ALICE_DESCRIPTOR"
        echo "   👨 Bob Wallet (unfunded):           $BOB_DESCRIPTOR"
        echo ""
        echo "💡 Wallets are ready - addresses will be derived automatically by your backend"
        echo ""
        echo "💡 Next: $0 add-wallets-to-backend (requires backend running)"
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
            
            # Remove wallets from backend if it's running
            echo "Removing wallets from backend..."
            if curl -s --connect-timeout 2 http://localhost:3001/wallets > /dev/null 2>&1; then
                ./docker-utils.sh remove-wallets-from-backend
            else
                echo "⚠️  Backend not running, skipping wallet removal"
            fi
            
            # Try to unload wallets before reset (if Bitcoin is running)
            if btc getblockchaininfo > /dev/null 2>&1; then
                echo "Unloading test wallets..."
                btc unloadwallet "alice" 2>/dev/null || true
                btc unloadwallet "bob" 2>/dev/null || true
                btc unloadwallet "miner" 2>/dev/null || true
            fi
            
            # Stop containers and remove all volumes (includes wallet data)
            docker-compose down -v
            
            # Clean up regtest database folder completely
            echo "Cleaning up regtest database..."
            if [ -d "../backend/database/regtest" ]; then
                rm -rf ../backend/database/regtest
                echo "✅ Regtest database folder removed"
            else
                echo "⚠️  Regtest database folder not found (this is normal for first run)"
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
    
    "alice-balance")
        btc loadwallet "alice" 2>/dev/null || true
        BALANCE=$(btc_alice getbalance)
        echo "Alice wallet balance: $BALANCE BTC"
        ;;
    
    "alice-address")
        btc loadwallet "alice" 2>/dev/null || true
        ADDRESS=$(btc_alice getnewaddress)
        echo "New Alice address: $ADDRESS"
        ;;
    
    "bob-balance")
        btc loadwallet "bob" 2>/dev/null || true
        BALANCE=$(btc_bob getbalance)
        echo "Bob wallet balance: $BALANCE BTC"
        ;;
    
    "bob-address")
        btc loadwallet "bob" 2>/dev/null || true
        ADDRESS=$(btc_bob getnewaddress)
        echo "New Bob address: $ADDRESS"
        ;;
    
    "alice-fund")
        if [ -z "$2" ]; then
            echo "Usage: $0 alice-fund <address> [amount=1.0]"
            exit 1
        fi
        AMOUNT=${3:-1.0}
        echo "Funding address $2 with $AMOUNT BTC from Alice..."
        btc loadwallet "alice" 2>/dev/null || true
        TXID=$(btc_alice sendtoaddress "$2" "$AMOUNT")
        echo "Transaction: $TXID"
        echo "💡 Use '$0 mine' to confirm transaction"
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
            btc loadwallet "alice" 2>/dev/null || true
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
            btc loadwallet "alice" 2>/dev/null || true
            btc loadwallet "bob" 2>/dev/null || true
            btc loadwallet "miner" 2>/dev/null || true
            echo "Alice wallet balance: $(btc_alice getbalance) BTC (funded - distributed across addresses)"
            echo "Bob wallet balance: $(btc_bob getbalance) BTC (unfunded)"
            echo "Miner wallet balance: $(btc_miner getbalance) BTC (background infrastructure)"
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
        echo "=== Docker Containers ==="
        docker-compose ps
        ;;
    
    "add-wallets-to-backend")
        BACKEND_URL=${2:-"http://localhost:3001"}
        echo "Adding Alice and Bob wallets to backend at $BACKEND_URL..."
        
        # Check if backend is running
        echo "🔍 Checking if backend is running..."
        if ! curl -s --connect-timeout 5 --max-time 10 "$BACKEND_URL/wallets" > /dev/null 2>&1; then
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
        
        # Get the descriptors from the Bitcoin wallets
        echo "📋 Getting wallet descriptors..."
        btc loadwallet "alice" 2>/dev/null || true
        btc loadwallet "bob" 2>/dev/null || true
        
        # Get Alice descriptor and convert to multipath format
        # 1. Get raw descriptor from Bitcoin Core wallet  
        # 2. Convert /0/* to /<0;1>/* for multipath support
        # 3. Remove old checksum (new one calculated by backend)
        # Note: Skip getdescriptorinfo as it reverts multipath format to single-path
        ALICE_DESCRIPTORS=$(btc_wallet alice listdescriptors)
        ALICE_RECEIVE_DESC=$(echo "$ALICE_DESCRIPTORS" | jq -r '.descriptors[] | select(.desc | startswith("wpkh") and contains("/0/*")) | .desc')
        ALICE_MULTIPATH_RAW=$(echo "$ALICE_RECEIVE_DESC" | sed 's|/0/\*|/<0;1>/\*|' | sed 's/#[^#]*$//')
        # Get proper checksum for multipath descriptor from Bitcoin Core
        ALICE_CHECKSUM_INFO=$(btc getdescriptorinfo "$ALICE_MULTIPATH_RAW")
        ALICE_CHECKSUM=$(echo "$ALICE_CHECKSUM_INFO" | jq -r '.checksum')
        ALICE_DESCRIPTOR="$ALICE_MULTIPATH_RAW#$ALICE_CHECKSUM"
        
        # Get Bob descriptor and convert to multipath format (same process as Alice)
        BOB_DESCRIPTORS=$(btc_wallet bob listdescriptors)
        BOB_RECEIVE_DESC=$(echo "$BOB_DESCRIPTORS" | jq -r '.descriptors[] | select(.desc | startswith("wpkh") and contains("/0/*")) | .desc')
        BOB_MULTIPATH_RAW=$(echo "$BOB_RECEIVE_DESC" | sed 's|/0/\*|/<0;1>/\*|' | sed 's/#[^#]*$//')
        # Get proper checksum for multipath descriptor from Bitcoin Core
        BOB_CHECKSUM_INFO=$(btc getdescriptorinfo "$BOB_MULTIPATH_RAW")
        BOB_CHECKSUM=$(echo "$BOB_CHECKSUM_INFO" | jq -r '.checksum')
        BOB_DESCRIPTOR="$BOB_MULTIPATH_RAW#$BOB_CHECKSUM"
        
        echo "👩 Alice descriptor: $ALICE_DESCRIPTOR"
        echo "👨 Bob descriptor: $BOB_DESCRIPTOR"
        
        # Add Alice wallet to backend
        echo "📤 Adding Alice wallet to backend..."
        ALICE_RESPONSE=$(curl -s -X POST "$BACKEND_URL/wallets" \
            -H "Content-Type: application/json" \
            -d "{\"name\":\"Alice (Regtest)\",\"output_descriptor\":\"$ALICE_DESCRIPTOR\",\"gap_limit\":5}")
        
        if echo "$ALICE_RESPONSE" | jq -e '.id' > /dev/null 2>&1; then
            ALICE_ID=$(echo "$ALICE_RESPONSE" | jq -r '.id')
            echo "✅ Alice wallet added with ID: $ALICE_ID"
            ALICE_SUCCESS=true
        else
            echo "❌ Failed to add Alice wallet"
            if echo "$ALICE_RESPONSE" | jq -e '.error' > /dev/null 2>&1; then
                ERROR_MSG=$(echo "$ALICE_RESPONSE" | jq -r '.error')
                echo "   Error: $ERROR_MSG"
            else
                echo "   Response: $ALICE_RESPONSE"
            fi
            ALICE_SUCCESS=false
        fi
        
        # Add Bob wallet to backend
        echo "📤 Adding Bob wallet to backend..."
        BOB_RESPONSE=$(curl -s -X POST "$BACKEND_URL/wallets" \
            -H "Content-Type: application/json" \
            -d "{\"name\":\"Bob (Regtest)\",\"output_descriptor\":\"$BOB_DESCRIPTOR\",\"gap_limit\":5}")
        
        if echo "$BOB_RESPONSE" | jq -e '.id' > /dev/null 2>&1; then
            BOB_ID=$(echo "$BOB_RESPONSE" | jq -r '.id')
            echo "✅ Bob wallet added with ID: $BOB_ID"
            BOB_SUCCESS=true
        else
            echo "❌ Failed to add Bob wallet"
            if echo "$BOB_RESPONSE" | jq -e '.error' > /dev/null 2>&1; then
                ERROR_MSG=$(echo "$BOB_RESPONSE" | jq -r '.error')
                echo "   Error: $ERROR_MSG"
            else
                echo "   Response: $BOB_RESPONSE"
            fi
            BOB_SUCCESS=false
        fi
        
        echo ""
        if [ "$ALICE_SUCCESS" = true ] && [ "$BOB_SUCCESS" = true ]; then
            echo "🎉 Both Alice and Bob wallets have been added to the backend!"
            echo "Check your frontend at http://localhost:3000 to see them."
        elif [ "$ALICE_SUCCESS" = true ] || [ "$BOB_SUCCESS" = true ]; then
            echo "⚠️  Some wallets were added successfully, but there were errors."
            echo "Check your frontend at http://localhost:3000 to see what was added."
        else
            echo "❌ Failed to add wallets to the backend."
            echo "Please check the backend logs and try again."
        fi
        ;;
    
    "remove-wallets-from-backend")
        BACKEND_URL=${2:-"http://localhost:3001"}
        echo "Removing regtest wallets from backend at $BACKEND_URL..."
        
        # Get all wallets from backend
        WALLETS_RESPONSE=$(curl -s "$BACKEND_URL/wallets")
        
        if echo "$WALLETS_RESPONSE" | jq -e '.wallets' > /dev/null 2>&1; then
            # Find and delete Alice, Bob and Miner wallets
            echo "$WALLETS_RESPONSE" | jq -r '.wallets[] | select(.name | test("Alice.*Regtest|Bob.*Regtest|Miner.*Regtest")) | .id' | while read -r wallet_id; do
                if [ -n "$wallet_id" ]; then
                    echo "🗑️  Deleting wallet $wallet_id..."
                    DELETE_RESPONSE=$(curl -s -X DELETE "$BACKEND_URL/wallets/$wallet_id")
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
        echo "🗑️  Wiping SQLite database..."
        
        # Remove SQLite metadata database
        if [ -f "../backend/txray.sqlite" ]; then
            rm -f ../backend/txray.sqlite
            echo "✅ SQLite metadata database removed"
            echo "💡 The database will be recreated when the backend starts"
        else
            echo "⚠️  SQLite metadata database not found"
        fi
        
        # Remove BDK wallet files
        if [ -d "../backend/wallets" ]; then
            rm -rf ../backend/wallets
            echo "✅ BDK wallets directory removed"
        else
            echo "⚠️  BDK wallets directory not found"
        fi
        ;;
        
    *)
        echo "Bitcoin regtest Docker development utilities"
        echo ""
        echo "Usage: $0 <command> [args...]"
        echo ""
        echo "Environment Commands:"
        echo "  start               Start Bitcoin + Electrum containers"
        echo "  create-wallets      Create Alice and Bob wallets (run after start)"
        echo "  stop                Stop all containers"
        echo "  restart             Restart all containers"  
        echo "  reset               Stop containers and delete all data (includes database)"
        echo "  wipe-database       Drop all database tables (standalone command)"
        echo "  logs [service]      Show logs (bitcoin/electrum or all)"
        echo "  status              Show environment status"
        echo ""
        echo "Alice Commands (funded wallet - 1 BTC distributed):"
        echo "  alice balance             Show Alice wallet balance"
        echo "  alice address             Generate new Alice address"
        echo "  alice send <amt>          Send Bitcoin from Alice to Bob (RBF-enabled)"
        echo "  alice fund <addr> [amt]   Fund address from Alice (default: 1.0)"
        echo "  alice rbf <txid> [rate]   Replace transaction with higher fee (default: 10 sat/byte)"
        echo "  alice cpfp <txid>         Create CPFP child transaction for Alice's unconfirmed output"
        echo "  alice consolidate         Consolidate 2 smallest UTXOs to new receive address"
        echo ""
        echo "Bob Commands (unfunded wallet):"
        echo "  bob balance               Show Bob wallet balance"
        echo "  bob address               Generate new Bob address"
        echo "  bob send <amt>            Send Bitcoin from Bob to Alice (RBF-enabled)"
        echo "  bob rbf <txid> [rate]     Replace transaction with higher fee (default: 10 sat/byte)"
        echo "  bob cpfp <txid>           Create CPFP child transaction for Bob's unconfirmed output"
        echo "  bob consolidate           Consolidate 2 smallest UTXOs to new receive address"
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
        echo "Backend Integration:"
        echo "  add-wallets-to-backend [url]    Add Alice/Bob wallets to backend (default: http://localhost:3001)"
        echo "  remove-wallets-from-backend [url] Remove regtest wallets from backend"
        echo ""
        echo "Examples:"
        echo "  $0 start                             # Start the environment"
        echo "  $0 create-wallets                    # Create Alice/Bob wallets (Alice gets 1 BTC distributed)"
        echo "  $0 add-wallets-to-backend            # Add Alice/Bob to your backend"
        echo "  $0 mine 6                            # Mine 6 blocks"
        echo "  $0 alice send 0.5                    # Send 0.5 BTC from Alice to Bob (RBF-enabled)"
        echo "  $0 alice rbf <txid> 15               # Replace transaction with 15 sat/byte fee (Alice)"
        echo "  $0 bob send 0.01                     # Send 0.01 BTC from Bob to Alice"
        echo "  $0 alice cpfp <txid>                 # Alice creates CPFP child for parent transaction"
        echo "  $0 alice consolidate                 # Consolidate Alice's 2 smallest UTXOs"
        echo "  $0 mine 1                            # Mine 1 block (confirms pending transactions)"
        echo "  $0 mempool-status                    # Check mempool"
        echo "  $0 get-mempool-txid 0                # Get first transaction from mempool"
        echo "  $0 mempool-purge restart             # Purge mempool via node restart"
        echo "  $0 reorg                             # 1-block reorganization"
        echo "  $0 run-tests bcrt1q...               # Run full test suite with wallet address"
        echo "  $0 reset                             # Reset everything (includes backend cleanup)"
        ;;
esac