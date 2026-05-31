#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TARGET_CONFIRMED_COUNT="${README_SCREENSHOT_CONFIRMED_TX_COUNT:-24}"

btc() {
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 "$@"
}

btc_wallet() {
    local wallet_name="$1"
    shift
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 -rpcwallet="$wallet_name" "$@"
}

fail() {
    echo "ERROR: $*" >&2
    exit 1
}

load_wallet() {
    btc loadwallet "$1" >/dev/null 2>&1 || true
}

confirmed_transaction_count() {
    btc_wallet segwit-desc listtransactions "*" 999999 |
        jq '[.[] | select(.confirmations > 0)] | length'
}

has_pending_transaction() {
    btc_wallet segwit-desc listtransactions "*" 999999 |
        jq -e '[.[] | select(.confirmations == 0)] | length > 0' >/dev/null
}

command -v docker >/dev/null 2>&1 || fail "Missing required tool: docker"
command -v jq >/dev/null 2>&1 || fail "Missing required tool: jq"

if ! btc getblockchaininfo >/dev/null 2>&1; then
    fail "Bitcoin regtest is not running. Start it with: cd scripts && ./dev.sh start"
fi

load_wallet miner
load_wallet segwit-desc

if ! btc_wallet segwit-desc getwalletinfo >/dev/null 2>&1; then
    fail "segwit-desc wallet is missing. Seed regtest wallets first with: cd scripts && CANARY_AUTO_YES=1 ./dev.sh init"
fi

current_confirmed_count="$(confirmed_transaction_count)"
if (( current_confirmed_count < TARGET_CONFIRMED_COUNT )); then
    needed=$((TARGET_CONFIRMED_COUNT - current_confirmed_count))
    amounts=()
    for _ in $(seq 1 "$needed"); do
        amounts+=("0.0001")
    done

    echo "Creating $needed confirmed README screenshot transactions with dev.sh..."
    (
        cd "$SCRIPT_DIR"
        ./dev.sh segwit-desc sent miner "${amounts[@]}"
    )
fi

if ! has_pending_transaction; then
    echo "Creating one pending README screenshot transaction with dev.sh..."
    (
        cd "$SCRIPT_DIR"
        ./dev.sh segwit-desc sending miner 0.0001
    )
fi
