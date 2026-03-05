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

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIB_DIR="$SCRIPT_DIR/lib"

required_libs=(
    helpers.sh
    bitcoin-tests.sh
    wallet-ops.sh
    backend.sh
    environment.sh
    btcpay.sh
    init.sh
)

for lib in "${required_libs[@]}"; do
    lib_path="$LIB_DIR/$lib"
    if [ ! -f "$lib_path" ]; then
        echo "Missing required library: $lib_path" >&2
        exit 1
    fi
    # shellcheck disable=SC1090
    source "$lib_path"
done

if handle_wallet_command "$@"; then
    exit 0
fi

case "$1" in
    mode)
        cmd_mode "$2"
        ;;
    start)
        cmd_start
        ;;
    init)
        cmd_init
        ;;
    stop)
        cmd_stop
        ;;
    restart)
        cmd_restart
        ;;
    reset)
        cmd_reset
        ;;
    logs)
        cmd_logs "$2"
        ;;
    mine)
        cmd_mine "$2"
        ;;
    reconsider-block)
        cmd_reconsider_block "$2"
        ;;
    get-mempool-txid)
        txid=$(get_mempool_txid "${2:-0}")
        if [ $? -eq 0 ]; then
            echo "$txid"
        fi
        ;;
    mempool-purge)
        mempool_purge "${2:-restart}"
        ;;
    reorg)
        reorg
        ;;
    run-tests)
        run_tests "$2"
        ;;
    mempool-status)
        cmd_mempool_status
        ;;
    status)
        cmd_status
        ;;
    add-wallets-to-backend)
        cmd_add_wallets_to_backend "$2"
        ;;
    remove-wallets-from-backend)
        cmd_remove_wallets_from_backend "$2"
        ;;
    wipe-database)
        cmd_wipe_database
        ;;
    kill)
        cmd_kill
        ;;
    btcpay-setup)
        cmd_btcpay_setup
        ;;
    *)
        cmd_help
        ;;
esac
