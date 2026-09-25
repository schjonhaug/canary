#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CANARY_REPO="$(cd "$SCRIPT_DIR/.." && pwd)"
REPOS_DIR="${CANARY_REPOS_DIR:-$(dirname "$CANARY_REPO")}"
UMBREL_APPS_REPO="$REPOS_DIR/umbrel-apps"

DOCKER_USER="schjonhaug"
BACKEND_IMAGE="$DOCKER_USER/canary-backend"
FRONTEND_IMAGE="$DOCKER_USER/canary-frontend"

UMBREL_HOST="umbrel@umbrel.local"
UMBREL_APP_STORE_PATH="~/umbrel/app-stores/getumbrel-umbrel-apps-github-53f74447/canary/"

SKIP_BUILD=false
SKIP_RSYNC=false
RESTORE=false
APP_VERSION=""
TAG_SUFFIX=""

usage() {
  cat <<'EOF'
Usage:
  scripts/test-umbrel-release.sh <version> [options]
  scripts/test-umbrel-release.sh --restore [options]

Build and push temporary Canary Docker images from the local canary repo,
patch the local Umbrel Canary package to those image digests, and rsync it
to a real Umbrel for manual smoke testing.

This script does not commit, tag, create releases, or open PRs.

Options:
  --skip-build             Reuse already-pushed temp image tags and only patch/rsync.
  --skip-rsync             Patch the local Umbrel package but do not sync to Umbrel.
  --restore                Restore the Canary Umbrel app-store package from origin/master.
  --host <ssh-host>        Umbrel SSH host. Default: umbrel@umbrel.local
  --app-store-path <path>  Remote Canary app-store path.
  --app-version <version>  Umbrel manifest version to set. Default: <version>
  --tag-suffix <suffix>    Docker tag suffix. Default: current canary short SHA.
  -h, --help               Show this help.

Examples:
  scripts/test-umbrel-release.sh 1.5.0
  scripts/test-umbrel-release.sh 1.5.0 --skip-build
  scripts/test-umbrel-release.sh --restore
EOF
}

info() {
  printf '\033[0;34m%s\033[0m\n' "info: $*"
}

ok() {
  printf '\033[0;32m%s\033[0m\n' "ok: $*"
}

warn() {
  printf '\033[1;33m%s\033[0m\n' "warn: $*" >&2
}

fail() {
  printf '\033[0;31m%s\033[0m\n' "error: $*" >&2
  exit 1
}

require_tool() {
  command -v "$1" >/dev/null 2>&1 || fail "missing required tool: $1"
}

resolve_digest() {
  local image_ref="$1"
  local digest=""

  for attempt in 1 2 3 4 5; do
    if digest="$(docker buildx imagetools inspect "$image_ref" --format '{{json .Manifest.Digest}}' | tr -d '"')" && [[ -n "$digest" ]]; then
      printf '%s\n' "$digest"
      return 0
    fi

    warn "failed to resolve digest for $image_ref; retrying ($attempt/5)"
    sleep "$((attempt * 3))"
  done

  return 1
}

confirm() {
  local prompt="$1"
  local response
  read -r -p "$prompt [y/N] " response
  [[ "$response" =~ ^[Yy]$ ]]
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --restore)
      RESTORE=true
      shift
      ;;
    --skip-build)
      SKIP_BUILD=true
      shift
      ;;
    --skip-rsync)
      SKIP_RSYNC=true
      shift
      ;;
    --host)
      [[ $# -ge 2 ]] || fail "--host requires a value"
      UMBREL_HOST="$2"
      shift 2
      ;;
    --app-store-path)
      [[ $# -ge 2 ]] || fail "--app-store-path requires a value"
      UMBREL_APP_STORE_PATH="$2"
      shift 2
      ;;
    --app-version)
      [[ $# -ge 2 ]] || fail "--app-version requires a value"
      APP_VERSION="$2"
      shift 2
      ;;
    --tag-suffix)
      [[ $# -ge 2 ]] || fail "--tag-suffix requires a value"
      TAG_SUFFIX="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    -*)
      fail "unknown option: $1"
      ;;
    *)
      [[ -z "${VERSION:-}" ]] || fail "only one version argument is allowed"
      VERSION="$1"
      shift
      ;;
  esac
done

restore_umbrel_package() {
  require_tool git
  require_tool rsync
  [[ -d "$UMBREL_APPS_REPO/.git" ]] || fail "umbrel-apps repo not found at $UMBREL_APPS_REPO"

  info "fetching upstream Umbrel app package"
  git -C "$UMBREL_APPS_REPO" fetch origin master

  local tmp_dir
  tmp_dir="$(mktemp -d)"
  trap 'rm -rf "$tmp_dir"' EXIT

  git -C "$UMBREL_APPS_REPO" archive --format=tar origin/master canary \
    | tar -x -C "$tmp_dir" --strip-components=1

  info "syncing upstream Canary package to $UMBREL_HOST:$UMBREL_APP_STORE_PATH"
  rsync -avz "$tmp_dir/" "$UMBREL_HOST:$UMBREL_APP_STORE_PATH"
  ok "Umbrel Canary app-store package restored from origin/master"
}

if [[ "$RESTORE" == true ]]; then
  restore_umbrel_package
  exit 0
fi

[[ -n "${VERSION:-}" ]] || fail "version is required"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "version must look like X.Y.Z"

APP_VERSION="${APP_VERSION:-$VERSION}"

require_tool git
require_tool docker
require_tool perl
[[ "$SKIP_RSYNC" == true ]] || require_tool rsync

[[ -d "$CANARY_REPO/.git" ]] || fail "canary repo not found at $CANARY_REPO"
[[ -d "$UMBREL_APPS_REPO/.git" ]] || fail "umbrel-apps repo not found at $UMBREL_APPS_REPO"

SHORT_SHA="$(git -C "$CANARY_REPO" rev-parse --short HEAD)"
TAG_SUFFIX="${TAG_SUFFIX:-$SHORT_SHA}"
TEST_TAG="v${VERSION}-test-${TAG_SUFFIX}"

BACKEND_REF="$BACKEND_IMAGE:$TEST_TAG"
FRONTEND_REF="$FRONTEND_IMAGE:$TEST_TAG"

cat <<EOF
Canary Umbrel smoke-test build

  canary repo:       $CANARY_REPO
  umbrel-apps repo:  $UMBREL_APPS_REPO
  canary commit:     $SHORT_SHA
  docker tag:        $TEST_TAG
  app version:       $APP_VERSION
  umbrel target:     $UMBREL_HOST:$UMBREL_APP_STORE_PATH

EOF

if ! confirm "Continue"; then
  echo "Aborted."
  exit 0
fi

if [[ "$SKIP_BUILD" != true ]]; then
  docker buildx version >/dev/null

  if ! docker buildx inspect canary-builder >/dev/null 2>&1; then
    info "creating docker buildx builder canary-builder"
    docker buildx create --name canary-builder --use
  else
    docker buildx use canary-builder
  fi

  info "building and pushing backend image $BACKEND_REF"
  docker buildx build \
    --platform linux/amd64,linux/arm64 \
    --tag "$BACKEND_REF" \
    --push \
    "$CANARY_REPO/backend"

  info "building and pushing frontend image $FRONTEND_REF"
  docker buildx build \
    --platform linux/amd64,linux/arm64 \
    --build-arg NEXT_PUBLIC_CANARY_MODE=self-hosted \
    --build-arg NEXT_PUBLIC_BUILD_COMMIT="$SHORT_SHA" \
    --tag "$FRONTEND_REF" \
    --push \
    "$CANARY_REPO/frontend"
else
  warn "skipping Docker build; expecting existing pushed tag $TEST_TAG"
fi

info "fetching image digests"
BACKEND_DIGEST="$(resolve_digest "$BACKEND_REF")"
FRONTEND_DIGEST="$(resolve_digest "$FRONTEND_REF")"

[[ -n "$BACKEND_DIGEST" ]] || fail "failed to resolve backend digest"
[[ -n "$FRONTEND_DIGEST" ]] || fail "failed to resolve frontend digest"

ok "backend digest: $BACKEND_DIGEST"
ok "frontend digest: $FRONTEND_DIGEST"

info "patching local Umbrel Canary package"
perl -0pi -e "s|image: \Q$BACKEND_IMAGE\E:v[^\\n@]*\\@sha256:[a-f0-9]+|image: $BACKEND_REF\@$BACKEND_DIGEST|" \
  "$UMBREL_APPS_REPO/canary/docker-compose.yml"
perl -0pi -e "s|image: \Q$FRONTEND_IMAGE\E:v[^\\n@]*\\@sha256:[a-f0-9]+|image: $FRONTEND_REF\@$FRONTEND_DIGEST|" \
  "$UMBREL_APPS_REPO/canary/docker-compose.yml"
perl -0pi -e "s|^version: \".*\"|version: \"$APP_VERSION\"|m" \
  "$UMBREL_APPS_REPO/canary/umbrel-app.yml"

info "current local Umbrel package diff"
git -C "$UMBREL_APPS_REPO" diff -- canary/docker-compose.yml canary/umbrel-app.yml

if [[ "$SKIP_RSYNC" != true ]]; then
  info "syncing Canary package to Umbrel"
  rsync -avz "$UMBREL_APPS_REPO/canary/" "$UMBREL_HOST:$UMBREL_APP_STORE_PATH"
  ok "files synced"
else
  warn "skipping rsync"
fi

cat <<EOF

Manual Umbrel smoke test

1. Open: http://umbrel.local/app-store?dialog=updates
2. Update/reinstall Canary.
3. Confirm Umbrel shows the generated app password.
4. Open Canary and sign in with:
   username: admin@local
   password: Umbrel-generated app password
5. Sign out and confirm Canary returns to /sign-in.
6. Restart Canary and confirm the same credentials still work.

Restore upstream app-store package when finished:
  scripts/test-umbrel-release.sh --restore

Temporary Docker tags used:
  $BACKEND_REF
  $FRONTEND_REF
EOF
