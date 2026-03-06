cmd_create_stress_wallet() {
    require_tools jq bc
    local tx_count="${1:-}"
    local wallet_name
    local stress_descriptors stress_receive_desc stress_multipath_raw stress_checksum_info stress_checksum stress_descriptor
    local miner_address miner_balance needed_btc blocks_needed
    local initial_fund stress_addr
    local completed batch_size start_time remaining current_batch
    local i dest_addr refund_addr elapsed rate eta
    local total_time final_balance tx_list_count

    if [ -z "$tx_count" ]; then
        echo "Usage: $0 create-stress-wallet <tx_count>"
        echo "Example: $0 create-stress-wallet 1000"
        exit 1
    fi

    wallet_name="stress-${tx_count}tx"

    echo "Creating stress-test wallet '$wallet_name' with $tx_count transactions..."
    echo ""
    echo "1/4 Creating wallet..."
    btc unloadwallet "$wallet_name" 2>/dev/null || true

    set +e
    CREATE_RESULT=$(btc -named createwallet wallet_name="$wallet_name" disable_private_keys=false blank=false passphrase="" avoid_reuse=false descriptors=true 2>&1)
    CREATE_EXIT_CODE=$?
    set -e

    if echo "$CREATE_RESULT" | grep -q "already exists"; then
        echo "   Wallet exists, loading..."
        load_wallet_if_needed "$wallet_name"
    elif [ "$CREATE_EXIT_CODE" -eq 0 ]; then
        echo "   Wallet created"
    else
        echo "   Failed to create wallet: $CREATE_RESULT"
        exit 1
    fi

    stress_descriptors=$(btc_wallet "$wallet_name" listdescriptors)
    stress_receive_desc=$(echo "$stress_descriptors" | jq -r '.descriptors[] | select(.desc | startswith("wpkh(") and contains("/0/*")) | .desc')
    stress_multipath_raw=$(echo "$stress_receive_desc" | sed 's|/0/\*|/<0;1>/\*|' | sed 's/#[^#]*$//')
    stress_checksum_info=$(btc getdescriptorinfo "$stress_multipath_raw")
    stress_checksum=$(echo "$stress_checksum_info" | jq -r '.checksum')
    stress_descriptor="$stress_multipath_raw#$stress_checksum"
    echo "   Descriptor: $stress_descriptor"

    echo ""
    echo "2/4 Ensuring miner is funded..."
    load_wallet_if_needed "miner"
    miner_address=$(btc_miner getnewaddress)
    miner_balance=$(btc_miner getbalance)
    echo "   Miner balance: $miner_balance BTC"

    needed_btc=$(echo "scale=2; $tx_count * 0.002" | bc -l)
    if [ "$(echo "$miner_balance < $needed_btc" | bc -l)" -eq 1 ]; then
        blocks_needed=$(echo "($needed_btc - $miner_balance) / 50 + 2" | bc)
        echo "   Mining $blocks_needed more blocks for funds..."
        btc generatetoaddress "$blocks_needed" "$miner_address" >/dev/null 2>&1
        miner_balance=$(btc_miner getbalance)
        echo "   Miner balance now: $miner_balance BTC"
    fi

    echo ""
    echo "3/4 Initial funding..."
    initial_fund=$(echo "scale=8; $tx_count * 0.002" | bc -l)
    stress_addr=$(btc_wallet "$wallet_name" getnewaddress)
    btc_miner sendtoaddress "$stress_addr" "$initial_fund" >/dev/null 2>&1
    btc generatetoaddress 1 "$miner_address" >/dev/null 2>&1
    echo "   Funded with $initial_fund BTC"

    echo ""
    echo "4/4 Generating $tx_count transactions..."
    echo "   (mining a block every 25 transactions to keep UTXOs confirmed)"
    echo ""

    completed=0
    batch_size=25
    start_time=$(date +%s)

    while [ "$completed" -lt "$tx_count" ]; do
        remaining=$((tx_count - completed))
        current_batch=$((remaining < batch_size ? remaining : batch_size))

        for i in $(seq 1 "$current_batch"); do
            dest_addr=$(btc_miner getnewaddress)
            if ! btc_wallet "$wallet_name" sendtoaddress "$dest_addr" 0.0001 "" "" true >/dev/null 2>&1; then
                refund_addr=$(btc_wallet "$wallet_name" getnewaddress)
                btc_miner sendtoaddress "$refund_addr" 0.5 >/dev/null 2>&1
                btc generatetoaddress 1 "$miner_address" >/dev/null 2>&1
                btc_wallet "$wallet_name" sendtoaddress "$dest_addr" 0.0001 "" "" true >/dev/null 2>&1 || true
            fi
        done

        btc generatetoaddress 1 "$miner_address" >/dev/null 2>&1
        completed=$((completed + current_batch))
        elapsed=$(($(date +%s) - start_time))
        if [ "$elapsed" -gt 0 ]; then
            rate=$((completed / elapsed))
            if [ "$rate" -gt 0 ]; then
                eta=$(((tx_count - completed) / rate))
            else
                eta="?"
            fi
            echo "   [${completed}/${tx_count}] ~${rate} tx/s, ETA: ${eta}s"
        else
            echo "   [${completed}/${tx_count}]"
        fi
    done

    total_time=$(($(date +%s) - start_time))
    final_balance=$(btc_wallet "$wallet_name" getbalance)
    tx_list_count=$(btc_wallet "$wallet_name" listtransactions "*" 999999 | jq 'length')

    echo ""
    echo "Stress wallet '$wallet_name' ready!"
    echo "   Transactions: $tx_list_count"
    echo "   Balance: $final_balance BTC"
    echo "   Time: ${total_time}s"
    echo "   Descriptor: $stress_descriptor"
    echo ""
    echo "To add to backend:"
    echo "   curl -X POST http://localhost:3000/api/wallets -H 'Content-Type: application/json' -d '{\"name\":\"$wallet_name\",\"descriptor\":\"$stress_descriptor\"}'"
}
