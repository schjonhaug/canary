#!/usr/bin/env bash
set -euo pipefail

mode="${1:-quick}"

run_backend() {
  (
    cd "backend"
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test -- --test-threads=1
  )
}

run_frontend() {
  (
    cd "frontend"
    pnpm run lint
    pnpm test
    pnpm run build
  )
}

run_system() {
  echo "Warning: system mode runs Docker-backed system tests and may affect local services." >&2
  (
    cd backend
    ./run-system-tests.sh
  )
}

run_upgrade() {
  echo "Warning: upgrade mode may start/stop services and create temporary upgrade worktrees." >&2
  (
    cd scripts
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
