#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
PLAYWRIGHT_DIR="$SCRIPT_DIR/playwright"
BACKEND_URL="http://127.0.0.1:3000"
FRONTEND_PORT="${CANARY_UPGRADE_FRONTEND_PORT:-3001}"
FRONTEND_URL="${CANARY_UPGRADE_FRONTEND_URL:-http://localhost:${FRONTEND_PORT}}"
NTFY_URL="http://127.0.0.1:2586"
SELF_HOSTED_ADMIN_EMAIL="${CANARY_SELF_HOSTED_ADMIN_EMAIL:-admin@local}"
SELF_HOSTED_ADMIN_PASSWORD="${CANARY_SELF_HOSTED_ADMIN_PASSWORD:-replace-with-a-strong-password}"
NTFY_USERNAME="${CANARY_NTFY_USERNAME:-testuser}"
NTFY_PASSWORD="${CANARY_NTFY_PASSWORD:-testpassword}"

FROM_TAG=""
KEEP_WORKTREE=0
SKIP_PLAYWRIGHT_INSTALL=0

WORK_DIR=""
WORKTREE_DIR=""
BACKEND_PID=""
FRONTEND_PID=""

TARGET_WALLET_CHECKSUM=""
TARGET_WALLET_NAME=""
TARGET_BTC_WALLET=""
NTFY_TOPIC=""
PRE_UPGRADE_TXID=""
POST_UPGRADE_TXID=""
EXPECTED_WALLET_COUNT=""
AUTH_TOKEN=""
PRE_UPGRADE_BDK_WALLET_ROW=""
PRE_UPGRADE_REVEALED_INDEXES=""

usage() {
    cat <<'EOF'
Usage: ./scripts/test-upgrade.sh [--from-tag <tag>] [--keep-worktree] [--skip-playwright-install]

Options:
  --from-tag <tag>           Upgrade from the given BDK-2 tag. Defaults to the latest such release.
  --keep-worktree            Keep the temporary worktree and logs after the run.
  --skip-playwright-install  Reuse an existing Playwright install.
  -h, --help                 Show this help message.
EOF
}

log() {
    printf '\n==> %s\n' "$1"
}

fail() {
    echo "ERROR: $*" >&2
    exit 1
}

require_tools() {
    local tool
    for tool in "$@"; do
        command -v "$tool" >/dev/null 2>&1 || fail "Missing required tool: $tool"
    done
}

ref_uses_bdk_wallet_2() {
    local ref="$1"

    git -C "$REPO_ROOT" show "${ref}:backend/Cargo.lock" 2>/dev/null | awk '
        $0 == "name = \"bdk_wallet\"" {
            getline
            if ($0 ~ /^version = \"2\./) found = 1
        }
        END { exit(found ? 0 : 1) }
    '
}

latest_bdk2_release_tag() {
    local tag

    while IFS= read -r tag; do
        if ref_uses_bdk_wallet_2 "$tag"; then
            echo "$tag"
            return 0
        fi
    done < <(git -C "$REPO_ROOT" tag --sort=-version:refname)

    return 1
}

docker_compose() {
    if docker compose version >/dev/null 2>&1; then
        docker compose "$@"
        return
    fi

    if command -v docker-compose >/dev/null 2>&1; then
        docker-compose "$@"
        return
    fi

    fail "Missing Docker Compose. Install the docker compose plugin or docker-compose binary."
}

kill_if_running() {
    local pid="$1"
    if [[ -n "$pid" ]] && kill -0 "$pid" >/dev/null 2>&1; then
        local child
        for child in $(pgrep -P "$pid" 2>/dev/null || true); do
            kill_if_running "$child"
        done
        kill "$pid" >/dev/null 2>&1 || true
        sleep 1
        kill -9 "$pid" >/dev/null 2>&1 || true
    fi
}

cleanup() {
    set +e
    kill_if_running "$BACKEND_PID"
    kill_if_running "$FRONTEND_PID"

    if [[ "$KEEP_WORKTREE" -eq 0 && -n "$WORKTREE_DIR" && -d "$WORKTREE_DIR" ]]; then
        git -C "$REPO_ROOT" worktree remove --force "$WORKTREE_DIR" >/dev/null 2>&1 || true
    fi

    if [[ "$KEEP_WORKTREE" -eq 0 && -n "$WORK_DIR" && -d "$WORK_DIR" ]]; then
        rm -rf "$WORK_DIR"
    fi
}

trap cleanup EXIT

while [[ $# -gt 0 ]]; do
    case "$1" in
        --from-tag)
            [[ $# -ge 2 ]] || fail "--from-tag requires a value"
            FROM_TAG="$2"
            shift 2
            ;;
        --keep-worktree)
            KEEP_WORKTREE=1
            shift
            ;;
        --skip-playwright-install)
            SKIP_PLAYWRIGHT_INSTALL=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            fail "Unknown argument: $1"
            ;;
    esac
done

require_tools git jq curl docker pnpm cargo npm npx mktemp lsof pgrep sqlite3

if [[ -z "$FROM_TAG" ]]; then
    FROM_TAG="$(latest_bdk2_release_tag)" \
        || fail "Could not find a release tag using bdk_wallet 2.x"
fi

git -C "$REPO_ROOT" rev-parse --verify "${FROM_TAG}^{tag}" >/dev/null 2>&1 \
    || git -C "$REPO_ROOT" rev-parse --verify "$FROM_TAG^{commit}" >/dev/null 2>&1 \
    || fail "Tag or commit not found: $FROM_TAG"

CURRENT_HEAD="$(git -C "$REPO_ROOT" rev-parse HEAD)"
WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/canary-upgrade-test.XXXXXX")"
WORKTREE_DIR="$WORK_DIR/repo"
LOG_DIR="$WORK_DIR/logs"
mkdir -p "$LOG_DIR"

copy_self_hosted_env() {
    local repo_dir="$1"

    cp "$repo_dir/backend/.env.example.self-hosted" "$repo_dir/backend/.env"
    cp "$repo_dir/frontend/.env.example.self-hosted" "$repo_dir/frontend/.env.local"

    if ! grep -q '^NTFY_SERVER_URL=' "$repo_dir/backend/.env"; then
        printf '\nNTFY_SERVER_URL=%s\n' "$NTFY_URL" >> "$repo_dir/backend/.env"
    fi
}

migrate_legacy_data_dir_if_needed() {
    local repo_dir="$1"
    local legacy_dir="$repo_dir/backend/database/regtest"
    local target_dir="$repo_dir/backend/database/self-hosted/regtest"

    if [[ ! -d "$legacy_dir" ]]; then
        return 0
    fi

    mkdir -p "$target_dir"

    if [[ -f "$legacy_dir/metadata.sqlite" ]]; then
        rm -f "$target_dir/metadata.sqlite" "$target_dir/metadata.sqlite-shm" "$target_dir/metadata.sqlite-wal"
        sqlite3 "$legacy_dir/metadata.sqlite" ".backup '$target_dir/metadata.sqlite'"
        rm -f "$target_dir/metadata.sqlite-shm" "$target_dir/metadata.sqlite-wal"
    fi

    if [[ -d "$legacy_dir/wallets" ]]; then
        rm -rf "$target_dir/wallets"
        cp -R "$legacy_dir/wallets" "$target_dir/wallets"
    fi
}

find_bdk_wallet_db() {
    local repo_dir="$1"
    local checksum="$2"
    local candidate

    for candidate in \
        "$repo_dir/backend/database/self-hosted/regtest/wallets/${checksum}.sqlite" \
        "$repo_dir/backend/database/regtest/wallets/${checksum}.sqlite"; do
        if [[ -f "$candidate" ]]; then
            echo "$candidate"
            return 0
        fi
    done

    return 1
}

capture_pre_upgrade_bdk_state() {
    local wallet_db schema_version lock_table_count
    wallet_db="$(find_bdk_wallet_db "$WORKTREE_DIR" "$TARGET_WALLET_CHECKSUM")" \
        || fail "Could not find BDK wallet database for $TARGET_WALLET_CHECKSUM"

    schema_version="$(sqlite3 "$wallet_db" "SELECT version FROM bdk_schemas WHERE name = 'bdk_wallet';")"
    [[ "$schema_version" == "0" ]] \
        || fail "Expected bdk_wallet 2.x schema version 0 before upgrade, got $schema_version"
    lock_table_count="$(sqlite3 "$wallet_db" "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'bdk_wallet_locked_outpoints';")"
    [[ "$lock_table_count" == "0" ]] \
        || fail "BDK 2.x wallet unexpectedly contains bdk_wallet_locked_outpoints"

    PRE_UPGRADE_BDK_WALLET_ROW="$(sqlite3 -separator '|' "$wallet_db" \
        "SELECT descriptor, change_descriptor, network FROM bdk_wallet WHERE id = 0;")"
    PRE_UPGRADE_REVEALED_INDEXES="$(sqlite3 "$wallet_db" \
        "SELECT group_concat(last_revealed, ',') FROM (SELECT last_revealed FROM bdk_descriptor_last_revealed ORDER BY descriptor_id);")"
    [[ -n "$PRE_UPGRADE_BDK_WALLET_ROW" ]] || fail "BDK wallet descriptor state is empty"
    [[ -n "$PRE_UPGRADE_REVEALED_INDEXES" ]] || fail "BDK revealed-index state is empty"
}

assert_post_upgrade_bdk_state() {
    local wallet_db schema_version lock_table_count wallet_row revealed_indexes
    wallet_db="$(find_bdk_wallet_db "$WORKTREE_DIR" "$TARGET_WALLET_CHECKSUM")" \
        || fail "Could not find upgraded BDK wallet database for $TARGET_WALLET_CHECKSUM"

    schema_version="$(sqlite3 "$wallet_db" "SELECT version FROM bdk_schemas WHERE name = 'bdk_wallet';")"
    [[ "$schema_version" == "1" ]] \
        || fail "Expected migrated bdk_wallet schema version 1, got $schema_version"
    lock_table_count="$(sqlite3 "$wallet_db" "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'bdk_wallet_locked_outpoints';")"
    [[ "$lock_table_count" == "1" ]] \
        || fail "Upgrade did not create bdk_wallet_locked_outpoints"

    wallet_row="$(sqlite3 -separator '|' "$wallet_db" \
        "SELECT descriptor, change_descriptor, network FROM bdk_wallet WHERE id = 0;")"
    revealed_indexes="$(sqlite3 "$wallet_db" \
        "SELECT group_concat(last_revealed, ',') FROM (SELECT last_revealed FROM bdk_descriptor_last_revealed ORDER BY descriptor_id);")"
    [[ "$wallet_row" == "$PRE_UPGRADE_BDK_WALLET_ROW" ]] \
        || fail "BDK descriptors or network changed during upgrade"
    [[ "$revealed_indexes" == "$PRE_UPGRADE_REVEALED_INDEXES" ]] \
        || fail "BDK revealed indexes changed during upgrade"
}

assert_web_ports_available() {
    local timeout=60
    local pids

    while (( timeout > 0 )); do
        pids="$(lsof -tiTCP:3000 -tiTCP:"$FRONTEND_PORT" -sTCP:LISTEN 2>/dev/null | sort -u || true)"
        if [[ -z "$pids" ]]; then
            return 0
        fi

        sleep 1
        timeout=$((timeout - 1))
    done

    local compact_pids
    compact_pids="${pids//$'\n'/, }"
    fail "Ports 3000/${FRONTEND_PORT} are already in use by PID(s): $compact_pids. Stop those processes or set CANARY_UPGRADE_FRONTEND_PORT before running this upgrade test."
}

wait_for_http() {
    local url="$1"
    local timeout="$2"
    local label="$3"

    while (( timeout > 0 )); do
        if curl -fsS "$url" >/dev/null 2>&1; then
            return 0
        fi
        sleep 1
        timeout=$((timeout - 1))
    done

    fail "$label did not become ready: $url"
}

wait_for_backend() {
    local timeout=420

    while (( timeout > 0 )); do
        local status
        status="$(curl -s -o /dev/null -w '%{http_code}' "$BACKEND_URL/api/wallets" || true)"
        if [[ "$status" == "200" || "$status" == "401" ]]; then
            return 0
        fi

        sleep 1
        timeout=$((timeout - 1))
    done

    fail "Backend did not become ready: $BACKEND_URL/api/wallets"
}

api_curl() {
    if [[ -n "$AUTH_TOKEN" ]]; then
        curl -fsS -H "Authorization: Bearer $AUTH_TOKEN" "$@"
    else
        curl -fsS "$@"
    fi
}

authenticate_if_required() {
    local status payload response token

    AUTH_TOKEN=""
    status="$(curl -s -o /dev/null -w '%{http_code}' "$BACKEND_URL/api/wallets" || true)"
    if [[ "$status" != "401" ]]; then
        return 0
    fi

    payload="$(jq -n \
        --arg email "$SELF_HOSTED_ADMIN_EMAIL" \
        --arg password "$SELF_HOSTED_ADMIN_PASSWORD" \
        '{email: $email, password: $password}')"

    response="$(curl -fsS -X POST \
        -H "Content-Type: application/json" \
        -d "$payload" \
        "$BACKEND_URL/api/auth/login")"

    token="$(echo "$response" | jq -r '.token // empty')"
    [[ -n "$token" ]] || fail "Self-hosted login did not return a token"
    AUTH_TOKEN="$token"
}

wait_for_wallets_synced() {
    local timeout=180
    while (( timeout > 0 )); do
        local response funded_count
        response="$(api_curl "$BACKEND_URL/api/wallets")"
        funded_count="$(echo "$response" | jq '[.wallets[] | select(.balance_total > 0)] | length')"
        if [[ "$funded_count" -gt 0 ]]; then
            return 0
        fi
        sleep 2
        timeout=$((timeout - 2))
    done

    fail "Timed out waiting for funded wallets to sync"
}

btc() {
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 "$@"
}

btc_wallet() {
    local wallet_name="$1"
    shift
    docker exec bitcoind-regtest bitcoin-cli -rpcuser=bitcoin -rpcpassword=bitcoin -rpcport=8332 -rpcwallet="$wallet_name" "$@"
}

mine_blocks() {
    local blocks="${1:-1}"
    local miner_address
    miner_address="$(btc_wallet miner getnewaddress)"
    btc generatetoaddress "$blocks" "$miner_address" >/dev/null
}

wallet_list_json() {
    api_curl "$BACKEND_URL/api/wallets"
}

wallet_detail_json() {
    local checksum="$1"
    api_curl "$BACKEND_URL/api/wallets/${checksum}/detail"
}

select_target_wallet() {
    local wallet_list
    wallet_list="$(wallet_list_json)"

    TARGET_WALLET_CHECKSUM="$(
        echo "$wallet_list" | jq -r '
            (.wallets[] | select(.name == "segwit-desc" and .balance_total > 0) | .checksum),
            (.wallets[] | select(.name == "Alice (Regtest)" and .balance_total > 0) | .checksum),
            (.wallets[] | select(.name == "alice" and .balance_total > 0) | .checksum),
            (.wallets[] | select(.name == "Charlie (Regtest)" and .balance_total > 0) | .checksum),
            (.wallets[] | select(.balance_total > 0) | .checksum)
            ' | head -n1
    )"
    [[ -n "$TARGET_WALLET_CHECKSUM" ]] || fail "No funded wallet found in /api/wallets"

    TARGET_WALLET_NAME="$(echo "$wallet_list" | jq -r --arg checksum "$TARGET_WALLET_CHECKSUM" '.wallets[] | select(.checksum == $checksum) | .name')"
    EXPECTED_WALLET_COUNT="$(echo "$wallet_list" | jq '.wallets | length')"

    case "$TARGET_WALLET_NAME" in
        "Alice (Regtest)"|alice)
            TARGET_BTC_WALLET="alice"
            ;;
        "Bob (Regtest)"|bob)
            TARGET_BTC_WALLET="bob"
            ;;
        "Charlie (Regtest)"|Charlie)
            TARGET_BTC_WALLET="charlie"
            ;;
        *)
            TARGET_BTC_WALLET="$TARGET_WALLET_NAME"
            ;;
    esac
}

create_ntfy_contact() {
    NTFY_TOPIC="canary-upgrade-test-$(date +%s)"

    local payload response
    payload="$(jq -n --arg topic "$NTFY_TOPIC" '{
        name: "Upgrade Test Contact",
        notification_methods: [
            {
                provider_type: "ntfy",
                notification_target: $topic
            }
        ]
    }')"

    response="$(api_curl -X POST \
        -H "Content-Type: application/json" \
        -d "$payload" \
        "$BACKEND_URL/api/wallets/${TARGET_WALLET_CHECKSUM}/contacts")"

    echo "$response" | jq -e '
        (.contact_id | type == "string")
    ' >/dev/null || fail "Failed to create ntfy contact: $response"
}

assert_contact_present() {
    local detail
    detail="$(wallet_detail_json "$TARGET_WALLET_CHECKSUM")"
    echo "$detail" | jq -e --arg topic "$NTFY_TOPIC" '
        [.contacts[].notification_methods[]? | select(.notification_target == $topic)] | length > 0
    ' >/dev/null || fail "Expected ntfy topic not found in wallet detail"
}

send_transaction_to_target_wallet() {
    local amount="$1"
    local receive_address txid

    btc loadwallet "$TARGET_BTC_WALLET" >/dev/null 2>&1 || true
    receive_address="$(btc_wallet "$TARGET_BTC_WALLET" getnewaddress)"
    txid="$(btc_wallet miner sendtoaddress "$receive_address" "$amount")"
    echo "$txid"
}

wait_for_transaction_status() {
    local checksum="$1"
    local txid="$2"
    local expected="$3"
    local timeout=180

    while (( timeout > 0 )); do
        local detail
        detail="$(wallet_detail_json "$checksum")"
        if echo "$detail" | jq -e --arg txid "$txid" --arg expected "$expected" '
            [.transactions[] | select(.txid == $txid and .transaction_status == $expected)] | length > 0
        ' >/dev/null; then
            return 0
        fi
        sleep 2
        timeout=$((timeout - 2))
    done

    fail "Timed out waiting for transaction ${txid} to reach status ${expected}"
}

capture_snapshot() {
    local checksum="$1"
    wallet_list_json > "$LOG_DIR/wallets-${checksum}.json"
    wallet_detail_json "$checksum" > "$LOG_DIR/wallet-detail-${checksum}.json"
}

assert_post_upgrade_state() {
    local detail wallets
    wallets="$(wallet_list_json)"
    detail="$(wallet_detail_json "$TARGET_WALLET_CHECKSUM")"

    echo "$wallets" | jq -e --arg checksum "$TARGET_WALLET_CHECKSUM" --arg name "$TARGET_WALLET_NAME" '
        [.wallets[] | select(.checksum == $checksum and .name == $name)] | length == 1
    ' >/dev/null || fail "Wallet list no longer contains the expected target wallet"

    echo "$detail" | jq -e --arg topic "$NTFY_TOPIC" --arg pre_txid "$PRE_UPGRADE_TXID" '
        ([.contacts[].notification_methods[]? | select(.notification_target == $topic)] | length > 0)
        and ([.transactions[] | select(.txid == $pre_txid)] | length > 0)
    ' >/dev/null || fail "Post-upgrade detail is missing the contact or the pre-upgrade transaction"
}

configure_ntfy_credentials() {
    local payload

    payload="$(jq -n \
        --arg server_url "$NTFY_URL" \
        --arg username "$NTFY_USERNAME" \
        --arg password "$NTFY_PASSWORD" \
        '{
            ntfy_server_url: $server_url,
            ntfy_username: $username,
            ntfy_password: $password
        }')"

    api_curl -X PUT \
        -H "Content-Type: application/json" \
        -d "$payload" \
        "$BACKEND_URL/api/user/preferences" >/dev/null
}

wait_for_notification_status() {
    local txid="$1"
    local timeout=180

    [[ "$txid" =~ ^[0-9a-fA-F]{64}$ ]] || fail "Invalid transaction id: $txid"

    while (( timeout > 0 )); do
        local detail
        detail="$(wallet_detail_json "$TARGET_WALLET_CHECKSUM")"

        if echo "$detail" | jq -e --arg txid "$txid" '
            [.transactions[] | select(.txid == $txid and (.notification_status | length > 0))] | length > 0
        ' >/dev/null; then
            return 0
        fi

        if [[ -f "$WORKTREE_DIR/backend/database/self-hosted/regtest/metadata.sqlite" ]] &&
            sqlite3 "$WORKTREE_DIR/backend/database/self-hosted/regtest/metadata.sqlite" \
                "SELECT COUNT(*) FROM notification_logs WHERE transaction_txid = '$txid';" 2>/dev/null |
            grep -Eq '^[1-9][0-9]*$'; then
            return 0
        fi

        sleep 2
        timeout=$((timeout - 2))
    done

    fail "Expected notification_status entries for transaction ${txid}"
}

run_playwright() {
    local stage_tag="$1"
    local txid="$2"
    local txid_prefix

    txid_prefix="${txid:0:12}"

    (
        cd "$PLAYWRIGHT_DIR"
        WALLET_CHECKSUM="$TARGET_WALLET_CHECKSUM" \
        WALLET_NAME="$TARGET_WALLET_NAME" \
        NTFY_TOPIC="$NTFY_TOPIC" \
        EXPECTED_WALLET_COUNT="$EXPECTED_WALLET_COUNT" \
        TXID_PREFIX="$txid_prefix" \
        AUTH_TOKEN="$AUTH_TOKEN" \
        FRONTEND_URL="$FRONTEND_URL" \
        npx playwright test --grep "$stage_tag"
    )
}

start_backend() {
    local repo_dir="$1"
    (
        cd "$repo_dir/backend"
        cargo run >"$LOG_DIR/backend.log" 2>&1
    ) &
    BACKEND_PID=$!
    wait_for_backend
    authenticate_if_required
}

start_frontend() {
    local repo_dir="$1"
    (
        cd "$repo_dir/frontend"
        pnpm install --frozen-lockfile >"$LOG_DIR/frontend-install.log" 2>&1
        mkdir -p logs
        pnpm exec next dev --port "$FRONTEND_PORT" >"$LOG_DIR/frontend.log" 2>&1
    ) &
    FRONTEND_PID=$!
    wait_for_http "$FRONTEND_URL" 300 "Frontend"
}

stop_app_processes() {
    kill_if_running "$BACKEND_PID"
    kill_if_running "$FRONTEND_PID"
    BACKEND_PID=""
    FRONTEND_PID=""
}

prepare_playwright() {
    if [[ "$SKIP_PLAYWRIGHT_INSTALL" -eq 1 ]]; then
        [[ -d "$PLAYWRIGHT_DIR/node_modules/@playwright/test" ]] \
            || fail "--skip-playwright-install requires existing dependencies under $PLAYWRIGHT_DIR/node_modules"
        return 0
    fi

    (
        cd "$PLAYWRIGHT_DIR"
        npm ci >"$LOG_DIR/playwright-install.log" 2>&1
        npx playwright install chromium >"$LOG_DIR/playwright-browser-install.log" 2>&1
    )
}

seed_old_version() {
    local old_script_dir="$1/scripts"

    if grep -q '^[[:space:]]*init)' "$old_script_dir/dev.sh"; then
        if (
            cd "$old_script_dir"
            ./dev.sh init >"$LOG_DIR/old-init.log" 2>&1
        ); then
            return 0
        fi

        # Recent BDK-2 releases can create and fund the Bitcoin Core wallets but their
        # legacy helper does not authenticate when POSTing them to Canary. Add one of the
        # real seeded descriptor wallets through this harness's authenticated API instead.
        local receive_descriptor multipath_raw descriptor_checksum descriptor payload response
        receive_descriptor="$(btc_wallet segwit-desc listdescriptors | jq -r \
            '.descriptors[] | select(.desc | startswith("wpkh(") and contains("/0/*")) | .desc')"
        [[ -n "$receive_descriptor" ]] \
            || fail "Old release did not create the seeded segwit-desc wallet"
        multipath_raw="$(echo "$receive_descriptor" | sed 's|/0/\*|/<0;1>/\*|' | sed 's/#[^#]*$//')"
        descriptor_checksum="$(btc getdescriptorinfo "$multipath_raw" | jq -r '.checksum')"
        descriptor="${multipath_raw}#${descriptor_checksum}"
        payload="$(jq -n --arg name "segwit-desc" --arg descriptor "$descriptor" \
            '{name: $name, descriptor: $descriptor}')"
        response="$(api_curl -X POST \
            -H "Content-Type: application/json" \
            -d "$payload" \
            "$BACKEND_URL/api/wallets")"
        echo "$response" | jq -e '.wallet.checksum | type == "string"' >/dev/null \
            || fail "Failed to add seeded BDK-2 wallet through authenticated API: $response"
        return 0
    fi

    if grep -q 'create-wallets' "$old_script_dir/dev.sh"; then
        (
            cd "$old_script_dir"
            ./dev.sh create-wallets >"$LOG_DIR/old-create-wallets.log" 2>&1
            if grep -q 'add-wallets-to-backend' dev.sh; then
                ./dev.sh add-wallets-to-backend >"$LOG_DIR/old-add-wallets.log" 2>&1
            fi
        )
        return 0
    fi

    fail "Could not find a wallet initialization command in $old_script_dir/dev.sh"
}

log "Creating temporary worktree from ${FROM_TAG}"
ref_uses_bdk_wallet_2 "$FROM_TAG" \
    || fail "Upgrade source ${FROM_TAG} does not use bdk_wallet 2.x"
git -C "$REPO_ROOT" worktree add --detach "$WORKTREE_DIR" "$FROM_TAG" >/dev/null
copy_self_hosted_env "$WORKTREE_DIR"

log "Resetting local ports and regtest infrastructure"
stop_app_processes
assert_web_ports_available
(
    cd "$SCRIPT_DIR"
    docker_compose down -v >"$LOG_DIR/docker-reset.log" 2>&1
)
rm -rf "$WORKTREE_DIR/backend/database/self-hosted/regtest"

log "Starting shared Docker infrastructure"
(
    cd "$SCRIPT_DIR"
    ./dev.sh start >"$LOG_DIR/docker-start.log" 2>&1
)

log "Installing isolated Playwright dependencies"
prepare_playwright

log "Starting old version from ${FROM_TAG}"
start_backend "$WORKTREE_DIR"
start_frontend "$WORKTREE_DIR"

log "Seeding self-hosted data on old version"
seed_old_version "$WORKTREE_DIR"
wait_for_wallets_synced
select_target_wallet
capture_snapshot "$TARGET_WALLET_CHECKSUM"

log "Adding ntfy contact and verifying wallet detail"
create_ntfy_contact
assert_contact_present

log "Sending a pre-upgrade transaction"
PRE_UPGRADE_TXID="$(send_transaction_to_target_wallet "0.01")"
wait_for_transaction_status "$TARGET_WALLET_CHECKSUM" "$PRE_UPGRADE_TXID" "pending"
mine_blocks 1
wait_for_transaction_status "$TARGET_WALLET_CHECKSUM" "$PRE_UPGRADE_TXID" "confirmed"

log "Running pre-upgrade Playwright verification"
run_playwright '@pre-upgrade' "$PRE_UPGRADE_TXID"

log "Upgrading worktree to current HEAD"
stop_app_processes
capture_pre_upgrade_bdk_state
assert_web_ports_available
git -C "$WORKTREE_DIR" checkout --detach "$CURRENT_HEAD" >"$LOG_DIR/git-checkout-head.log" 2>&1
copy_self_hosted_env "$WORKTREE_DIR"
migrate_legacy_data_dir_if_needed "$WORKTREE_DIR"

log "Starting upgraded version"
start_backend "$WORKTREE_DIR"
start_frontend "$WORKTREE_DIR"

log "Verifying preserved state after upgrade"
assert_post_upgrade_state
assert_post_upgrade_bdk_state
configure_ntfy_credentials

log "Running post-upgrade Playwright verification"
run_playwright '@post-upgrade' "$PRE_UPGRADE_TXID"

log "Sending a post-upgrade transaction"
POST_UPGRADE_TXID="$(send_transaction_to_target_wallet "0.02")"
wait_for_transaction_status "$TARGET_WALLET_CHECKSUM" "$POST_UPGRADE_TXID" "pending"
mine_blocks 1
wait_for_transaction_status "$TARGET_WALLET_CHECKSUM" "$POST_UPGRADE_TXID" "confirmed"

wait_for_notification_status "$POST_UPGRADE_TXID"

log "Upgrade test passed"
echo "From tag:        $FROM_TAG"
echo "Upgraded commit: $CURRENT_HEAD"
echo "Wallet:          $TARGET_WALLET_NAME ($TARGET_WALLET_CHECKSUM)"
echo "ntfy topic:      $NTFY_TOPIC"
echo "Pre-upgrade tx:  $PRE_UPGRADE_TXID"
echo "Post-upgrade tx: $POST_UPGRADE_TXID"
echo "Logs:            $LOG_DIR"
