#!/usr/bin/env bash
set -euo pipefail

mode="${1:-quick}"

run_backend() {
  cd "backend"
  cargo fmt --check
  cargo clippy -- -D warnings
  cargo test -- --test-threads=1
  cd - >/dev/null
}

run_frontend() {
  cd "frontend"
  pnpm run lint
  pnpm test
  pnpm run build
  cd - >/dev/null
}

run_system() {
  cd backend
  ./run-system-tests.sh
  cd - >/dev/null
}

run_upgrade() {
  cd scripts
  ./test-upgrade.sh
  cd - >/dev/null
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
