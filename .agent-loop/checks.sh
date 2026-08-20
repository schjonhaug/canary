#!/usr/bin/env bash
set -euo pipefail

mode="${1:-quick}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

check_requirements() {
  for cmd in "$@"; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
      echo "Error: required command '$cmd' is not installed or not on PATH." >&2
      exit 1
    fi
  done
}

run_backend() {
  check_requirements cargo
  (
    cd "$REPO_ROOT/backend"
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    # Keep tests single-threaded to avoid shared database and port contention.
    cargo test -- --test-threads=1
  )
}

run_frontend() {
  check_requirements pnpm
  (
    cd "$REPO_ROOT/frontend"
    pnpm run lint
    pnpm test
    NEXT_PUBLIC_CANARY_MODE=cloud pnpm run build
    pnpm run test:csp
  )
}

run_system() {
  echo "Warning: system mode runs Docker-backed system tests and may affect local services." >&2
  check_requirements bash
  (
    cd "$REPO_ROOT/backend"
    ./run-system-tests.sh
  )
}

run_upgrade() {
  echo "Warning: upgrade mode may start/stop services and create temporary upgrade worktrees." >&2
  check_requirements bash
  (
    cd "$REPO_ROOT/scripts"
    ./test-upgrade.sh
  )
}

case "$mode" in
  quick)
    run_backend
    run_frontend
    ;;
  backend) run_backend ;;
  frontend) run_frontend ;;
  system) run_system ;;
  upgrade) run_upgrade ;;
  ci)
    # Alias for quick until CI-specific gates diverge.
    run_backend
    run_frontend
    ;;
  full)
    run_backend
    run_frontend
    run_system
    run_upgrade
    ;;
  *)
    echo "Usage: $0 [quick|backend|frontend|system|upgrade|ci|full]" >&2
    exit 2
    ;;
esac
