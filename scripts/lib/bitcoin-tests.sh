cpfp_for_wallet() {
    local wallet="$1"
    local parent_txid="$2"
    if [ -z "$parent_txid" ]; then
        echo "Usage: $0 $wallet cpfp <parent_txid>"
        exit 1
    fi
    if ! btc_wallet "$wallet" getwalletinfo >/dev/null 2>&1; then
        echo "❌ $wallet wallet not found. Run '$0 init' first"
        exit 1
    fi
    local wallet_balance
    wallet_balance=$(btc_wallet "$wallet" getbalance)
    echo "💰 $wallet wallet balance: $wallet_balance BTC (confirmed)"
    if [ "$(compare_decimal "$wallet_balance < 0.001")" -eq 1 ]; then
        echo "❌ $wallet needs confirmed funds for CPFP fees. Current balance: $wallet_balance BTC"
        echo "💡 Fund $wallet first with: $0 miner sending $wallet 0.01 && $0 mine 1"
        exit 1
    fi
    echo "👶 Creating CPFP child transaction ($wallet spends unconfirmed output)..."
    local parent_in_wallet
    parent_in_wallet=$(btc_wallet "$wallet" gettransaction "$parent_txid" 2>/dev/null || echo "not found")
    if [ "$parent_in_wallet" = "not found" ]; then
        echo "❌ Parent transaction not found in $wallet wallet"
        exit 1
    fi
    local parent_confirmations parent_amount
    parent_confirmations=$(echo "$parent_in_wallet" | jq -r '.confirmations')
    parent_amount=$(echo "$parent_in_wallet" | jq -r '.amount')
    if [ "$parent_confirmations" -gt 0 ]; then
        echo "❌ Parent transaction is already confirmed ($parent_confirmations confirmations)"
        echo "💡 CPFP only works on unconfirmed transactions"
        exit 1
    fi
    echo "   ✅ Parent transaction found in $wallet wallet (unconfirmed)"
    echo "   💰 Parent amount: $parent_amount BTC"
    local parent_raw wallet_outputs total_wallet_amount output_count i output_address output_value
    parent_raw=$(btc getrawtransaction "$parent_txid" true)
    wallet_outputs=()
    total_wallet_amount=0
    output_count=$(echo "$parent_raw" | jq '.vout | length')
    for ((i=0; i<output_count; i++)); do
        output_address=$(echo "$parent_raw" | jq -r ".vout[$i].scriptPubKey.address")
        output_value=$(echo "$parent_raw" | jq -r ".vout[$i].value")
        if btc_wallet "$wallet" getaddressinfo "$output_address" 2>/dev/null | jq -r '.ismine' | grep -q "true"; then
            wallet_outputs+=("$i:$output_value")
            total_wallet_amount=$(echo "scale=8; $total_wallet_amount + $output_value" | bc -l)
            echo "   📍 Found $wallet's output $i: $output_value BTC at $output_address"
        fi
    done
    if [ ${#wallet_outputs[@]} -eq 0 ]; then
        echo "❌ No outputs in parent transaction belong to $wallet's wallet"
        exit 1
    fi
    local half_amount dynamic_fee min_fee_flexible min_fee_absolute min_fee child_amount_raw child_amount min_child_amount change_address inputs output_index outputs raw_tx signed_tx signed_hex sign_complete child_txid mempool_size
    half_amount=$(echo "scale=8; $total_wallet_amount * 0.5" | bc -l)
    if [ "$(compare_decimal "$half_amount > 0.005")" -eq 1 ]; then
        dynamic_fee=0.005
    else
        dynamic_fee=$half_amount
    fi
    min_fee_flexible=$(echo "scale=8; $total_wallet_amount * 0.8" | bc -l)
    min_fee_absolute=0.00001
    if [ "$(compare_decimal "$min_fee_flexible < $min_fee_absolute")" -eq 1 ]; then
        min_fee=$min_fee_flexible
    else
        min_fee=$min_fee_absolute
    fi
    if [ "$(compare_decimal "$dynamic_fee < $min_fee")" -eq 1 ]; then
        dynamic_fee=$min_fee
    fi
    child_amount_raw=$(echo "scale=8; $total_wallet_amount - $dynamic_fee" | bc -l)
    child_amount=$(echo "$child_amount_raw" | sed 's/^\./0./')
    min_child_amount=0.00001
    if [ "$(compare_decimal "$child_amount < $min_child_amount")" -eq 1 ]; then
        echo "❌ Child amount too small: $child_amount BTC (need at least $min_child_amount BTC after fees)"
        echo "   Available: $total_wallet_amount BTC, Required fee: $dynamic_fee BTC"
        exit 1
    fi
    change_address=$(btc_wallet "$wallet" getnewaddress "" "$(get_address_type "$wallet")")
    echo "   🔍 Creating CPFP child spending $total_wallet_amount BTC → $child_amount BTC ($dynamic_fee BTC fee)"
    echo "   🎯 Target: $change_address"
    inputs="["
    for i in "${!wallet_outputs[@]}"; do
        output_index=$(echo "${wallet_outputs[$i]}" | cut -d':' -f1)
        if [ "$i" -gt 0 ]; then
            inputs+=","
        fi
        inputs+="{\"txid\":\"$parent_txid\",\"vout\":$output_index}"
    done
    inputs+="]"
    outputs="{\"$change_address\":$child_amount}"
    echo "   🔧 Creating raw transaction..."
    raw_tx=$(btc_wallet "$wallet" createrawtransaction "$inputs" "$outputs")
    if [ -z "$raw_tx" ]; then
        echo "❌ Failed to create raw transaction"
        exit 1
    fi
    echo "   ✍️  Signing transaction..."
    signed_tx=$(btc_wallet "$wallet" signrawtransactionwithwallet "$raw_tx")
    signed_hex=$(echo "$signed_tx" | jq -r '.hex')
    sign_complete=$(echo "$signed_tx" | jq -r '.complete')
    if [ "$sign_complete" != "true" ]; then
        echo "❌ Failed to sign transaction"
        echo "Signing result: $signed_tx"
        exit 1
    fi
    echo "   📡 Broadcasting CPFP child transaction..."
    child_txid=$(btc sendrawtransaction "$signed_hex")
    if [ -z "$child_txid" ]; then
        echo "❌ Failed to create child transaction"
        echo "   $wallet balance (confirmed): $(btc_wallet "$wallet" getbalance)"
        echo "   $wallet balance (unconfirmed): $(btc_wallet "$wallet" getbalance "*" 0)"
        exit 1
    fi
    echo "   ✅ Child transaction created: $child_txid"
    echo "   💰 Amount: $child_amount BTC (high fee: $dynamic_fee BTC)"
    echo "   🎯 Target: $change_address ($wallet change address)"
    echo ""
    echo "🔗 CPFP Relationship Created:"
    echo "   👨 Parent: $parent_txid ($wallet → $wallet, stuck due to low fee)"
    echo "   👶 Child:  $child_txid ($wallet → $wallet, high fee accelerates parent)"
    echo ""
    echo "📊 Current mempool status:"
    mempool_size=$(btc getmempoolinfo | jq -r '.size')
    echo "   Transactions in mempool: $mempool_size"
    echo ""
    echo "🔍 Transaction Details:"
    echo "Parent transaction ($wallet wallet view):"
    btc_wallet "$wallet" gettransaction "$parent_txid" | jq -r '"   Fee: " + (.fee | tostring) + " BTC, Confirmations: " + (.confirmations | tostring)'
    echo ""
    echo "Child transaction ($wallet wallet view):"
    btc_wallet "$wallet" gettransaction "$child_txid" | jq -r '"   Fee: " + (.fee | tostring) + " BTC, Confirmations: " + (.confirmations | tostring)'
    echo ""
    echo "🎉 CPFP test scenario complete!"
    echo ""
    echo "📱 Check your application to see:"
    echo "   - Both transactions appear in mempool"
    echo "   - $wallet's balance shows pending amounts"
    echo "   - CPFP relationship should be detected"
    echo ""
    echo "⛏️  Mine blocks to confirm both transactions:"
    echo "   $0 mine 1"
    echo ""
    echo "🔍 Both transactions should confirm together due to CPFP!"
}

get_mempool_txid() {
    local index=${1:-0}
    local mempool_txids txid

    mempool_txids=$(btc getrawmempool)
    if [ "$mempool_txids" = "[]" ]; then
        echo "Error: Mempool is empty" >&2
        return 1
    fi

    txid=$(echo "$mempool_txids" | jq -r ".[$index] // empty")
    if [ -z "$txid" ] || [ "$txid" = "null" ]; then
        echo "Error: No transaction found at index $index" >&2
        echo "Available transactions:" >&2
        echo "$mempool_txids" | jq -r '.[]' | nl -v0 >&2
        return 1
    fi

    echo "$txid"
}

mempool_purge() {
    local method=${1:-restart}
    echo "🗑️  Testing Mempool Purge using method: $method"

    case "$method" in
        restart)
            echo "🔄 Method: Bitcoin node restart (simulates mempool purge)"
            echo "📊 Current mempool before restart:"
            local mempool_before
            mempool_before=$(btc getrawmempool)
            echo "$mempool_before" | jq length
            echo "$mempool_before" | jq -r '.[]' | head -5
            if [ "$(echo "$mempool_before" | jq length)" -eq 0 ]; then
                echo "⚠️  Mempool is empty. Creating test transaction first..."
                local new_address
                new_address=$(btc_segwit_desc getnewaddress)
                load_wallet_if_needed "segwit-desc"
                btc_segwit_desc sendtoaddress "$new_address" 0.001
                echo "✅ Created test transaction"
                mempool_before=$(btc getrawmempool)
            fi
            echo ""
            echo "🛑 Stopping Bitcoin node..."
            docker stop bitcoind-regtest
            echo "⏳ Waiting 3 seconds..."
            sleep 3
            echo "🚀 Starting Bitcoin node..."
            docker start bitcoind-regtest
            echo "⏳ Waiting for Bitcoin Core to be ready..."
            local timeout=30
            while [ "$timeout" -gt 0 ]; do
                if btc getblockchaininfo > /dev/null 2>&1; then
                    echo "✅ Bitcoin Core is ready"
                    break
                fi
                sleep 1
                timeout=$((timeout-1))
            done
            echo ""
            echo "📊 Mempool after restart:"
            local mempool_after
            mempool_after=$(btc getrawmempool)
            echo "$mempool_after" | jq length
            if [ "$(echo "$mempool_after" | jq length)" -eq 0 ]; then
                echo "✅ SUCCESS: Mempool was purged during restart"
            else
                echo "⚠️  WARNING: Mempool was NOT purged during restart"
                echo "   This may be due to mempool persistence being enabled"
                echo "   Check bitcoin.conf for 'persistmempool=0' setting"
                echo "   Transactions remaining:"
                echo "$mempool_after" | jq -r '.[]' | head -5
            fi
            echo ""
            echo "🎯 Result: Mempool should be empty after restart"
            echo "   This simulates various purge scenarios like:"
            echo "   - Node restart"
            echo "   - Memory pressure eviction"
            echo "   - Network partition recovery"
            ;;
        double-spend)
            echo "💰 Method: Double-spend conflict (one tx will be purged)"
            echo "🔍 Finding UTXO to double-spend..."
            load_wallet_if_needed "segwit-desc"
            local utxos
            utxos=$(btc_segwit_desc listunspent 1)
            if [ "$(echo "$utxos" | jq length)" -eq 0 ]; then
                echo "❌ No confirmed UTXOs available for double-spend test"
                echo "💡 Mine some blocks first: $0 mine 6"
                return 1
            fi
            local utxo utxo_txid utxo_vout utxo_amount address1 address2 send_amount raw_tx1 signed_tx1 txid1 raw_tx2 signed_tx2
            utxo=$(echo "$utxos" | jq -r '.[0]')
            utxo_txid=$(echo "$utxo" | jq -r '.txid')
            utxo_vout=$(echo "$utxo" | jq -r '.vout')
            utxo_amount=$(echo "$utxo" | jq -r '.amount')
            echo "📋 Using UTXO: $utxo_txid:$utxo_vout ($utxo_amount BTC)"
            address1=$(btc_segwit_desc getnewaddress)
            address2=$(btc_segwit_desc getnewaddress)
            send_amount=$(echo "scale=8; $utxo_amount - 0.001" | bc -l)
            echo "🚀 Creating first transaction to $address1..."
            raw_tx1=$(btc createrawtransaction "[{\"txid\":\"$utxo_txid\",\"vout\":$utxo_vout}]" "{\"$address1\":$send_amount}")
            signed_tx1=$(btc_segwit_desc signrawtransactionwithwallet "$raw_tx1" | jq -r '.hex')
            txid1=$(btc sendrawtransaction "$signed_tx1")
            echo "✅ First transaction: $txid1"
            echo "🚀 Creating conflicting transaction to $address2..."
            raw_tx2=$(btc createrawtransaction "[{\"txid\":\"$utxo_txid\",\"vout\":$utxo_vout}]" "{\"$address2\":$send_amount}")
            signed_tx2=$(btc_segwit_desc signrawtransactionwithwallet "$raw_tx2" | jq -r '.hex')
            echo "🚀 Attempting to send conflicting transaction..."
            if btc sendrawtransaction "$signed_tx2" 2>/dev/null; then
                echo "❌ Unexpected: Second transaction was accepted"
            else
                echo "✅ Expected: Second transaction rejected (double-spend)"
            fi
            echo ""
            echo "🎯 Result: First transaction should remain in mempool"
            echo "   Second transaction should be rejected/purged"
            echo "   This demonstrates conflict resolution"
            ;;
        low-fee)
            echo "💸 Method: Low-fee transaction (may be purged under fee pressure)"
            echo "🚀 Creating very low-fee transaction..."
            load_wallet_if_needed "segwit-desc"
            local new_address low_fee_rate txid tx_info fee size fee_rate
            new_address=$(btc_segwit_desc getnewaddress)
            low_fee_rate=0.00000001
            txid=$(btc_segwit_desc sendtoaddress "$new_address" 0.001 "" "" false true "$low_fee_rate" "unset" 2>/dev/null || echo "")
            if [ -n "$txid" ]; then
                echo "✅ Low-fee transaction created: $txid"
                tx_info=$(btc getmempoolentry "$txid" 2>/dev/null || echo "")
                if [ -n "$tx_info" ]; then
                    fee=$(echo "$tx_info" | jq -r '.fees.base')
                    size=$(echo "$tx_info" | jq -r '.size')
                    fee_rate=$(echo "scale=8; $fee * 100000000 / $size" | bc -l)
                    echo "📊 Transaction details:"
                    echo "   Fee: $fee BTC"
                    echo "   Size: $size bytes"
                    echo "   Fee rate: $fee_rate sat/byte"
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
            echo "❌ Unknown method: $method"
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
    echo ""
    echo "📊 Final mempool status:"
    btc getmempoolinfo
}

reorg() {
    echo "🔄 Testing Blockchain Reorganization"
    echo "📊 Current blockchain state:"
    local initial_height initial_tip
    initial_height=$(btc getblockcount)
    initial_tip=$(btc getbestblockhash)
    echo "   Height: $initial_height"
    echo "   Tip: $initial_tip"
    echo ""
    echo "💰 Creating test transaction before reorg..."
    load_wallet_if_needed "segwit-desc"
    load_wallet_if_needed "segwit-empty"
    local empty_address test_txid tx_info confirmations tip_hash tip_height new_height new_tip tx_info_after confirmations_after final_height final_tip tx_info_final confirmations_final
    empty_address=$(btc_segwit_empty getnewaddress)
    test_txid=$(btc_segwit_desc sendtoaddress "$empty_address" 0.001)
    echo "✅ Test transaction: $test_txid"
    echo "   segwit-desc → segwit-empty: 0.001 BTC"
    echo "   🎯 segwit-empty address: $empty_address"
    echo ""
    echo "⏸️  After the tx is found in the mempool, press enter to mine a block"
    read -r
    echo "⛏️  Mining 1 block to confirm transaction..."
    mine_blocks 1
    echo "📊 After mining:"
    echo "   Height: $(btc getblockcount)"
    echo "   Tip: $(btc getbestblockhash)"
    tx_info=$(btc_segwit_desc gettransaction "$test_txid")
    confirmations=$(echo "$tx_info" | jq -r '.confirmations')
    echo "   Transaction confirmations: $confirmations"
    echo ""
    echo "⏸️  Press enter to invalidate the tip block"
    read -r
    tip_hash=$(btc getbestblockhash)
    tip_height=$(btc getblockcount)
    echo ""
    echo "🔄 Starting reorganization..."
    echo "   Invalidating tip block: $tip_hash (height: $tip_height)"
    echo "   This will move transaction back to mempool"
    echo "🚫 Invalidating tip block..."
    btc invalidateblock "$tip_hash"
    echo "✅ Tip block invalidated successfully"
    new_height=$(btc getblockcount)
    new_tip=$(btc getbestblockhash)
    echo "📊 After invalidation:"
    echo "   Height: $new_height"
    echo "   Tip: $new_tip"
    echo ""
    echo "🔍 Checking transaction status after reorg..."
    tx_info_after=$(btc_segwit_desc gettransaction "$test_txid" 2>/dev/null || echo "{}")
    confirmations_after=$(echo "$tx_info_after" | jq -r '.confirmations // 0')
    if [ "$confirmations_after" -eq 0 ]; then
        echo "✅ Transaction is back in mempool (0 confirmations)"
        if btc getmempoolentry "$test_txid" > /dev/null 2>&1; then
            echo "✅ Confirmed: Transaction is in mempool"
        else
            echo "⚠️  Transaction not found in mempool (may have been dropped)"
        fi
    else
        echo "⚠️  Transaction still has $confirmations_after confirmations"
    fi
    echo ""
    echo "⏸️  Press enter to mine a new block, completing the reorg"
    read -r
    echo ""
    echo "⛏️  Mining new block to re-confirm transaction..."
    mine_blocks 1
    final_height=$(btc getblockcount)
    final_tip=$(btc getbestblockhash)
    echo "📊 Final state:"
    echo "   Height: $final_height"
    echo "   Tip: $final_tip"
    echo ""
    echo "🔍 Final transaction status..."
    tx_info_final=$(btc_segwit_desc gettransaction "$test_txid" 2>/dev/null || echo "{}")
    confirmations_final=$(echo "$tx_info_final" | jq -r '.confirmations // 0')
    if [ "$confirmations_final" -gt 0 ]; then
        echo "✅ Transaction re-confirmed with $confirmations_final confirmations"
    elif [ "$confirmations_final" -eq 0 ]; then
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
    echo "   Initial height: $initial_height → Final height: $final_height"
    echo "   Transaction: $test_txid"
    echo "   Final confirmations: $confirmations_final"
    echo ""
    echo "🔍 Monitor your backend logs for reorg detection messages!"
    echo ""
    echo "📊 Final mempool status:"
    btc getmempoolinfo
}

run_tests() {
    local wallet_address=${1:-}
    echo "🧪 Canary - Comprehensive Bitcoin Test Suite"
    echo "=========================================="
    if [ -z "$wallet_address" ]; then
        echo "⚠️  No wallet address provided. You'll need to:"
        echo "   1. Start your application"
        echo "   2. Add a test wallet"
        echo "   3. Get an address from that wallet"
        echo "   4. Run: $0 run-tests <wallet_address>"
        echo ""
        echo "Example: $0 run-tests bcrt1qtest123456789abcdef"
        return 1
    fi

    pause_test() {
        echo ""
        echo "⏸️  Pausing for 5 seconds to observe changes..."
        echo "   Check your application for updates!"
        sleep 5
        echo ""
    }

    echo "🚀 Starting comprehensive test suite with address: $wallet_address"
    echo ""
    echo "📍 TEST 1: Basic Mempool Transaction"
    echo "-----------------------------------"
    load_wallet_if_needed "segwit-desc"
    btc_segwit_desc sendtoaddress "$wallet_address" 0.001
    pause_test

    echo "📍 TEST 2: RBF (Replace-By-Fee)"
    echo "-------------------------------"
    echo "Creating low-fee transaction for RBF testing..."
    local first_txid result new_txid parent_txid
    first_txid=$(btc_segwit_desc sendtoaddress "$wallet_address" 0.002 "" "" false true 0.00001 "unset")
    echo "First transaction: $first_txid"
    sleep 2
    echo "Attempting RBF replacement with bumpfee..."
    result=$(btc_segwit_desc bumpfee "$first_txid" "{\"fee_rate\": 15}" 2>&1 || echo "RBF failed")
    if echo "$result" | jq -e '.txid' > /dev/null 2>&1; then
        new_txid=$(echo "$result" | jq -r '.txid')
        echo "✅ RBF successful: $new_txid"
    else
        echo "❌ RBF failed: $result"
    fi
    pause_test

    echo "📍 TEST 3: CPFP (Child-Pays-For-Parent)"
    echo "---------------------------------------"
    echo "Creating low-fee parent transaction..."
    parent_txid=$(btc_segwit_desc sendtoaddress "$wallet_address" 0.003 "" "" false true 0.00001 "unset")
    echo "Parent transaction: $parent_txid"
    sleep 2
    echo "Creating CPFP child transaction..."
    cpfp_for_wallet "segwit-desc" "$parent_txid"
    pause_test

    echo "📍 TEST 4: Mempool Purge (Node Restart)"
    echo "---------------------------------------"
    echo "Creating transaction to be purged..."
    btc_segwit_desc sendtoaddress "$wallet_address" 0.001
    sleep 2
    echo "Testing mempool purge via restart..."
    mempool_purge "restart"
    pause_test

    echo "📍 TEST 5: Blockchain Reorganization"
    echo "------------------------------------"
    echo "Creating transaction for reorg testing..."
    btc_segwit_desc sendtoaddress "$wallet_address" 0.004
    sleep 2
    echo "Mining blocks to confirm transaction..."
    mine_blocks 3
    sleep 2
    echo "Testing blockchain reorganization..."
    reorg
    pause_test

    echo "📍 TEST 6: Transaction Confirmation"
    echo "-----------------------------------"
    echo "Creating final test transaction..."
    btc_segwit_desc sendtoaddress "$wallet_address" 0.005
    sleep 2
    echo "Confirming transaction..."
    mine_blocks 1
    pause_test

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
    echo "📊 Final blockchain state:"
    btc getblockchaininfo | jq '.blocks, .bestblockhash'
    echo ""
    echo "📊 Final mempool state:"
    btc getmempoolinfo
    echo ""
    echo "🎉 Test suite completed successfully!"
    echo "Monitor your backend logs and application for all the changes!"
}
