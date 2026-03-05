cmd_add_wallets_to_backend() {
    local backend_url="${1:-http://localhost:3000}"
    echo "Adding descriptor wallets to backend at $backend_url..."
    echo "🔍 Checking if backend is running..."
    if ! backend_is_available "$backend_url"; then
        echo "❌ Backend is not running at $backend_url"
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

    echo "📋 Getting wallet descriptors..."
    local wallet_configs=(
        "segwit-desc|.desc | startswith(\"wpkh(\") and contains(\"/0/*\")"
        "legacy-desc|.desc | startswith(\"pkh(\") and contains(\"/0/*\")"
        "nested-desc|.desc | startswith(\"sh(wpkh(\") and contains(\"/0/*\")"
        "taproot-desc|.desc | startswith(\"tr(\") and contains(\"/0/*\")"
        "segwit-empty|.desc | startswith(\"wpkh(\") and contains(\"/0/*\")"
        "legacy-empty|.desc | startswith(\"pkh(\") and contains(\"/0/*\")"
        "nested-empty|.desc | startswith(\"sh(wpkh(\") and contains(\"/0/*\")"
        "taproot-empty|.desc | startswith(\"tr(\") and contains(\"/0/*\")"
    )
    local success_count=0
    local total_count=${#wallet_configs[@]}
    local config wallet_name jq_filter descriptor

    for config in "${wallet_configs[@]}"; do
        IFS='|' read -r wallet_name jq_filter <<< "$config"
        load_wallet_if_needed "$wallet_name"
        descriptor=$(get_wallet_descriptor "$wallet_name" "$jq_filter")
        echo "   $wallet_name: $descriptor"
        if backend_add_wallet "$backend_url" "$wallet_name" "$descriptor" "📱"; then
            success_count=$((success_count + 1))
        fi
    done

    load_wallet_if_needed "charlie"
    descriptor=$(get_wallet_descriptor "charlie" '.desc | startswith("wpkh(") and contains("/0/*")')
    echo "   charlie: $descriptor"
    if backend_add_wallet "$backend_url" "Charlie" "$descriptor" "🎭"; then
        success_count=$((success_count + 1))
    fi
    total_count=$((total_count + 1))

    echo ""
    if [ "$success_count" -eq "$total_count" ]; then
        echo "🎉 All $total_count wallets have been added to the backend!"
        echo "Check your frontend at http://localhost:3001 to see them."
    elif [ "$success_count" -gt 0 ]; then
        echo "⚠️  $success_count/$total_count wallets added successfully."
        echo "Check your frontend at http://localhost:3001 to see what was added."
    else
        echo "❌ Failed to add wallets to the backend."
        echo "Please check the backend logs and try again."
    fi
}

cmd_remove_wallets_from_backend() {
    local backend_url="${1:-http://localhost:3000}"
    echo "Removing regtest wallets from backend at $backend_url..."
    local wallets_response
    wallets_response=$(curl -s "$backend_url/api/wallets")

    if echo "$wallets_response" | jq -e '.wallets' > /dev/null 2>&1; then
        echo "$wallets_response" | jq -r '.wallets[] | select(.name | test("Regtest")) | .checksum' | while read -r wallet_id; do
            if [ -n "$wallet_id" ]; then
                echo "🗑️  Deleting wallet $wallet_id..."
                local delete_response
                delete_response=$(curl -s -X DELETE "$backend_url/api/wallets/$wallet_id")
                if echo "$delete_response" | jq -e '.message' > /dev/null 2>&1; then
                    echo "✅ Wallet $wallet_id deleted successfully"
                else
                    echo "❌ Failed to delete wallet $wallet_id: $delete_response"
                fi
            fi
        done
        echo "🎉 Regtest wallets removed from backend!"
    else
        echo "❌ Failed to get wallets from backend: $wallets_response"
    fi
}
