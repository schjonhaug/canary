cmd_mode() {
    local mode="$1"
    local data_dir backend_env frontend_env

    require_tools sed
    if [[ "$mode" != "self-hosted" && "$mode" != "cloud" ]]; then
        echo "Usage: $0 mode [self-hosted|cloud]"
        echo "  self-hosted - Single-user mode without authentication"
        echo "  cloud       - Multi-user mode with authentication and Stripe billing"
        exit 1
    fi

    kill_servers
    if [[ "$mode" == "self-hosted" ]]; then
        data_dir="./database/self-hosted"
    else
        data_dir="./database/cloud"
    fi

    backend_env="../backend/.env"
    if [[ -f "$backend_env" ]]; then
        sed_in_place "s/^CANARY_MODE=.*/CANARY_MODE=$mode/" "$backend_env"
        sed_in_place "s|^CANARY_DATA_DIR=.*|CANARY_DATA_DIR=$data_dir|" "$backend_env"
        echo "Updated $backend_env:"
        echo "  CANARY_MODE=$mode"
        echo "  CANARY_DATA_DIR=$data_dir"
    else
        echo "Warning: $backend_env not found"
    fi

    frontend_env="../frontend/.env.local"
    if [[ -f "$frontend_env" ]]; then
        sed_in_place "s/^NEXT_PUBLIC_CANARY_MODE=.*/NEXT_PUBLIC_CANARY_MODE=$mode/" "$frontend_env"
        echo "Updated $frontend_env:"
        echo "  NEXT_PUBLIC_CANARY_MODE=$mode"
    else
        echo "Warning: $frontend_env not found"
    fi

    echo ""
    echo "Mode switched to: $mode"
    echo "Start backend and frontend to apply changes."
}

cmd_start() {
    require_tools docker-compose curl nc
    echo "Starting Bitcoin regtest environment with Docker..."
    docker-compose up -d

    echo "Waiting for Bitcoin Core to start..."
    local timeout=60
    while [ "$timeout" -gt 0 ]; do
        if btc getblockchaininfo > /dev/null 2>&1; then
            echo "✅ Bitcoin Core is ready"
            break
        fi
        sleep 1
        timeout=$((timeout-1))
    done

    if [ "$timeout" -eq 0 ]; then
        echo "❌ Bitcoin Core failed to start within 60 seconds"
        echo "Checking logs..."
        docker logs bitcoind-regtest | tail -10
        exit 1
    fi

    echo "Waiting for Fulcrum Electrum server to start..."
    timeout=120
    while [ "$timeout" -gt 0 ]; do
        if nc -z localhost 50001 2>/dev/null; then
            echo "✅ Fulcrum Electrum server is ready"
            break
        fi
        sleep 2
        timeout=$((timeout-2))
    done

    if [ "$timeout" -le 0 ]; then
        echo "⚠️  Fulcrum may still be starting (can take a few minutes on first run)"
        echo "Check logs with: ./docker-utils.sh logs fulcrum"
    fi

    echo "Waiting for ntfy server to start..."
    timeout=30
    local ntfy_token=""
    while [ "$timeout" -gt 0 ]; do
        if curl -s http://localhost:2586/v1/health > /dev/null 2>&1; then
            echo "✅ ntfy server is ready (auth: deny-all)"
            break
        fi
        sleep 1
        timeout=$((timeout-1))
    done

    if [ "$timeout" -le 0 ]; then
        echo "⚠️  ntfy server may still be starting"
    else
        echo "Setting up ntfy test credentials..."
        printf "testpassword\ntestpassword\n" | docker exec -i ntfy-regtest ntfy user add --role=admin testuser 2>/dev/null || true
        ntfy_token=$(docker exec ntfy-regtest ntfy token list testuser 2>/dev/null | grep -o 'tk_[a-zA-Z0-9_]*' | head -1)
        if [ -z "$ntfy_token" ]; then
            ntfy_token=$(docker exec ntfy-regtest ntfy token add -l "Dev token" testuser 2>&1 | grep -o 'tk_[a-zA-Z0-9_]*' | head -1)
        fi
    fi

    echo ""
    echo "🚀 Bitcoin regtest environment is running!"
    echo "Bitcoin RPC: localhost:18443"
    echo "Fulcrum Electrum server: localhost:50001"
    echo "ntfy server: http://localhost:2586"
    echo "  User: testuser / testpassword"
    if [ -n "$ntfy_token" ]; then
        echo "  Token: $ntfy_token"
    fi
    echo "Set BITCOIN_NETWORK=regtest in your environment"
    echo ""
    echo "💡 Next: $0 init (creates wallets and adds to backend)"
}

cmd_stop() {
    echo "Stopping Bitcoin regtest environment..."
    docker-compose down
    echo "✅ Environment stopped"
}

cmd_restart() {
    echo "Restarting Bitcoin regtest environment..."
    docker-compose restart
}

cmd_reset() {
    echo "⚠️  This will stop containers and delete all blockchain data AND database data!"
    read -p "Are you sure? (y/N): " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo "Reset cancelled"
        return
    fi

    echo "Resetting environment..."
    kill_servers
    if btc getblockchaininfo > /dev/null 2>&1; then
        echo "Unloading test wallets..."
        local wallet
        for wallet in segwit-desc legacy-desc nested-desc taproot-desc segwit-empty legacy-empty nested-empty taproot-empty charlie bacon satoshi-genesis miner; do
            btc unloadwallet "$wallet" 2>/dev/null || true
        done
    fi
    docker-compose down -v
    echo "Cleaning up regtest databases..."
    local found_db=false
    if [ -d "../backend/database/cloud/regtest" ]; then
        rm -rf ../backend/database/cloud/regtest
        echo "✅ Cloud regtest database folder removed"
        found_db=true
    fi
    if [ -d "../backend/database/self-hosted/regtest" ]; then
        rm -rf ../backend/database/self-hosted/regtest
        echo "✅ Self-hosted regtest database folder removed"
        found_db=true
    fi
    if [ "$found_db" = false ]; then
        echo "⚠️  No regtest database folders found (this is normal for first run)"
    fi
    echo "✅ Environment reset complete (all wallets, blockchain data, and database wiped)"
}

cmd_logs() {
    local service="${1:-}"
    if [ -n "$service" ]; then
        docker-compose logs -f "$service"
    else
        docker-compose logs -f
    fi
}

cmd_mine() {
    local blocks="${1:-1}"
    echo "Mining $blocks block(s) to Miner wallet..."
    load_wallet_if_needed "miner"
    local address
    address=$(btc_miner getnewaddress)
    btc generatetoaddress "$blocks" "$address"
    echo "✅ Mined $blocks block(s) to Miner wallet"
}

cmd_reconsider_block() {
    local block_hash="$1"
    if [ -z "$block_hash" ]; then
        echo "Usage: $0 reconsider-block <block_hash>"
        echo "Example: $0 reconsider-block 1a2b3c4d5e6f..."
        exit 1
    fi
    echo "🔄 Reconsidering block $block_hash..."
    local old_tip_height new_tip_hash new_tip_height
    old_tip_height=$(btc getblockcount)
    btc reconsiderblock "$block_hash"
    new_tip_hash=$(btc getbestblockhash)
    new_tip_height=$(btc getblockcount)
    echo "   ✅ Block reconsidered!"
    echo "   New tip: $new_tip_hash (height: $new_tip_height)"
    echo ""
    if [ "$new_tip_height" -gt "$old_tip_height" ]; then
        echo "📊 Effect:"
        echo "   - Block was reactivated and became the new tip"
        echo "   - Blockchain height increased from $old_tip_height to $new_tip_height"
        echo "   - Transactions in the block are now confirmed again"
    else
        echo "📊 Effect:"
        echo "   - Block was reconsidered but did not become the new tip"
        echo "   - Another chain may have more work"
    fi
}

cmd_mempool_status() {
    echo "=== Mempool Status ==="
    if btc getblockchaininfo > /dev/null 2>&1; then
        local mempool_size mempool_bytes
        mempool_size=$(btc getmempoolinfo | grep '"size"' | cut -d':' -f2 | tr -d ' ,')
        mempool_bytes=$(btc getmempoolinfo | grep '"bytes"' | cut -d':' -f2 | tr -d ' ,')
        echo "Mempool transactions: $mempool_size"
        echo "Mempool size: $mempool_bytes bytes"
        if [ "$mempool_size" -gt 0 ]; then
            echo "Pending transactions:"
            btc getrawmempool | grep -E '"[a-f0-9]{64}"' | head -5
            if [ "$mempool_size" -gt 5 ]; then
                echo "... and $((mempool_size - 5)) more"
            fi
        else
            echo "No pending transactions"
        fi
    else
        echo "Bitcoin Core: ❌ Not running"
    fi
}

cmd_status() {
    echo "=== Bitcoin regtest Status ==="
    if btc getblockchaininfo > /dev/null 2>&1; then
        echo "Bitcoin Core: ✅ Running"
        echo "Block count: $(btc getblockcount)"
        local wallet
        for wallet in segwit-desc legacy-desc nested-desc taproot-desc segwit-empty charlie miner; do
            load_wallet_if_needed "$wallet"
        done
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
        echo "Mempool transactions: $(btc getmempoolinfo | grep '"size"' | cut -d':' -f2 | tr -d ' ,')"
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
        local ntfy_token
        ntfy_token=$(docker exec ntfy-regtest ntfy token list testuser 2>/dev/null | grep -o 'tk_[a-zA-Z0-9_]*' | head -1)
        if [ -n "$ntfy_token" ]; then
            echo "  Token: $ntfy_token"
        fi
    else
        echo "ntfy server: ❌ Not running"
    fi
    echo ""
    echo "=== Docker Containers ==="
    docker-compose ps
}

cmd_wipe_database() {
    echo "🗑️  Wiping all SQLite databases..."
    local found_db=false
    if [ -d "../backend/database/cloud" ]; then
        rm -rf ../backend/database/cloud
        echo "✅ Cloud database folder removed"
        found_db=true
    fi
    if [ -d "../backend/database/self-hosted" ]; then
        rm -rf ../backend/database/self-hosted
        echo "✅ Self-hosted database folder removed"
        found_db=true
    fi
    if [ "$found_db" = true ]; then
        echo "💡 Databases will be recreated when the backend starts"
    else
        echo "⚠️  No database folders found"
    fi
}

cmd_kill() {
    kill_servers
    echo "🎯 Port cleanup complete"
}

cmd_help() {
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
}
