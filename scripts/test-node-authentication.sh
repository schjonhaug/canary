#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PLAYWRIGHT_DIR="$SCRIPT_DIR/playwright"
PLATFORM="${CANARY_NODE_PLATFORM:-}"
RESULT_FILE="${CANARY_NODE_AUTH_RESULT_FILE:-}"
STARTOS_HOST="${START9_HOST:-}"
UMBREL_HOST="${CANARY_UMBREL_PUBLIC_HOST:-umbrel.local}"
MYNODE_HOST="${MYNODE_HOST:-mynode.local}"

fail() {
    echo "ERROR: $*" >&2
    exit 1
}

require_tool() {
    command -v "$1" >/dev/null 2>&1 || fail "Missing required tool: $1"
}

usage() {
    cat <<'EOF'
Usage: CANARY_NODE_PLATFORM=<startos|umbrel|mynode> \
       CANARY_SELF_HOSTED_ADMIN_PASSWORD=<password> \
       ./scripts/test-node-authentication.sh

Optional environment:
  CANARY_NODE_PUBLIC_URL          Override public URL discovery for Umbrel/myNode.
  CANARY_NODE_RESTART_COMMAND     Override the platform restart command.
  CANARY_NODE_AUTH_RESULT_FILE    Write non-secret JSON evidence to this file.
  CANARY_EXPECTED_CONTACT_NAMES   Comma-separated contact names to verify.
  START9_HOST                     StartOS server URL or hostname.
  CANARY_UMBREL_PUBLIC_HOST       Umbrel browser hostname (default: umbrel.local).
  MYNODE_HOST                     myNode browser hostname (default: mynode.local).
EOF
}

discover_startos_url() {
    local host binding
    host="$STARTOS_HOST"
    [[ -n "$host" ]] || fail "START9_HOST is required for StartOS URL discovery"
    [[ "$host" == http://* || "$host" == https://* ]] || host="https://$host"

    binding="$(start-cli -H "$host" package host canary binding ui-multi list --format json)"
    echo "$binding" | jq -r '
        .["3000"].addresses.available
        | (map(select(.ssl == true and .metadata.kind == "mdns"))[0]
           // map(select(.ssl == true and .metadata.kind == "ipv4" and .metadata.gateway != "lxcbr0" and .metadata.gateway != "lo"))[0])
        | select(. != null)
        | "https://\(.hostname):\(.port)"
    '
}

discover_public_url() {
    if [[ -n "${CANARY_NODE_PUBLIC_URL:-}" ]]; then
        printf '%s\n' "$CANARY_NODE_PUBLIC_URL"
        return
    fi

    case "$PLATFORM" in
        startos) discover_startos_url ;;
        umbrel) printf 'http://%s:3005\n' "$UMBREL_HOST" ;;
        mynode) printf 'http://%s:3005\n' "$MYNODE_HOST" ;;
        *) fail "CANARY_NODE_PLATFORM must be startos, umbrel, or mynode" ;;
    esac
}

restart_node_canary() {
    if [[ -n "${CANARY_NODE_RESTART_COMMAND:-}" ]]; then
        /bin/bash -lc "$CANARY_NODE_RESTART_COMMAND"
        return
    fi

    case "$PLATFORM" in
        startos)
            local host="$STARTOS_HOST"
            [[ "$host" == http://* || "$host" == https://* ]] || host="https://$host"
            start-cli -H "$host" package restart canary
            ;;
        umbrel)
            ssh "${CANARY_UMBREL_SSH_TARGET:-umbrel@$UMBREL_HOST}" \
                '/usr/local/bin/umbreld client apps.restart.mutate --appId canary'
            ;;
        mynode)
            ssh "${MYNODE_SSH_TARGET:-admin@$MYNODE_HOST}" 'sudo systemctl restart canary'
            ;;
    esac
}

wait_for_url() {
    local url="$1"
    local attempts=120
    while (( attempts > 0 )); do
        if curl -kfsS "$url" >/dev/null 2>&1; then
            return
        fi
        sleep 2
        attempts=$((attempts - 1))
    done
    fail "Canary did not become ready at $url"
}

run_browser_stage() {
    local stage="$1"
    local url="$2"
    local mutate="$3"

    (
        cd "$PLAYWRIGHT_DIR"
        CANARY_NODE_URL="$url" \
        CANARY_NODE_STAGE="$stage" \
        CANARY_NODE_MUTATE="$mutate" \
        node node-authentication.mjs
    )
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
    usage
    exit 0
fi

[[ -n "$PLATFORM" ]] || fail "CANARY_NODE_PLATFORM is required"
[[ -n "${CANARY_SELF_HOSTED_ADMIN_PASSWORD:-}" ]] \
    || fail "CANARY_SELF_HOSTED_ADMIN_PASSWORD is required"
require_tool curl
require_tool jq
require_tool node
case "$PLATFORM" in
    startos) require_tool start-cli ;;
    umbrel|mynode) require_tool ssh ;;
    *) fail "CANARY_NODE_PLATFORM must be startos, umbrel, or mynode" ;;
esac
[[ -d "$PLAYWRIGHT_DIR/node_modules/@playwright/test" ]] \
    || fail "Run npm ci and npx playwright install chromium in scripts/playwright first"

before_url="$(discover_public_url)"
[[ -n "$before_url" ]] || fail "Could not discover the $PLATFORM public URL after install/upgrade"
wait_for_url "$before_url"
before_result="$(run_browser_stage after-install "$before_url" 1 | tail -n 1)"
echo "$before_result" | jq -e '.browser_authentication == "passed" and .signed_out == true' >/dev/null

restart_node_canary

after_url="$(discover_public_url)"
[[ -n "$after_url" ]] || fail "Could not rediscover the $PLATFORM public URL after restart"
wait_for_url "$after_url"
after_result="$(run_browser_stage after-restart "$after_url" 0 | tail -n 1)"
echo "$after_result" | jq -e '.browser_authentication == "passed"' >/dev/null

combined_result="$(jq -n \
    --arg platform "$PLATFORM" \
    --argjson after_install "$before_result" \
    --argjson after_restart "$after_result" \
    '{platform: $platform, browser_authentication: "passed", after_install: $after_install, after_restart: $after_restart}')"

if [[ -n "$RESULT_FILE" ]]; then
    mkdir -p "$(dirname "$RESULT_FILE")"
    temp_file="$(mktemp "${RESULT_FILE}.XXXXXX")"
    printf '%s\n' "$combined_result" >"$temp_file"
    mv "$temp_file" "$RESULT_FILE"
fi

printf '%s\n' "$combined_result"
