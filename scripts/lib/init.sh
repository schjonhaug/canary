create_descriptor_wallet() {
    local wallet_name="$1"
    local tprv="$2"
    local wrapper="$3"
    local bip_path="$4"

    echo "📋 Creating $wallet_name wallet..."
    btc unloadwallet "$wallet_name" 2>/dev/null || true

    set +e
    CREATE_RESULT=$(btc -named createwallet wallet_name="$wallet_name" disable_private_keys=false blank=true passphrase="" avoid_reuse=false descriptors=true 2>&1)
    CREATE_EXIT_CODE=$?
    set -e

    if echo "$CREATE_RESULT" | grep -q "already exists"; then
        echo "   ✅ $wallet_name wallet exists, loading..."
        load_wallet_if_needed "$wallet_name"
    elif [ "$CREATE_EXIT_CODE" -eq 0 ]; then
        echo "   ✅ $wallet_name blank wallet created"
        local ext_raw int_raw ext_checksum int_checksum
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

cmd_init() {
    if ! btc getblockchaininfo > /dev/null 2>&1; then
        echo "🔧 Bitcoin Core not running — starting infrastructure first..."
        "$0" start
        echo ""
    fi

    echo "🏦 Setting up development wallets..."
    if ! btc getblockchaininfo > /dev/null 2>&1; then
        echo "❌ Bitcoin Core is not running."
        exit 1
    fi

    local funded_tprv empty_tprv segwit_desc_descriptor legacy_desc_descriptor nested_desc_descriptor taproot_desc_descriptor segwit_empty_descriptor legacy_empty_descriptor nested_empty_descriptor taproot_empty_descriptor charlie_descriptors charlie_receive_desc charlie_multipath_raw charlie_checksum_info charlie_checksum charlie_descriptor bacon_descriptors bacon_receive_desc bacon_multipath_raw bacon_checksum_info bacon_checksum bacon_descriptor miner_descriptors miner_receive_desc miner_multipath_raw miner_checksum_info miner_checksum miner_address block_count charlie_balance bacon_balance satoshi_genesis_tprv satoshi_genesis_address satoshi_genesis_balance backend_url reply
    funded_tprv="tprv8ZgxMBicQKsPe6D3wxZPHxy7JEMBwjxiimYXSvM8aRaqZmDvU7Jec5c8aSB8rCzFmAP6aEjPhUmiXm2KB7XUzkApX2prmDHcQsUQY5DsxJw"
    empty_tprv="tprv8ZgxMBicQKsPd3nsWkJrKQgXk22UnGFHFPDWNCeTjvzeY9LjRYdi3RNX1NfkCwj4mD1YTsNPCCtGPTTQkxn14oLKNg6vuVkChn55qa5rC7K"

    create_descriptor_wallet "segwit-desc" "$funded_tprv" "wpkh" "84h/1h/0h"
    create_descriptor_wallet "legacy-desc" "$funded_tprv" "pkh" "44h/1h/0h"
    create_descriptor_wallet "nested-desc" "$funded_tprv" "sh_wpkh" "49h/1h/0h"
    create_descriptor_wallet "taproot-desc" "$funded_tprv" "tr" "86h/1h/0h"
    segwit_desc_descriptor=$(get_wallet_descriptor "segwit-desc" '.desc | startswith("wpkh(") and contains("/0/*")')
    legacy_desc_descriptor=$(get_wallet_descriptor "legacy-desc" '.desc | startswith("pkh(") and contains("/0/*")')
    nested_desc_descriptor=$(get_wallet_descriptor "nested-desc" '.desc | startswith("sh(wpkh(") and contains("/0/*")')
    taproot_desc_descriptor=$(get_wallet_descriptor "taproot-desc" '.desc | startswith("tr(") and contains("/0/*")')

    create_descriptor_wallet "segwit-empty" "$empty_tprv" "wpkh" "84h/1h/0h"
    create_descriptor_wallet "legacy-empty" "$empty_tprv" "pkh" "44h/1h/0h"
    create_descriptor_wallet "nested-empty" "$empty_tprv" "sh_wpkh" "49h/1h/0h"
    create_descriptor_wallet "taproot-empty" "$empty_tprv" "tr" "86h/1h/0h"
    segwit_empty_descriptor=$(get_wallet_descriptor "segwit-empty" '.desc | startswith("wpkh(") and contains("/0/*")')
    legacy_empty_descriptor=$(get_wallet_descriptor "legacy-empty" '.desc | startswith("pkh(") and contains("/0/*")')
    nested_empty_descriptor=$(get_wallet_descriptor "nested-empty" '.desc | startswith("sh(wpkh(") and contains("/0/*")')
    taproot_empty_descriptor=$(get_wallet_descriptor "taproot-empty" '.desc | startswith("tr(") and contains("/0/*")')

    echo "📋 Creating Charlie wallet..."
    btc unloadwallet "charlie" 2>/dev/null || true
    set +e
    CREATE_RESULT=$(btc -named createwallet wallet_name="charlie" disable_private_keys=false blank=true passphrase="" avoid_reuse=false descriptors=true 2>&1)
    CREATE_EXIT_CODE=$?
    set -e
    if echo "$CREATE_RESULT" | grep -q "already exists"; then
        echo "   ✅ Charlie wallet exists, loading..."
        load_wallet_if_needed "charlie"
    elif [ "$CREATE_EXIT_CODE" -eq 0 ]; then
        echo "   ✅ Charlie blank wallet created"
        btc_charlie importdescriptors '[{"desc": "wpkh(tprv8ZgxMBicQKsPd7Uf69XL1XwhmjHopUGep8GuEiJDZmbQz6o58LninorQAfcKZWARbtRtfnLcJ5MQ2AtHcQJCCRUcMRvmDUjyEmNUWwx8UbK/84h/1h/0h/0/*)#pe5sgqha", "timestamp": "now", "active": true, "internal": false, "range": [0, 999]}, {"desc": "wpkh(tprv8ZgxMBicQKsPd7Uf69XL1XwhmjHopUGep8GuEiJDZmbQz6o58LninorQAfcKZWARbtRtfnLcJ5MQ2AtHcQJCCRUcMRvmDUjyEmNUWwx8UbK/84h/1h/0h/1/*)#sd334489", "timestamp": "now", "active": true, "internal": true, "range": [0, 999]}]' >/dev/null 2>&1
        echo "   ✅ Charlie wallet seeded with deterministic descriptors"
    else
        echo "   ❌ Failed to create Charlie wallet: $CREATE_RESULT"
        exit 1
    fi
    charlie_descriptors=$(btc_wallet charlie listdescriptors)
    charlie_receive_desc=$(echo "$charlie_descriptors" | jq -r '.descriptors[] | select(.desc | startswith("wpkh") and contains("/0/*")) | .desc')
    charlie_multipath_raw=$(echo "$charlie_receive_desc" | sed 's|/0/\*|/<0;1>/\*|' | sed 's/#[^#]*$//')
    charlie_checksum_info=$(btc getdescriptorinfo "$charlie_multipath_raw")
    charlie_checksum=$(echo "$charlie_checksum_info" | jq -r '.checksum')
    charlie_descriptor="$charlie_multipath_raw#$charlie_checksum"

    echo "📋 Creating Bacon wallet..."
    btc unloadwallet "bacon" 2>/dev/null || true
    set +e
    CREATE_RESULT=$(btc -named createwallet wallet_name="bacon" disable_private_keys=false blank=true passphrase="" avoid_reuse=false descriptors=true 2>&1)
    CREATE_EXIT_CODE=$?
    set -e
    if echo "$CREATE_RESULT" | grep -q "already exists"; then
        echo "   ✅ Bacon wallet exists, loading..."
        load_wallet_if_needed "bacon"
    elif [ "$CREATE_EXIT_CODE" -eq 0 ]; then
        echo "   ✅ Bacon blank wallet created"
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
    bacon_descriptors=$(btc_wallet bacon listdescriptors)
    bacon_receive_desc=$(echo "$bacon_descriptors" | jq -r '.descriptors[] | select(.desc | startswith("wpkh") and contains("/0/*")) | .desc')
    bacon_multipath_raw=$(echo "$bacon_receive_desc" | sed 's|/0/\*|/<0;1>/\*|' | sed 's/#[^#]*$//')
    bacon_checksum_info=$(btc getdescriptorinfo "$bacon_multipath_raw")
    bacon_checksum=$(echo "$bacon_checksum_info" | jq -r '.checksum')
    bacon_descriptor="$bacon_multipath_raw#$bacon_checksum"

    echo "📋 Creating Miner wallet..."
    btc unloadwallet "miner" 2>/dev/null || true
    set +e
    CREATE_RESULT=$(btc -named createwallet wallet_name="miner" disable_private_keys=false blank=false passphrase="" avoid_reuse=false descriptors=true 2>&1)
    CREATE_EXIT_CODE=$?
    set -e
    if echo "$CREATE_RESULT" | grep -q "already exists"; then
        echo "   ✅ Miner wallet exists, loading..."
        load_wallet_if_needed "miner"
    elif [ "$CREATE_EXIT_CODE" -eq 0 ]; then
        echo "   ✅ Miner wallet created"
    else
        echo "   ❌ Failed to create Miner wallet: $CREATE_RESULT"
        exit 1
    fi
    miner_descriptors=$(btc_wallet miner listdescriptors)
    miner_receive_desc=$(echo "$miner_descriptors" | jq -r '.descriptors[] | select(.desc | startswith("wpkh") and contains("/0/*")) | .desc')
    miner_multipath_raw=$(echo "$miner_receive_desc" | sed 's|/0/\*|/<0;1>/\*|' | sed 's/#[^#]*$//')
    miner_checksum_info=$(btc getdescriptorinfo "$miner_multipath_raw")
    miner_checksum=$(echo "$miner_checksum_info" | jq -r '.checksum')
    miner_address=$(btc_wallet miner getnewaddress)

    echo "💰 Funding descriptor wallets..."
    block_count=$(btc getblockcount 2>/dev/null || echo "0")
    if [ "$block_count" -lt 104 ]; then
        local recipients segwit_addr_5 segwit_addr_05 segwit_addr_01 funded_wallet funded_addr i
        echo "   ⛏️  Mining blocks and transferring funds..."
        btc generatetoaddress 103 "$miner_address" >/dev/null 2>&1
        echo "   📍 Generating addresses for segwit-desc distributed funding..."
        recipients="{"
        segwit_addr_5=$(btc_wallet "segwit-desc" getnewaddress)
        recipients="${recipients}\"$segwit_addr_5\":0.5"
        for i in {1..5}; do
            segwit_addr_05=$(btc_wallet "segwit-desc" getnewaddress)
            recipients="${recipients},\"$segwit_addr_05\":0.05"
        done
        for i in {1..25}; do
            segwit_addr_01=$(btc_wallet "segwit-desc" getnewaddress)
            recipients="${recipients},\"$segwit_addr_01\":0.01"
        done
        recipients="${recipients}}"
        echo "   💸 Creating single transaction with multiple outputs..."
        echo "   📊 Distribution: 1×0.5 BTC + 5×0.05 BTC + 25×0.01 BTC = 1 BTC across 31 addresses"
        btc_miner sendmany "" "$recipients" >/dev/null 2>&1
        btc generatetoaddress 1 "$miner_address" >/dev/null 2>&1
        echo "   ✅ segwit-desc funded with 1 BTC (distributed across 31 addresses)"
        for funded_wallet in legacy-desc nested-desc taproot-desc; do
            funded_addr=$(btc_wallet "$funded_wallet" getnewaddress "" "$(get_address_type "$funded_wallet")")
            btc_miner sendtoaddress "$funded_addr" 0.123 >/dev/null 2>&1
            echo "   ✅ $funded_wallet funded with 0.123 BTC"
        done
        btc generatetoaddress 1 "$miner_address" >/dev/null 2>&1
    else
        echo "   ✅ Descriptor wallets already funded"
    fi

    echo "💰 Funding Charlie wallet at index 250..."
    charlie_balance=$(btc_wallet charlie getbalance 2>/dev/null || echo "0")
    if [ "$(echo "$charlie_balance == 0" | bc -l 2>/dev/null || echo "1")" -eq 1 ]; then
        local addr charlie_addr_250 i charlie_txid
        echo "   📍 Generating addresses up to index 250..."
        for i in {0..250}; do
            addr=$(btc_charlie getnewaddress 2>/dev/null)
            if [ "$i" -eq 250 ]; then
                charlie_addr_250="$addr"
                echo "   🎯 Address at index 250: $charlie_addr_250"
            fi
            if [ $((i % 50)) -eq 0 ] && [ "$i" -gt 0 ]; then
                echo "   📍 Generated addresses 0-$i..."
            fi
        done
        echo "   💸 Sending 0.5 BTC to Charlie at index 250..."
        charlie_txid=$(btc_miner sendtoaddress "$charlie_addr_250" 0.5)
        btc generatetoaddress 1 "$miner_address" >/dev/null 2>&1
        echo "   ✅ Charlie funded with 0.5 BTC at index 250"
        echo "   📋 Transaction: $charlie_txid"
    else
        echo "   ✅ Charlie already funded"
    fi

    echo "💰 Funding Bacon wallet (for demo account)..."
    bacon_balance=$(btc_wallet bacon getbalance 2>/dev/null || echo "0")
    if [ "$(echo "$bacon_balance == 0" | bc -l 2>/dev/null || echo "1")" -eq 1 ]; then
        local bacon_addr bacon_addr2 segwit_addr segwit_addr2
        echo "   💸 Sending 0.1 BTC to Bacon wallet..."
        bacon_addr=$(btc_wallet bacon getnewaddress)
        btc_miner sendtoaddress "$bacon_addr" 0.1 >/dev/null
        btc generatetoaddress 1 "$miner_address" >/dev/null 2>&1
        echo "   ✅ Bacon funded with 0.1 BTC"
        echo "   📜 Creating transaction history..."
        segwit_addr=$(btc_wallet "segwit-desc" getnewaddress)
        btc_wallet bacon sendtoaddress "$segwit_addr" 0.02 >/dev/null
        btc generatetoaddress 1 "$miner_address" >/dev/null 2>&1
        echo "   ✅ Bacon → segwit-desc: 0.02 BTC"
        bacon_addr2=$(btc_wallet bacon getnewaddress)
        btc_wallet "segwit-desc" sendtoaddress "$bacon_addr2" 0.015 >/dev/null
        btc generatetoaddress 1 "$miner_address" >/dev/null 2>&1
        echo "   ✅ segwit-desc → Bacon: 0.015 BTC"
        segwit_addr2=$(btc_wallet "segwit-desc" getnewaddress)
        btc_wallet bacon sendtoaddress "$segwit_addr2" 0.01 >/dev/null
        btc generatetoaddress 1 "$miner_address" >/dev/null 2>&1
        echo "   ✅ Bacon → segwit-desc: 0.01 BTC"
        echo "   ✅ Transaction history created (4 transactions)"
    else
        echo "   ✅ Bacon already funded"
    fi

    satoshi_genesis_tprv="tprv8ZgxMBicQKsPeZjnkSokuUQsdrWJ83bXz4Eqm1aVDkDSSJ9BqHGMsjxpBEb3n6V9X3u6ThQQ1dmsvigtXWxvP8YJL9FST4DighMqnHtmFTo"
    satoshi_genesis_address="bcrt1q20lu6ldqtssq7y7ewarlamlzldnmyk5w4n3e97"
    echo "📋 Creating Satoshi (Genesis) wallet..."
    btc unloadwallet "satoshi-genesis" 2>/dev/null || true
    set +e
    CREATE_RESULT=$(btc -named createwallet wallet_name="satoshi-genesis" disable_private_keys=false blank=true passphrase="" avoid_reuse=false descriptors=true 2>&1)
    CREATE_EXIT_CODE=$?
    set -e
    if echo "$CREATE_RESULT" | grep -q "already exists"; then
        echo "   ✅ Satoshi (Genesis) wallet exists, loading..."
        load_wallet_if_needed "satoshi-genesis"
    elif [ "$CREATE_EXIT_CODE" -eq 0 ]; then
        echo "   ✅ Satoshi (Genesis) blank wallet created"
        local satoshi_ext_raw satoshi_int_raw satoshi_ext_checksum satoshi_int_checksum
        satoshi_ext_raw="wpkh($satoshi_genesis_tprv/84h/1h/0h/0/*)"
        satoshi_int_raw="wpkh($satoshi_genesis_tprv/84h/1h/0h/1/*)"
        satoshi_ext_checksum=$(btc getdescriptorinfo "$satoshi_ext_raw" | jq -r '.checksum')
        satoshi_int_checksum=$(btc getdescriptorinfo "$satoshi_int_raw" | jq -r '.checksum')
        btc_wallet "satoshi-genesis" importdescriptors "[
          {\"desc\": \"${satoshi_ext_raw}#${satoshi_ext_checksum}\", \"timestamp\": \"now\", \"active\": true, \"internal\": false, \"range\": [0, 999]},
          {\"desc\": \"${satoshi_int_raw}#${satoshi_int_checksum}\", \"timestamp\": \"now\", \"active\": true, \"internal\": true, \"range\": [0, 999]}
        ]" >/dev/null 2>&1
        echo "   ✅ Satoshi (Genesis) wallet seeded with deterministic descriptors"
    else
        echo "   ❌ Failed to create Satoshi (Genesis) wallet: $CREATE_RESULT"
        exit 1
    fi

    echo "💰 Funding Satoshi (Genesis) wallet..."
    satoshi_genesis_balance=$(btc_wallet "satoshi-genesis" getbalance 2>/dev/null || echo "0")
    if [ "$(echo "$satoshi_genesis_balance == 0" | bc -l 2>/dev/null || echo "1")" -eq 1 ]; then
        echo "   💸 Sending 0.5 BTC to Satoshi (Genesis) address..."
        btc_miner sendtoaddress "$satoshi_genesis_address" 0.5 >/dev/null 2>&1
        btc generatetoaddress 1 "$miner_address" >/dev/null 2>&1
        echo "   ✅ Satoshi (Genesis) funded with 0.5 BTC at $satoshi_genesis_address"
    else
        echo "   ✅ Satoshi (Genesis) already funded"
    fi

    echo "📋 Creating single-address wallets..."
    local addr_types=("legacy" "p2sh-segwit" "bech32" "bech32m")
    local addr_labels=("P2PKH legacy" "P2SH nested segwit" "P2WPKH native segwit" "P2TR taproot")
    local addr_wallet_names=("legacy-address" "p2sh-address" "segwit-address" "taproot-address")
    local i addr_type addr_label addr_wallet_name addr_balance address addr_list
    for i in "${!addr_types[@]}"; do
        addr_type="${addr_types[$i]}"
        addr_label="${addr_labels[$i]}"
        addr_wallet_name="${addr_wallet_names[$i]}"
        set +e
        CREATE_RESULT=$(btc -named createwallet wallet_name="$addr_wallet_name" descriptors=true 2>&1)
        CREATE_EXIT_CODE=$?
        set -e
        if echo "$CREATE_RESULT" | grep -q "already exists"; then
            load_wallet_if_needed "$addr_wallet_name"
        elif [ "$CREATE_EXIT_CODE" -ne 0 ]; then
            echo "   ❌ Failed to create $addr_wallet_name: $CREATE_RESULT"
            continue
        fi
        addr_balance=$(btc_wallet "$addr_wallet_name" getbalance 2>/dev/null || echo "0")
        if [ "$(echo "$addr_balance == 0" | bc -l 2>/dev/null || echo "1")" -eq 1 ]; then
            address=$(btc_wallet "$addr_wallet_name" getnewaddress "" "$addr_type")
            btc_miner sendtoaddress "$address" 0.123 > /dev/null
            echo "   ✅ $addr_wallet_name ($addr_label): $address — funded 0.123 BTC"
        else
            echo "   ✅ $addr_wallet_name already funded ($addr_balance BTC)"
        fi
    done
    btc generatetoaddress 1 "$miner_address" >/dev/null 2>&1

    echo ""
    echo "🎉 All wallets setup complete!"
    echo ""
    echo "📱 Funded descriptor wallets:"
    echo "   segwit-desc  (wpkh, 1 BTC distributed):  $segwit_desc_descriptor"
    echo "   legacy-desc  (pkh, 0.123 BTC):           $legacy_desc_descriptor"
    echo "   nested-desc  (sh(wpkh), 0.123 BTC):      $nested_desc_descriptor"
    echo "   taproot-desc (tr, 0.123 BTC):             $taproot_desc_descriptor"
    echo ""
    echo "📱 Empty descriptor wallets:"
    echo "   segwit-empty  (wpkh):     $segwit_empty_descriptor"
    echo "   legacy-empty  (pkh):      $legacy_empty_descriptor"
    echo "   nested-empty  (sh(wpkh)): $nested_empty_descriptor"
    echo "   taproot-empty (tr):       $taproot_empty_descriptor"
    echo ""
    echo "📱 Other wallets:"
    echo "   🎭 Charlie (funded - 0.5 BTC at index 250):  $charlie_descriptor"
    echo "   🥓 Bacon (demo - ~0.08 BTC):                 $bacon_descriptor"
    echo "   🪙 Satoshi Genesis (sample - 0.5 BTC):       $satoshi_genesis_address"
    echo ""
    echo "📍 Single addresses (for address monitoring):"
    for i in "${!addr_wallet_names[@]}"; do
        addr_wallet_name="${addr_wallet_names[$i]}"
        addr_label="${addr_labels[$i]}"
        load_wallet_if_needed "$addr_wallet_name"
        addr_list=$(btc_wallet "$addr_wallet_name" listreceivedbyaddress 0 true)
        address=$(echo "$addr_list" | jq -r '.[0].address')
        addr_balance=$(btc_wallet "$addr_wallet_name" getbalance)
        echo "   🔍 $addr_wallet_name ($addr_label): $address ($addr_balance BTC)"
    done
    echo ""
    echo ""
    "$0" btcpay-setup

    backend_url="http://localhost:3000"
    echo ""
    if curl -s --connect-timeout 2 --max-time 5 "$backend_url/api/wallets" > /dev/null 2>&1; then
        read -p "Add wallets to backend? (self-hosted mode only) (Y/n): " -n 1 -r reply
        echo
    else
        echo "⚠️  Backend not running at $backend_url — it must be running to add wallets."
        echo "   Start it with:  cd ../backend && cargo run"
        echo "   Note: This only works in self-hosted mode (unauthenticated API)."
        echo ""
        read -p "Press Enter when the backend is running, or type 'n' to skip: " -n 1 -r reply
        echo
    fi
    if [[ $reply =~ ^[Nn]$ ]]; then
        echo "💡 You can add wallets later with: $0 add-wallets-to-backend"
        exit 0
    fi

    echo "🔍 Checking backend at $backend_url..."
    if backend_is_available "$backend_url"; then
        echo "✅ Backend is running — adding wallets..."
        echo ""
        backend_add_wallet "$backend_url" "segwit-desc" "$segwit_desc_descriptor" "📱"
        backend_add_wallet "$backend_url" "legacy-desc" "$legacy_desc_descriptor" "📱"
        backend_add_wallet "$backend_url" "nested-desc" "$nested_desc_descriptor" "📱"
        backend_add_wallet "$backend_url" "taproot-desc" "$taproot_desc_descriptor" "📱"
        backend_add_wallet "$backend_url" "segwit-empty" "$segwit_empty_descriptor" "📱"
        backend_add_wallet "$backend_url" "legacy-empty" "$legacy_empty_descriptor" "📱"
        backend_add_wallet "$backend_url" "nested-empty" "$nested_empty_descriptor" "📱"
        backend_add_wallet "$backend_url" "taproot-empty" "$taproot_empty_descriptor" "📱"
        backend_add_wallet "$backend_url" "Charlie" "$charlie_descriptor" "🎭"
        for i in "${!addr_wallet_names[@]}"; do
            addr_wallet_name="${addr_wallet_names[$i]}"
            load_wallet_if_needed "$addr_wallet_name"
            addr_list=$(btc_wallet "$addr_wallet_name" listreceivedbyaddress 0 true)
            address=$(echo "$addr_list" | jq -r '.[0].address')
            backend_add_wallet "$backend_url" "$addr_wallet_name" "$address" "🔍"
        done
        echo ""
        echo "🎉 Init complete! Check http://localhost:3001"
    else
        echo "⚠️  Backend not running — wallets not added to database"
        echo "💡 Start the backend and run: $0 add-wallets-to-backend"
    fi
}
