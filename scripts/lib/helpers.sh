btc() {
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 "$@"
}

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

btc_charlie() {
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 -rpcwallet=charlie "$@"
}

btc_bacon() {
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 -rpcwallet=bacon "$@"
}

btc_miner() {
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 -rpcwallet=miner "$@"
}

btc_wallet() {
    local wallet_name=$1
    shift
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 -rpcwallet="$wallet_name" "$@"
}

get_address_type() {
    case "$1" in
        legacy-desc|legacy-empty)   echo "legacy" ;;
        nested-desc|nested-empty)   echo "p2sh-segwit" ;;
        taproot-desc|taproot-empty) echo "bech32m" ;;
        *)                          echo "bech32" ;;
    esac
}

is_wallet_name() {
    case "$1" in
        segwit-desc|legacy-desc|nested-desc|taproot-desc|segwit-empty|legacy-empty|nested-empty|taproot-empty|charlie|bacon|miner|legacy-address|p2sh-address|segwit-address|taproot-address)
            return 0
            ;;
        *)
            return 1
            ;;
    esac
}

is_raw_bitcoin_address() {
    local address="$1"

    if [[ "$address" =~ ^(bc1|tb1|bcrt1)[ac-hj-np-z02-9]{11,87}$ ]]; then
        return 0
    fi

    if [[ "$address" =~ ^[123mn2][1-9A-HJ-NP-Za-km-z]{25,62}$ ]]; then
        return 0
    fi

    return 1
}

load_wallet_if_needed() {
    btc loadwallet "$1" 2>/dev/null || true
}

require_tools() {
    local tool
    for tool in "$@"; do
        if ! command -v "$tool" >/dev/null 2>&1; then
            echo "Missing required tool: $tool" >&2
            exit 1
        fi
    done
}

compare_decimal() {
    local expression="$1"
    require_tools bc
    echo "$expression" | bc -l
}

is_interactive_shell() {
    [ -t 0 ] && [ -t 1 ]
}

prompt_to_continue() {
    local prompt="$1"
    local default_answer="${2:-yes}"
    local reply

    if [ "${CANARY_AUTO_YES:-}" = "1" ] || [ "${CANARY_AUTO_YES:-}" = "true" ]; then
        return 0
    fi

    if ! is_interactive_shell; then
        [ "$default_answer" = "yes" ]
        return
    fi

    read -p "$prompt" -n 1 -r reply
    echo
    if [ -z "$reply" ]; then
        [ "$default_answer" = "yes" ]
        return
    fi

    [[ $reply =~ ^[Yy]$ ]]
}

get_single_address_wallet_address() {
    local wallet_name="$1"
    local addr_list
    addr_list=$(btc_wallet "$wallet_name" listreceivedbyaddress 0 true)
    echo "$addr_list" | jq -r '.[0].address'
}

get_wallet_descriptor() {
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

backend_is_available() {
    local backend_url="${1:-http://localhost:3000}"
    curl -s --connect-timeout 5 --max-time 10 "$backend_url/api/wallets" > /dev/null 2>&1
}

backend_add_wallet() {
    local backend_url="$1"
    local name="$2"
    local descriptor="$3"
    local emoji="${4:-📤}"
    local response payload

    require_tools jq
    payload=$(jq -n --arg name "$name" --arg descriptor "$descriptor" '{name: $name, descriptor: $descriptor}')
    response=$(curl -s -X POST "$backend_url/api/wallets" \
        -H "Content-Type: application/json" \
        -d "$payload")

    if echo "$response" | jq -e '.wallet.checksum' > /dev/null 2>&1; then
        local checksum
        checksum=$(echo "$response" | jq -r '.wallet.checksum')
        echo "   $emoji $name added (checksum: $checksum)"
        return 0
    fi

    local error_msg
    error_msg=$(echo "$response" | jq -r '.error // "unknown error"')
    echo "   $emoji $name: $error_msg"
    return 1
}

mine_blocks() {
    local blocks=${1:-1}
    load_wallet_if_needed "miner"
    local address
    address=$(btc_miner getnewaddress)
    btc generatetoaddress "$blocks" "$address" >/dev/null 2>&1
}

kill_servers() {
    if lsof -ti:3000,3001 > /dev/null 2>&1; then
        local pids
        pids=$(lsof -ti:3000,3001 | sort -u)
        echo "🔪 Stopping backend/frontend (ports 3000, 3001)..."
        if [ -n "$pids" ]; then
            echo "$pids" | xargs kill 2>/dev/null || true
            sleep 1
            echo "$pids" | xargs kill -9 2>/dev/null || true
        fi
        sleep 1
    fi
}

sed_in_place() {
    local expression="$1"
    local file="$2"

    if sed --version >/dev/null 2>&1; then
        sed -i "$expression" "$file"
    else
        sed -i '' "$expression" "$file"
    fi
}
