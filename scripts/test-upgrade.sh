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
TO_REF="HEAD"
KEEP_WORKTREE=0
SKIP_PLAYWRIGHT_INSTALL=0
RUN_SUCCEEDED=0

WORK_DIR=""
WORKTREE_DIR=""
LOG_DIR=""
BACKEND_PID=""
FRONTEND_PID=""

SOURCE_SHA=""
TARGET_SHA=""

TARGET_WALLET_CHECKSUM=""
TARGET_WALLET_NAME=""
TARGET_BTC_WALLET=""
NTFY_TOPIC_A=""
NTFY_TOPIC_B=""
NTFY_TOPIC_INACTIVE=""
CONTACT_A_ID=""
CONTACT_B_ID=""
INACTIVE_CONTACT_ID=""
LEGACY_BALANCE_ALERT_ID=""
LEGACY_BALANCE_THRESHOLD_SATS=""
PRE_UPGRADE_TXID=""
EXPECTED_WALLET_COUNT=""
AUTH_TOKEN=""
PRE_UPGRADE_BDK_WALLET_ROW=""
PRE_UPGRADE_REVEALED_INDEXES=""

usage() {
    cat <<'EOF'
Usage: ./scripts/test-upgrade.sh [--from-tag <tag>] [--to-ref <ref>] [--keep-worktree] [--skip-playwright-install]

Options:
  --from-tag <tag>           Upgrade from the given BDK-2 tag. Defaults to the latest such release.
  --to-ref <ref>             Upgrade to this commit-ish. Defaults to HEAD.
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
    local exit_status=$?
    set +e
    kill_if_running "$BACKEND_PID"
    kill_if_running "$FRONTEND_PID"

    if [[ "$exit_status" -ne 0 && -n "$LOG_DIR" && -d "$LOG_DIR" ]]; then
        KEEP_WORKTREE=1
        if docker info >/dev/null 2>&1; then
            (
                cd "$SCRIPT_DIR"
                docker_compose logs --no-color >"$LOG_DIR/docker-services.log" 2>&1
            )
        fi
        capture_database_artifacts "failure"
        printf '\nUpgrade gate failed. Artifacts retained at: %s\n' "$WORK_DIR" >&2
        printf '  logs:     %s\n' "$LOG_DIR" >&2
        printf '  worktree: %s\n' "$WORKTREE_DIR" >&2
    fi

    if [[ "$RUN_SUCCEEDED" -eq 1 && "$KEEP_WORKTREE" -eq 0 && -n "$WORKTREE_DIR" && -d "$WORKTREE_DIR" ]]; then
        git -C "$REPO_ROOT" worktree remove --force "$WORKTREE_DIR" >/dev/null 2>&1 || true
    fi

    if [[ "$RUN_SUCCEEDED" -eq 1 && "$KEEP_WORKTREE" -eq 0 && -n "$WORK_DIR" && -d "$WORK_DIR" ]]; then
        rm -rf "$WORK_DIR"
    fi

    return "$exit_status"
}

trap cleanup EXIT

while [[ $# -gt 0 ]]; do
    case "$1" in
        --from-tag)
            [[ $# -ge 2 ]] || fail "--from-tag requires a value"
            FROM_TAG="$2"
            shift 2
            ;;
        --to-ref)
            [[ $# -ge 2 ]] || fail "--to-ref requires a value"
            TO_REF="$2"
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

git -C "$REPO_ROOT" rev-parse --verify "${TO_REF}^{commit}" >/dev/null 2>&1 \
    || fail "Target ref not found: $TO_REF"

SOURCE_SHA="$(git -C "$REPO_ROOT" rev-parse "${FROM_TAG}^{commit}")"
TARGET_SHA="$(git -C "$REPO_ROOT" rev-parse "${TO_REF}^{commit}")"
WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/canary-upgrade-test.XXXXXX")"
WORKTREE_DIR="$WORK_DIR/repo"
LOG_DIR="$WORK_DIR/logs"
mkdir -p "$LOG_DIR"

jq -n \
    --arg source_ref "$FROM_TAG" \
    --arg source_sha "$SOURCE_SHA" \
    --arg target_ref "$TO_REF" \
    --arg target_sha "$TARGET_SHA" \
    '{source: {ref: $source_ref, sha: $source_sha}, target: {ref: $target_ref, sha: $target_sha}}' \
    >"$LOG_DIR/refs.json"

copy_self_hosted_env() {
    local repo_dir="$1"

    cp "$repo_dir/backend/.env.example.self-hosted" "$repo_dir/backend/.env"
    cp "$repo_dir/frontend/.env.example.self-hosted" "$repo_dir/frontend/.env.local"

    if ! grep -q '^NTFY_SERVER_URL=' "$repo_dir/backend/.env"; then
        printf '\nNTFY_SERVER_URL=%s\n' "$NTFY_URL" >> "$repo_dir/backend/.env"
    fi
    if grep -q '^CANARY_SYNC_INTERVAL=' "$repo_dir/backend/.env"; then
        sed -i.bak 's/^CANARY_SYNC_INTERVAL=.*/CANARY_SYNC_INTERVAL=2/' "$repo_dir/backend/.env"
        rm -f "$repo_dir/backend/.env.bak"
    else
        printf 'CANARY_SYNC_INTERVAL=2\n' >> "$repo_dir/backend/.env"
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

metadata_db_path() {
    local repo_dir="$1"
    local candidate

    for candidate in \
        "$repo_dir/backend/database/self-hosted/regtest/metadata.sqlite" \
        "$repo_dir/backend/database/regtest/metadata.sqlite"; do
        if [[ -f "$candidate" ]]; then
            echo "$candidate"
            return 0
        fi
    done

    return 1
}

capture_database_artifacts() {
    local label="$1"
    local db_path snapshot_dir

    [[ -n "$WORKTREE_DIR" && -d "$WORKTREE_DIR" ]] || return 0
    db_path="$(metadata_db_path "$WORKTREE_DIR" 2>/dev/null || true)"
    [[ -n "$db_path" && -f "$db_path" ]] || return 0

    snapshot_dir="$LOG_DIR/database-$label"
    mkdir -p "$snapshot_dir"
    sqlite3 "$db_path" ".backup '$snapshot_dir/metadata.sqlite'" || true
    sqlite3 -json "$db_path" \
        "SELECT * FROM contacts ORDER BY created_at, id;" \
        >"$snapshot_dir/contacts.json" 2>/dev/null || true
    sqlite3 -json "$db_path" \
        "SELECT * FROM contact_notification_methods ORDER BY created_at, id;" \
        >"$snapshot_dir/contact-notification-methods.json" 2>/dev/null || true
    sqlite3 -json "$db_path" \
        "SELECT * FROM balance_alerts ORDER BY created_at, id;" \
        >"$snapshot_dir/balance-alerts.json" 2>/dev/null || true
    sqlite3 -json "$db_path" \
        "SELECT * FROM balance_alert_notifications ORDER BY created_at, id;" \
        >"$snapshot_dir/balance-alert-notifications.json" 2>/dev/null || true
    sqlite3 -json "$db_path" \
        "SELECT * FROM notification_logs ORDER BY created_at, id;" \
        >"$snapshot_dir/notification-logs.json" 2>/dev/null || true
    sqlite3 -json "$db_path" \
        "SELECT * FROM balance_alert_notification_logs ORDER BY created_at, id;" \
        >"$snapshot_dir/balance-alert-notification-logs.json" 2>/dev/null || true
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
    local name="$1"
    local topic="$2"
    local payload response
    payload="$(jq -n --arg name "$name" --arg topic "$topic" '{
        name: $name,
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

    echo "$response" | jq -r '.contact_id'
}

seed_inactive_ntfy_contact() {
    local db_path
    db_path="$(metadata_db_path "$WORKTREE_DIR")" \
        || fail "Could not find metadata database for inactive contact fixture"

    INACTIVE_CONTACT_ID="upgrade-inactive-contact"
    sqlite3 "$db_path" <<SQL
PRAGMA busy_timeout = 10000;
BEGIN IMMEDIATE;
INSERT INTO contacts (id, wallet_checksum, name, is_active)
VALUES ('$INACTIVE_CONTACT_ID', '$TARGET_WALLET_CHECKSUM', 'Upgrade Inactive', 0);
INSERT INTO contact_notification_methods (
    id, contact_id, provider_type, notification_target, wallet_checksum
)
VALUES (
    'upgrade-inactive-method', '$INACTIVE_CONTACT_ID', 'ntfy', '$NTFY_TOPIC_INACTIVE', '$TARGET_WALLET_CHECKSUM'
);
COMMIT;
SQL
}

assert_contacts_present() {
    local detail
    detail="$(wallet_detail_json "$TARGET_WALLET_CHECKSUM")"
    echo "$detail" | jq -e \
        --arg topic_a "$NTFY_TOPIC_A" \
        --arg topic_b "$NTFY_TOPIC_B" \
        --arg topic_inactive "$NTFY_TOPIC_INACTIVE" '
        ([.contacts[].notification_methods[]? | .notification_target] | sort)
        | contains([$topic_a, $topic_b, $topic_inactive] | sort)
    ' >/dev/null || fail "Expected active/inactive ntfy topics not found in wallet detail"
}

wallet_balance_sats() {
    wallet_list_json | jq -r --arg checksum "$TARGET_WALLET_CHECKSUM" \
        '.wallets[] | select(.checksum == $checksum) | .balance_total'
}

create_legacy_balance_alert() {
    local current_balance payload response
    current_balance="$(wallet_balance_sats)"
    LEGACY_BALANCE_THRESHOLD_SATS=$((current_balance - 1000000))
    [[ "$LEGACY_BALANCE_THRESHOLD_SATS" -gt 0 ]] \
        || fail "Wallet balance is too small for the balance-threshold scenario"

    payload="$(jq -n --argjson threshold "$LEGACY_BALANCE_THRESHOLD_SATS" \
        '{threshold_sats: $threshold, alert_type: "below"}')"
    response="$(api_curl -X POST -H "Content-Type: application/json" -d "$payload" \
        "$BACKEND_URL/api/wallets/${TARGET_WALLET_CHECKSUM}/balance-alerts")"
    LEGACY_BALANCE_ALERT_ID="$(echo "$response" | jq -r '.id // empty')"
    [[ -n "$LEGACY_BALANCE_ALERT_ID" ]] \
        || fail "Failed to create legacy balance alert: $response"
}

btc_mempool() {
    btc getrawmempool | jq -cS 'sort'
}

dev_mempool_delta() {
    local label="$1"
    local expected_new="$2"
    shift 2
    local before after delta output_file

    before="$(btc_mempool)"
    output_file="$LOG_DIR/dev-${label}.log"
    (
        cd "$SCRIPT_DIR"
        ./dev.sh "$@"
    ) >"$output_file" 2>&1
    after="$(btc_mempool)"
    delta="$(jq -n --argjson before "$before" --argjson after "$after" \
        '$after - $before')"
    [[ "$(echo "$delta" | jq 'length')" -eq "$expected_new" ]] \
        || fail "$label created an unexpected mempool delta: $delta"
    echo "$delta" | jq -r '.[0]'
}

dev_rbf_delta() {
    local label="$1"
    local original_txid="$2"
    local before after delta output_file

    before="$(btc_mempool)"
    [[ "$(echo "$before" | jq --arg txid "$original_txid" 'index($txid) != null')" == "true" ]] \
        || fail "RBF original $original_txid is not in the mempool"
    output_file="$LOG_DIR/dev-${label}.log"
    (
        cd "$SCRIPT_DIR"
        ./dev.sh "$TARGET_BTC_WALLET" rbf "$original_txid"
    ) >"$output_file" 2>&1
    after="$(btc_mempool)"
    delta="$(jq -n --argjson before "$before" --argjson after "$after" '$after - $before')"
    [[ "$(echo "$delta" | jq 'length')" -eq 1 ]] \
        || fail "$label did not create exactly one replacement: $delta"
    [[ "$(echo "$after" | jq --arg txid "$original_txid" 'index($txid) == null')" == "true" ]] \
        || fail "$label left the replaced transaction in the mempool"
    echo "$delta" | jq -r '.[0]'
}

dev_mine() {
    local label="$1"
    (
        cd "$SCRIPT_DIR"
        ./dev.sh mine 1
    ) >"$LOG_DIR/dev-mine-${label}.log" 2>&1
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

    echo "$detail" | jq -e \
        --arg topic_a "$NTFY_TOPIC_A" \
        --arg topic_b "$NTFY_TOPIC_B" \
        --arg topic_inactive "$NTFY_TOPIC_INACTIVE" \
        --arg pre_txid "$PRE_UPGRADE_TXID" '
        ([.contacts[].notification_methods[]? | .notification_target]
            | contains([$topic_a, $topic_b, $topic_inactive]))
        and ([.transactions[] | select(.txid == $pre_txid)] | length > 0)
    ' >/dev/null || fail "Post-upgrade detail is missing contacts or the pre-upgrade transaction"
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

scenario_txids_sql() {
    local scenario_file="$1"
    jq -r '[.transactions[]] | map("\u0027" + . + "\u0027") | join(",")' "$scenario_file"
}

scenario_alert_ids_sql() {
    local scenario_file="$1"
    jq -r '.alert_ids | map("\u0027" + . + "\u0027") | join(",")' "$scenario_file"
}

stage_delivery_ready() {
    local scenario_file="$1"
    local db_path txids_sql alert_ids_sql topic txid expected_count count
    db_path="$(metadata_db_path "$WORKTREE_DIR")" || return 1
    txids_sql="$(scenario_txids_sql "$scenario_file")"
    alert_ids_sql="$(scenario_alert_ids_sql "$scenario_file")"

    for topic in "$NTFY_TOPIC_A" "$NTFY_TOPIC_B"; do
        while IFS=$'\t' read -r txid expected_count; do
            count="$(sqlite3 "$db_path" \
                "SELECT COUNT(*) FROM notification_logs
                 WHERE transaction_txid = '$txid'
                   AND notification_target_snapshot = '$topic'
                   AND status = 'sent';")"
            [[ "$count" -eq "$expected_count" ]] || return 1
        done < <(jq -r '.transaction_deliveries[] | [.txid, .count] | @tsv' "$scenario_file")

        count="$(sqlite3 "$db_path" \
            "SELECT COUNT(*) FROM balance_alert_notification_logs
             WHERE balance_alert_id IN ($alert_ids_sql)
               AND notification_target_snapshot = '$topic'
               AND status = 'sent';")"
        [[ "$count" -eq 1 ]] || return 1
    done

    [[ "$(sqlite3 "$db_path" \
        "SELECT COUNT(*) FROM notification_logs
         WHERE transaction_txid IN ($txids_sql) AND status = 'failed';")" -eq 0 ]] || return 1
    [[ "$(sqlite3 "$db_path" \
        "SELECT COUNT(*) FROM balance_alert_notification_logs
         WHERE balance_alert_id IN ($alert_ids_sql) AND status = 'failed';")" -eq 0 ]] || return 1
}

wait_for_stage_delivery() {
    local scenario_file="$1"
    local timeout=180
    while (( timeout > 0 )); do
        if stage_delivery_ready "$scenario_file"; then
            return 0
        fi
        sleep 2
        timeout=$((timeout - 2))
    done
    fail "Timed out waiting for the complete notification matrix in $(basename "$scenario_file")"
}

capture_ntfy_topic() {
    local topic="$1"
    local start_time="$2"
    local end_time="$3"
    local output_file="$4"
    local raw_file="${output_file%.json}.ndjson"

    curl -fsS -u "$NTFY_USERNAME:$NTFY_PASSWORD" \
        "$NTFY_URL/$topic/json?poll=1&since=all" >"$raw_file"
    jq -s --arg topic "$topic" --argjson start "$start_time" --argjson end "$end_time" '
        [.[] | select(.event == "message" and .topic == $topic and .time >= $start and .time <= $end)]
    ' "$raw_file" >"$output_file"
}

capture_ntfy_stage() {
    local stage="$1"
    local start_time="$2"
    local end_time="$3"
    capture_ntfy_topic "$NTFY_TOPIC_A" "$start_time" "$end_time" "$LOG_DIR/ntfy-${stage}-a.json"
    capture_ntfy_topic "$NTFY_TOPIC_B" "$start_time" "$end_time" "$LOG_DIR/ntfy-${stage}-b.json"
    capture_ntfy_topic "$NTFY_TOPIC_INACTIVE" "$start_time" "$end_time" "$LOG_DIR/ntfy-${stage}-inactive.json"
}

ntfy_message_count() {
    local topic="$1"
    curl -fsS -u "$NTFY_USERNAME:$NTFY_PASSWORD" \
        "$NTFY_URL/$topic/json?poll=1&since=all" | jq -s '[.[] | select(.event == "message")] | length'
}

assert_restart_does_not_duplicate() {
    local stage="$1"
    local before_a before_b after_a after_b
    before_a="$(ntfy_message_count "$NTFY_TOPIC_A")"
    before_b="$(ntfy_message_count "$NTFY_TOPIC_B")"

    kill_if_running "$BACKEND_PID"
    BACKEND_PID=""
    start_backend "$WORKTREE_DIR" "${stage}-restart"
    sleep 6

    after_a="$(ntfy_message_count "$NTFY_TOPIC_A")"
    after_b="$(ntfy_message_count "$NTFY_TOPIC_B")"
    [[ "$after_a" -eq "$before_a" && "$after_b" -eq "$before_b" ]] \
        || fail "Backend restart duplicated notifications during $stage (A: $before_a->$after_a, B: $before_b->$after_b)"
}

assert_post_upgrade_content_translation() {
    local db_path expected_count
    db_path="$(metadata_db_path "$WORKTREE_DIR")" \
        || fail "Could not find upgraded metadata database"
    expected_count="$(sqlite3 "$db_path" \
        "SELECT COUNT(*)
         FROM contact_notification_methods method
         JOIN contacts contact ON contact.id = method.contact_id
         WHERE contact.id IN ('$CONTACT_A_ID', '$CONTACT_B_ID', '$INACTIVE_CONTACT_ID')
           AND method.provider_type = 'ntfy'
           AND method.is_enabled = 1
           AND method.content_wallet_name = 1
           AND method.content_event_type = 1
           AND method.content_transaction_amount = 1
           AND method.content_transaction_balance = 0
           AND method.content_balance_alert_condition = 1
           AND method.content_balance_alert_threshold = 1
           AND method.content_balance_alert_balance = 1;")"
    [[ "$expected_count" -eq 3 ]] \
        || fail "v1.5.2 content settings did not translate without privacy expansion"
}

validate_stage_artifacts() {
    local stage="$1"
    local scenario_file="$2"
    local db_path txids_sql alert_ids_sql topic topic_file txid expected_count count total expected_total expected_body_file actual_body_file duplicate_count wrong_topic_count current_balance
    db_path="$(metadata_db_path "$WORKTREE_DIR")" || fail "Metadata database is missing"
    txids_sql="$(scenario_txids_sql "$scenario_file")"
    alert_ids_sql="$(scenario_alert_ids_sql "$scenario_file")"

    sqlite3 -json "$db_path" \
        "SELECT * FROM notification_logs WHERE transaction_txid IN ($txids_sql) ORDER BY created_at, id;" \
        >"$LOG_DIR/notification-logs-${stage}.json"
    sqlite3 -json "$db_path" \
        "SELECT * FROM balance_alert_notification_logs WHERE balance_alert_id IN ($alert_ids_sql) ORDER BY created_at, id;" \
        >"$LOG_DIR/balance-notification-logs-${stage}.json"

    wrong_topic_count="$(sqlite3 "$db_path" \
        "SELECT
           (SELECT COUNT(*) FROM notification_logs
            WHERE transaction_txid IN ($txids_sql)
              AND notification_target_snapshot NOT IN ('$NTFY_TOPIC_A', '$NTFY_TOPIC_B'))
           +
           (SELECT COUNT(*) FROM balance_alert_notification_logs
            WHERE balance_alert_id IN ($alert_ids_sql)
              AND notification_target_snapshot NOT IN ('$NTFY_TOPIC_A', '$NTFY_TOPIC_B'));")"
    [[ "$wrong_topic_count" -eq 0 ]] || fail "$stage delivered to an unexpected topic"

    duplicate_count="$(sqlite3 "$db_path" \
        "SELECT COUNT(*) FROM (
           SELECT transaction_txid, notification_target_snapshot, message_content, COUNT(*) AS copies
           FROM notification_logs
           WHERE transaction_txid IN ($txids_sql)
           GROUP BY transaction_txid, notification_target_snapshot, message_content
           HAVING copies > 1
         );")"
    [[ "$duplicate_count" -eq 0 ]] || fail "$stage contains duplicate transaction deliveries"

    for topic in "$NTFY_TOPIC_A" "$NTFY_TOPIC_B"; do
        if [[ "$topic" == "$NTFY_TOPIC_A" ]]; then
            topic_file="$LOG_DIR/ntfy-${stage}-a.json"
        else
            topic_file="$LOG_DIR/ntfy-${stage}-b.json"
        fi
        total="$(jq 'length' "$topic_file")"
        expected_total="$(jq '[.transaction_deliveries[].count] | add + 1' "$scenario_file")"
        [[ "$total" -eq "$expected_total" ]] \
            || fail "$stage topic $topic received $total messages; expected $expected_total"

        while IFS=$'\t' read -r txid expected_count; do
            count="$(sqlite3 "$db_path" \
                "SELECT COUNT(*) FROM notification_logs
                 WHERE transaction_txid = '$txid'
                   AND notification_target_snapshot = '$topic'
                   AND status = 'sent';")"
            [[ "$count" -eq "$expected_count" ]] \
                || fail "$stage transaction $txid delivered $count times to $topic; expected $expected_count"
        done < <(jq -r '.transaction_deliveries[] | [.txid, .count] | @tsv' "$scenario_file")

        count="$(sqlite3 "$db_path" \
            "SELECT COUNT(*) FROM balance_alert_notification_logs
             WHERE balance_alert_id IN ($alert_ids_sql)
               AND notification_target_snapshot = '$topic'
               AND status = 'sent';")"
        [[ "$count" -eq 1 ]] \
            || fail "$stage balance alert delivered $count times to $topic; expected 1"

        expected_body_file="$LOG_DIR/expected-bodies-${stage}-$(basename "$topic_file")"
        actual_body_file="$LOG_DIR/actual-bodies-${stage}-$(basename "$topic_file")"
        {
            sqlite3 "$db_path" \
                "SELECT json_quote(message_content) FROM notification_logs
                 WHERE transaction_txid IN ($txids_sql)
                   AND notification_target_snapshot = '$topic'
                   AND status = 'sent';"
            sqlite3 "$db_path" \
                "SELECT json_quote(message_content) FROM balance_alert_notification_logs
                 WHERE balance_alert_id IN ($alert_ids_sql)
                   AND notification_target_snapshot = '$topic'
                   AND status = 'sent';"
        } | jq -sS 'sort' >"$expected_body_file"
        jq -S '[.[].message] | sort' "$topic_file" >"$actual_body_file"
        cmp -s "$expected_body_file" "$actual_body_file" \
            || fail "$stage ntfy bodies do not match the successful database delivery log for $topic"
    done

    [[ "$(jq 'length' "$LOG_DIR/ntfy-${stage}-inactive.json")" -eq 0 ]] \
        || fail "$stage delivered a notification to the inactive contact"

    current_balance="$(sqlite3 "$db_path" \
        "SELECT group_concat(DISTINCT current_balance_sats)
         FROM balance_alert_notifications
         WHERE balance_alert_id IN ($alert_ids_sql);")"
    [[ -n "$current_balance" && "$current_balance" != *,* ]] \
        || fail "$stage balance alert did not preserve one semantic current-balance value"

    jq -n \
        --arg stage "$stage" \
        --arg topic_a "$NTFY_TOPIC_A" \
        --arg topic_b "$NTFY_TOPIC_B" \
        --argjson threshold "$LEGACY_BALANCE_THRESHOLD_SATS" \
        --argjson current_balance "$current_balance" '
        def content: {
          wallet_name: true,
          event_type: true,
          transaction_amount: true,
          transaction_balance: false,
          balance_alert_condition: true,
          balance_alert_threshold: true,
          balance_alert_balance: true
        };
        [ $topic_a, $topic_b ] as $topics
        | [
            $topics[] as $topic
            | {destination: $topic, scenario: "incoming", event: "transaction", direction: "receive", states: ["pending", "confirmed"], amount_sats: 1000000, content: content},
              {destination: $topic, scenario: "outgoing", event: "transaction", direction: "send", states: ["pending", "confirmed"], amount_sats: 2000000, content: content},
              {destination: $topic, scenario: "rbf", event: "rbf", direction: "send", states: ["original_pending", "replacement_pending", "replacement_confirmed"], amount_sats: 500000, content: content},
              {destination: $topic, scenario: "cpfp", event: "cpfp", direction: "receive+send", states: ["parent_pending", "child_pending", "parent_confirmed", "child_confirmed"], amounts_sats: {parent: 400000, child: 200000}, content: content},
              {destination: $topic, scenario: "balance", event: "balance_alert", direction: null, states: ["triggered"], amount_sats: null, balance: {condition: "below", threshold_sats: $threshold, current_balance_sats: $current_balance}, content: content}
          ] | {stage: $stage, notifications: sort_by(.destination, .scenario)}
    ' >"$LOG_DIR/semantic-manifest-${stage}.json"
}

run_notification_scenarios() {
    local stage="$1"
    local scenario_file="$LOG_DIR/scenario-${stage}.json"
    local incoming outgoing rbf_original rbf_replacement cpfp_parent cpfp_child alert_ids_json

    incoming="$(dev_mempool_delta "${stage}-incoming" 1 miner sending "$TARGET_BTC_WALLET" 0.01)"
    wait_for_transaction_status "$TARGET_WALLET_CHECKSUM" "$incoming" "pending"
    dev_mine "${stage}-incoming"
    wait_for_transaction_status "$TARGET_WALLET_CHECKSUM" "$incoming" "confirmed"

    if [[ "$stage" == "pre" ]]; then
        create_legacy_balance_alert
    fi

    outgoing="$(dev_mempool_delta "${stage}-outgoing" 1 "$TARGET_BTC_WALLET" sending miner 0.02)"
    wait_for_transaction_status "$TARGET_WALLET_CHECKSUM" "$outgoing" "pending"
    dev_mine "${stage}-outgoing"
    wait_for_transaction_status "$TARGET_WALLET_CHECKSUM" "$outgoing" "confirmed"

    rbf_original="$(dev_mempool_delta "${stage}-rbf-original" 1 "$TARGET_BTC_WALLET" sending miner 0.005)"
    wait_for_transaction_status "$TARGET_WALLET_CHECKSUM" "$rbf_original" "pending"
    rbf_replacement="$(dev_rbf_delta "${stage}-rbf-replacement" "$rbf_original")"
    wait_for_transaction_status "$TARGET_WALLET_CHECKSUM" "$rbf_original" "replaced"
    wait_for_transaction_status "$TARGET_WALLET_CHECKSUM" "$rbf_replacement" "pending"
    dev_mine "${stage}-rbf"
    wait_for_transaction_status "$TARGET_WALLET_CHECKSUM" "$rbf_replacement" "confirmed"

    cpfp_parent="$(dev_mempool_delta "${stage}-cpfp-parent" 1 miner sending "$TARGET_BTC_WALLET" 0.004)"
    wait_for_transaction_status "$TARGET_WALLET_CHECKSUM" "$cpfp_parent" "pending"
    cpfp_child="$(dev_mempool_delta "${stage}-cpfp-child" 1 "$TARGET_BTC_WALLET" cpfp "$cpfp_parent")"
    wait_for_transaction_status "$TARGET_WALLET_CHECKSUM" "$cpfp_child" "pending"
    dev_mine "${stage}-cpfp"
    wait_for_transaction_status "$TARGET_WALLET_CHECKSUM" "$cpfp_parent" "confirmed"
    wait_for_transaction_status "$TARGET_WALLET_CHECKSUM" "$cpfp_child" "confirmed"

    if [[ "$stage" == "pre" ]]; then
        alert_ids_json="$(jq -n --arg id "$LEGACY_BALANCE_ALERT_ID" '[$id]')"
    else
        local db_path
        db_path="$(metadata_db_path "$WORKTREE_DIR")"
        alert_ids_json="$(sqlite3 -json "$db_path" \
            "SELECT id FROM balance_alerts
             WHERE wallet_checksum = '$TARGET_WALLET_CHECKSUM'
               AND contact_id IN ('$CONTACT_A_ID', '$CONTACT_B_ID')
               AND threshold_sats = $LEGACY_BALANCE_THRESHOLD_SATS
               AND alert_type = 'below';" | jq '[.[].id]')"
        [[ "$(echo "$alert_ids_json" | jq 'length')" -eq 2 ]] \
            || fail "Expected exactly two migrated per-contact balance alerts"
    fi

    jq -n \
        --arg stage "$stage" \
        --arg incoming "$incoming" \
        --arg outgoing "$outgoing" \
        --arg rbf_original "$rbf_original" \
        --arg rbf_replacement "$rbf_replacement" \
        --arg cpfp_parent "$cpfp_parent" \
        --arg cpfp_child "$cpfp_child" \
        --argjson alert_ids "$alert_ids_json" \
        --argjson threshold "$LEGACY_BALANCE_THRESHOLD_SATS" '
        {
          stage: $stage,
          transactions: {
            incoming: $incoming,
            outgoing: $outgoing,
            rbf_original: $rbf_original,
            rbf_replacement: $rbf_replacement,
            cpfp_parent: $cpfp_parent,
            cpfp_child: $cpfp_child
          },
          transaction_deliveries: [
            {txid: $incoming, count: 2},
            {txid: $outgoing, count: 2},
            {txid: $rbf_original, count: 1},
            {txid: $rbf_replacement, count: 2},
            {txid: $cpfp_parent, count: 2},
            {txid: $cpfp_child, count: 2}
          ],
          alert_ids: $alert_ids,
          threshold_sats: $threshold
        }
    ' >"$scenario_file"

    wait_for_stage_delivery "$scenario_file"
    if [[ "$stage" == "pre" ]]; then
        PRE_UPGRADE_TXID="$incoming"
    fi
}

restore_and_rearm_legacy_alert() {
    local current_balance delta amount restore_txid db_path
    current_balance="$(wallet_balance_sats)"
    delta=$((LEGACY_BALANCE_THRESHOLD_SATS - current_balance))
    [[ "$delta" -gt 0 ]] || fail "Pre-upgrade balance did not fall below the legacy threshold"
    amount="$(LC_NUMERIC=C awk -v sats="$delta" 'BEGIN { printf "%.8f", sats / 100000000 }')"
    restore_txid="$(dev_mempool_delta "pre-balance-restore" 1 miner sending "$TARGET_BTC_WALLET" "$amount")"
    wait_for_transaction_status "$TARGET_WALLET_CHECKSUM" "$restore_txid" "pending"
    dev_mine "pre-balance-restore"
    wait_for_transaction_status "$TARGET_WALLET_CHECKSUM" "$restore_txid" "confirmed"
    [[ "$(wallet_balance_sats)" -eq "$LEGACY_BALANCE_THRESHOLD_SATS" ]] \
        || fail "Could not restore the wallet exactly to the legacy threshold"

    stop_app_processes
    db_path="$(metadata_db_path "$WORKTREE_DIR")"
    sqlite3 "$db_path" \
        "UPDATE balance_alerts
         SET is_active = 1, last_checked_balance_sats = $LEGACY_BALANCE_THRESHOLD_SATS
         WHERE id = '$LEGACY_BALANCE_ALERT_ID';"
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
        NTFY_TOPIC_A="$NTFY_TOPIC_A" \
        NTFY_TOPIC_B="$NTFY_TOPIC_B" \
        NTFY_TOPIC_INACTIVE="$NTFY_TOPIC_INACTIVE" \
        CONTACT_A_NAME="Upgrade Active A" \
        CONTACT_B_NAME="Upgrade Active B" \
        INACTIVE_CONTACT_NAME="Upgrade Inactive" \
        EXPECTED_WALLET_COUNT="$EXPECTED_WALLET_COUNT" \
        TXID_PREFIX="$txid_prefix" \
        AUTH_TOKEN="$AUTH_TOKEN" \
        FRONTEND_URL="$FRONTEND_URL" \
        npx playwright test --grep "$stage_tag"
    )
}

start_backend() {
    local repo_dir="$1"
    local label="${2:-app}"
    (
        cd "$repo_dir/backend"
        cargo run >"$LOG_DIR/backend-${label}.log" 2>&1
    ) &
    BACKEND_PID=$!
    wait_for_backend
    authenticate_if_required
}

start_frontend() {
    local repo_dir="$1"
    local label="${2:-app}"
    (
        cd "$repo_dir/frontend"
        pnpm install --frozen-lockfile >"$LOG_DIR/frontend-install-${label}.log" 2>&1
        mkdir -p logs
        pnpm exec next dev --port "$FRONTEND_PORT" >"$LOG_DIR/frontend-${label}.log" 2>&1
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

log "Preparing ${FROM_TAG} (${SOURCE_SHA}) -> ${TO_REF} (${TARGET_SHA})"
ref_uses_bdk_wallet_2 "$FROM_TAG" \
    || fail "Upgrade source ${FROM_TAG} does not use bdk_wallet 2.x"
git -C "$REPO_ROOT" worktree add --detach "$WORKTREE_DIR" "$SOURCE_SHA" >/dev/null
copy_self_hosted_env "$WORKTREE_DIR"

log "Resetting local ports and regtest infrastructure"
echo "WARNING: this deletes only the Docker volumes declared by scripts/docker-compose.yml." >&2
echo "Never run this gate against non-regtest or user data." >&2
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

log "Starting source version ${FROM_TAG} (${SOURCE_SHA})"
start_backend "$WORKTREE_DIR" "pre"
start_frontend "$WORKTREE_DIR" "pre"

log "Seeding self-hosted data on old version"
seed_old_version "$WORKTREE_DIR"
wait_for_wallets_synced
select_target_wallet
capture_snapshot "$TARGET_WALLET_CHECKSUM"

log "Configuring authenticated ntfy and notification fixtures before source activity"
configure_ntfy_credentials
RUN_ID="$(date +%s)-$$"
NTFY_TOPIC_A="canary-upgrade-${RUN_ID}-a"
NTFY_TOPIC_B="canary-upgrade-${RUN_ID}-b"
NTFY_TOPIC_INACTIVE="canary-upgrade-${RUN_ID}-inactive"
CONTACT_A_ID="$(create_ntfy_contact "Upgrade Active A" "$NTFY_TOPIC_A")"
CONTACT_B_ID="$(create_ntfy_contact "Upgrade Active B" "$NTFY_TOPIC_B")"
seed_inactive_ntfy_contact
assert_contacts_present
capture_database_artifacts "pre-fixture"

log "Running source notification matrix"
PRE_STAGE_START="$(date +%s)"
run_notification_scenarios "pre"
assert_restart_does_not_duplicate "pre"
PRE_STAGE_END="$(date +%s)"
capture_ntfy_stage "pre" "$PRE_STAGE_START" "$PRE_STAGE_END"
validate_stage_artifacts "pre" "$LOG_DIR/scenario-pre.json"

log "Running pre-upgrade Playwright verification"
run_playwright '@pre-upgrade' "$PRE_UPGRADE_TXID"

log "Restoring the balance threshold and rearming its deliverable legacy alert"
sleep 2
restore_and_rearm_legacy_alert
capture_database_artifacts "pre-upgrade"
capture_pre_upgrade_bdk_state
assert_web_ports_available
git -C "$WORKTREE_DIR" checkout --detach "$TARGET_SHA" >"$LOG_DIR/git-checkout-target.log" 2>&1
copy_self_hosted_env "$WORKTREE_DIR"
migrate_legacy_data_dir_if_needed "$WORKTREE_DIR"

log "Starting target version ${TO_REF} (${TARGET_SHA})"
start_backend "$WORKTREE_DIR" "post"
start_frontend "$WORKTREE_DIR" "post"

log "Verifying preserved state after upgrade"
assert_post_upgrade_state
assert_post_upgrade_bdk_state
assert_post_upgrade_content_translation

log "Running post-upgrade Playwright verification"
run_playwright '@post-upgrade' "$PRE_UPGRADE_TXID"

log "Running target notification matrix"
sleep 2
POST_STAGE_START="$(date +%s)"
run_notification_scenarios "post"
assert_restart_does_not_duplicate "post"
POST_STAGE_END="$(date +%s)"
capture_ntfy_stage "post" "$POST_STAGE_START" "$POST_STAGE_END"
validate_stage_artifacts "post" "$LOG_DIR/scenario-post.json"

log "Comparing normalized source and target notification semantics"
jq -S '.notifications' "$LOG_DIR/semantic-manifest-pre.json" >"$LOG_DIR/semantic-pre.normalized.json"
jq -S '.notifications' "$LOG_DIR/semantic-manifest-post.json" >"$LOG_DIR/semantic-post.normalized.json"
cmp -s "$LOG_DIR/semantic-pre.normalized.json" "$LOG_DIR/semantic-post.normalized.json" \
    || fail "Source and target semantic notification manifests differ"
capture_database_artifacts "post-upgrade"

log "Upgrade test passed"
RUN_SUCCEEDED=1
echo "Source:    $FROM_TAG ($SOURCE_SHA)"
echo "Target:    $TO_REF ($TARGET_SHA)"
echo "Scenarios: incoming/outgoing pending+confirmed, RBF, CPFP, balance crossing, fan-out, inactive non-delivery, restart dedup"
echo "Wallet:    $TARGET_WALLET_NAME ($TARGET_WALLET_CHECKSUM)"
if [[ "$KEEP_WORKTREE" -eq 1 ]]; then
    echo "Artifacts: $WORK_DIR"
fi
