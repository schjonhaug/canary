get_target_address() {
    local destination="$1"
    local raw_address="${2:-}"

    if [ -n "$raw_address" ]; then
        echo "$raw_address"
        return
    fi

    case "$destination" in
        *-address)
            get_single_address_wallet_address "$destination"
            ;;
        *)
            btc_wallet "$destination" getnewaddress "" "$(get_address_type "$destination")"
            ;;
    esac
}

send_from_wallet() {
    local wallet="$1"
    local target="$2"
    local amount="$3"
    local subtract_fee="${4:-false}"
    local addr_type response send_opts own_addr

    addr_type=$(get_address_type "$wallet")
    case "$wallet" in
        *-address)
            own_addr=$(get_single_address_wallet_address "$wallet")
            send_opts="{\"change_address\": \"$own_addr\"}"
            if [ "$subtract_fee" = "true" ]; then
                send_opts="{\"change_address\": \"$own_addr\", \"subtract_fee_from_outputs\": [0]}"
            fi
            response=$(btc_wallet "$wallet" send "{\"$target\": $amount}" null "unset" null "$send_opts" 2>&1) || {
                echo "❌ Failed to send from $wallet: $response" >&2
                return 1
            }
            echo "$response" | jq -r '.txid'
            ;;
        *)
            if [ "$addr_type" != "bech32" ]; then
                send_opts="{\"change_type\": \"$addr_type\"}"
                if [ "$subtract_fee" = "true" ]; then
                    send_opts="{\"change_type\": \"$addr_type\", \"subtract_fee_from_outputs\": [0]}"
                fi
                response=$(btc_wallet "$wallet" send "{\"$target\": $amount}" null "unset" null "$send_opts" 2>&1) || {
                    echo "❌ Failed to send from $wallet: $response" >&2
                    return 1
                }
                echo "$response" | jq -r '.txid'
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

wallet_sending_impl() {
    local wallet="$1"
    shift
    local destination_wallet="$1"
    shift
    local amounts=("$@")
    local raw_address=""

    if [ -z "$destination_wallet" ] || [ ${#amounts[@]} -eq 0 ]; then
        echo "Usage: $0 $wallet sending <destination_wallet> <amount1> [amount2] [amount3] ..."
        echo "       $0 $wallet sending <destination_wallet> max  # Drain wallet"
        echo "Available destinations: segwit-desc, segwit-empty, legacy-desc, legacy-empty, nested-desc, nested-empty, taproot-desc, taproot-empty, charlie, miner"
        echo "Examples:"
        echo "  $0 $wallet sending segwit-empty 0.1 0.2 0.05  # Send three separate transactions"
        echo "  $0 $wallet sending miner max                  # Drain wallet to miner"
        return 1
    fi

    if is_wallet_name "$destination_wallet"; then
        :
    elif is_raw_bitcoin_address "$destination_wallet"; then
        raw_address="$destination_wallet"
    else
        echo "❌ Invalid destination: $destination_wallet"
        echo "Use a wallet name or a raw Bitcoin address (bcrt1..., tb1..., bc1...)"
        echo "Available wallets: segwit-desc, segwit-empty, legacy-desc, legacy-empty, nested-desc, nested-empty, taproot-desc, taproot-empty, charlie, miner, legacy-address, p2sh-address, segwit-address, taproot-address"
        return 1
    fi

    if [ "$wallet" = "miner" ] && [ "$destination_wallet" = "miner" ]; then
        echo "❌ Miner wallet cannot send to itself"
        echo "Miner can send to: segwit-desc, segwit-empty, charlie, etc."
        return 1
    fi

    load_wallet_if_needed "$wallet"
    if [ -z "$raw_address" ]; then
        load_wallet_if_needed "$destination_wallet"
    fi

    if [ "${amounts[0]}" = "max" ] && [ ${#amounts[@]} -eq 1 ]; then
        local target_address current_balance txid
        target_address=$(get_target_address "$destination_wallet" "$raw_address")
        current_balance=$(btc_wallet "$wallet" getbalance)
        echo "🎯 Draining $wallet wallet ($current_balance BTC) to $destination_wallet address: $target_address"
        txid=$(send_from_wallet "$wallet" "$target_address" "$current_balance" true) || return 1
        echo "✅ Transaction sent: $txid"
        echo "💡 Use '$0 mine' to confirm transaction"
        return 0
    fi

    echo "🎯 Sending ${#amounts[@]} separate transactions from $wallet to $destination_wallet"
    local txids=()
    local i amount target_address txid
    for i in "${!amounts[@]}"; do
        amount="${amounts[$i]}"
        target_address=$(get_target_address "$destination_wallet" "$raw_address")
        echo "  📤 Transaction $((i+1))/${#amounts[@]}: Sending $amount BTC to address $target_address"
        txid=$(send_from_wallet "$wallet" "$target_address" "$amount") || return 1
        txids+=("$txid")
        echo "     ✅ Transaction $((i+1)) sent: $txid"
    done

    echo ""
    echo "🎉 All ${#amounts[@]} transactions sent successfully:"
    for i in "${!txids[@]}"; do
        echo "  $((i+1)). ${amounts[$i]} BTC → ${txids[$i]}"
    done
    echo "💡 Use '$0 mine' to confirm all transactions"
    return 0
}

cmd_wallet_sending() {
    wallet_sending_impl "$@"
    return $?
}

cmd_wallet_sent() {
    local wallet="$1"
    shift
    wallet_sending_impl "$wallet" "$@" || return $?
    echo "⛏️  Mining 1 block to confirm all transactions..."
    mine_blocks 1
    echo "✅ All transactions confirmed in block"
    return 0
}

cmd_wallet_balance() {
    local wallet="$1"
    load_wallet_if_needed "$wallet"
    local balance
    balance=$(btc_wallet "$wallet" getbalance)
    echo "$wallet wallet balance: $balance BTC"
    return 0
}

cmd_wallet_address() {
    local wallet="$1"
    load_wallet_if_needed "$wallet"
    local address
    address=$(btc_wallet "$wallet" getnewaddress "" "$(get_address_type "$wallet")")
    echo "New $wallet address: $address"
    return 0
}

cmd_wallet_fund() {
    local wallet="$1"
    local target_address="$2"
    local amount="${3:-1.0}"
    if [ -z "$target_address" ]; then
        echo "Usage: $0 $wallet fund <address> [amount=1.0]"
        return 1
    fi
    load_wallet_if_needed "$wallet"
    echo "Funding address $target_address with $amount BTC from $wallet..."
    local txid
    txid=$(btc_wallet "$wallet" sendtoaddress "$target_address" "$amount")
    echo "Transaction: $txid"
    echo "💡 Use '$0 mine' to confirm transaction"
    return 0
}

cmd_wallet_rbf() {
    local wallet="$1"
    local txid="$2"
    if [ -z "$txid" ]; then
        echo "Usage: $0 $wallet rbf <txid>"
        return 1
    fi
    echo "🔄 Bumping fee for transaction $txid (automatic fee calculation)..."
    load_wallet_if_needed "$wallet"
    local result
    result=$(btc_wallet "$wallet" bumpfee "$txid" 2>&1)
    if echo "$result" | jq -e '.txid' > /dev/null 2>&1; then
        local new_txid old_fee new_fee
        new_txid=$(echo "$result" | jq -r '.txid')
        old_fee=$(echo "$result" | jq -r '.origfee')
        new_fee=$(echo "$result" | jq -r '.fee')
        echo "✅ RBF replacement successful!"
        echo "   Original TXID: $txid"
        echo "   New TXID: $new_txid"
        echo "   Original fee: $old_fee BTC"
        echo "   New fee: $new_fee BTC"
        echo "💡 Use '$0 mine' to confirm when ready"
    else
        echo "❌ RBF failed: $result"
        echo "💡 Common reasons:"
        echo "   - Transaction already confirmed"
        echo "   - Transaction was not RBF-enabled"
        echo "   - Fee rate not higher than original"
        return 1
    fi
    return 0
}

cmd_wallet_consolidate() {
    local wallet="$1"
    echo "🔄 Consolidating 2 smallest UTXOs for $wallet..."
    load_wallet_if_needed "$wallet"

    if ! btc_wallet "$wallet" getwalletinfo >/dev/null 2>&1; then
        echo "❌ $wallet wallet not found. Run '$0 init' first"
        return 1
    fi

    local utxos utxo_count utxo1 utxo2 amount1 amount2 txid1 txid2 vout1 vout2 total_amount consolidate_amount consolidate_address inputs outputs raw_tx signed_tx signed_hex sign_complete consolidate_txid
    utxos=$(btc_wallet "$wallet" listunspent | jq -r '.[] | "\(.amount) \(.txid) \(.vout)"' | sort -n)
    utxo_count=$(echo "$utxos" | wc -l | tr -d ' ')
    if [ "$utxo_count" -lt 2 ]; then
        echo "❌ $wallet needs at least 2 UTXOs to consolidate. Current UTXOs: $utxo_count"
        echo "💡 Fund $wallet with multiple transactions first"
        return 1
    fi

    utxo1=$(echo "$utxos" | head -1)
    utxo2=$(echo "$utxos" | head -2 | tail -1)
    amount1=$(echo "$utxo1" | cut -d' ' -f1)
    txid1=$(echo "$utxo1" | cut -d' ' -f2)
    vout1=$(echo "$utxo1" | cut -d' ' -f3)
    amount2=$(echo "$utxo2" | cut -d' ' -f1)
    txid2=$(echo "$utxo2" | cut -d' ' -f2)
    vout2=$(echo "$utxo2" | cut -d' ' -f3)

    echo "   📍 UTXO 1: $amount1 BTC (txid: $txid1, vout: $vout1)"
    echo "   📍 UTXO 2: $amount2 BTC (txid: $txid2, vout: $vout2)"

    total_amount=$(LC_NUMERIC=C awk "BEGIN {printf \"%.8f\", $amount1 + $amount2}")
    consolidate_amount=$(LC_NUMERIC=C awk "BEGIN {printf \"%.8f\", $amount1 + $amount2 - 0.0001}")
    echo "   💰 Total: $total_amount BTC → $consolidate_amount BTC (0.0001 BTC fee)"

    consolidate_address=$(btc_wallet "$wallet" getrawchangeaddress "$(get_address_type "$wallet")")
    echo "   🎯 Consolidating to: $consolidate_address"
    inputs="[{\"txid\":\"$txid1\",\"vout\":$vout1},{\"txid\":\"$txid2\",\"vout\":$vout2}]"
    outputs="{\"$consolidate_address\":$consolidate_amount}"

    echo "   🔧 Creating consolidation transaction..."
    raw_tx=$(btc_wallet "$wallet" createrawtransaction "$inputs" "$outputs")
    if [ -z "$raw_tx" ]; then
        echo "❌ Failed to create raw transaction"
        return 1
    fi

    echo "   ✍️  Signing transaction..."
    signed_tx=$(btc_wallet "$wallet" signrawtransactionwithwallet "$raw_tx")
    signed_hex=$(echo "$signed_tx" | jq -r '.hex')
    sign_complete=$(echo "$signed_tx" | jq -r '.complete')
    if [ "$sign_complete" != "true" ]; then
        echo "❌ Failed to sign transaction"
        echo "Signing result: $signed_tx"
        return 1
    fi

    echo "   📡 Broadcasting consolidation transaction..."
    consolidate_txid=$(btc sendrawtransaction "$signed_hex")
    if [ -z "$consolidate_txid" ]; then
        echo "❌ Failed to broadcast transaction"
        return 1
    fi

    echo "   ✅ Consolidation transaction created: $consolidate_txid"
    echo "   💰 Consolidated: $consolidate_amount BTC"
    echo "   🎯 Address: $consolidate_address"
    echo ""
    echo "🔗 Consolidation Summary:"
    echo "   Input 1: $amount1 BTC from $txid1:$vout1"
    echo "   Input 2: $amount2 BTC from $txid2:$vout2"
    echo "   Output:  $consolidate_amount BTC to $consolidate_address"
    echo "   Fee:     0.0001 BTC"
    echo ""
    echo "💡 Use '$0 mine 1' to confirm the consolidation"
    return 0
}

handle_wallet_command() {
    local wallet="$1"
    local subcmd="$2"

    if ! is_wallet_name "$wallet"; then
        return 1
    fi

    shift 2
    case "$subcmd" in
        "")
            echo "Usage: $0 $wallet <sending|sent|balance|address|fund|rbf|cpfp|consolidate> [args]"
            return 1
            ;;
        sending)
            cmd_wallet_sending "$wallet" "$@"
            ;;
        sent)
            cmd_wallet_sent "$wallet" "$@"
            ;;
        balance)
            cmd_wallet_balance "$wallet"
            ;;
        address)
            cmd_wallet_address "$wallet"
            ;;
        fund)
            cmd_wallet_fund "$wallet" "$@"
            ;;
        rbf)
            cmd_wallet_rbf "$wallet" "$1"
            ;;
        cpfp)
            cpfp_for_wallet "$wallet" "$1"
            ;;
        consolidate)
            cmd_wallet_consolidate "$wallet"
            ;;
        *)
            echo "Unknown subcommand for $wallet: $subcmd"
            return 1
            ;;
    esac

    return $?
}
