#!/bin/bash
set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Script directory and paths. The node distro repos are expected as siblings of
# this Canary checkout unless CANARY_REPOS_DIR points elsewhere.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CANARY_REPO="$(cd "$SCRIPT_DIR/.." && pwd)"
REPOS_DIR="${CANARY_REPOS_DIR:-$(dirname "$CANARY_REPO")}"
# Release state is local-only and must stay outside the public repo.
RELEASE_STATE_ROOT="${CANARY_RELEASE_STATE_DIR:-${XDG_STATE_HOME:-$HOME/.local/state}/canary/release}"
UMBREL_APPS_REPO="$REPOS_DIR/umbrel-apps"
UMBREL_GALLERY_REPO="$REPOS_DIR/umbrel-apps-gallery"
UMBREL_GALLERY_BASE_REPO="getumbrel/umbrel-apps-gallery"
UMBREL_GALLERY_HEAD_REPO="schjonhaug/umbrel-apps-gallery"
UMBREL_HOST="umbrel@umbrel.local"
# App store path (not app-data which contains runtime data/wallets)
UMBREL_APP_STORE_PATH="~/umbrel/app-stores/getumbrel-umbrel-apps-github-53f74447/canary/"

# Docker image names
DOCKER_USER="schjonhaug"
BACKEND_IMAGE="$DOCKER_USER/canary-backend"
FRONTEND_IMAGE="$DOCKER_USER/canary-frontend"

# Start9 paths
CANARY_STARTOS_REPO="$REPOS_DIR/canary-startos"
# start-cli 1.x looks for .startos (signing key and host profiles) in an
# ancestor of the package repo, so the workspace defaults to the repos dir.
START9_WORKSPACE_DIR="${START9_WORKSPACE_DIR:-$REPOS_DIR}"
START9_BASE_REPO="Start9-Community/canary-startos"
START9_HEAD_REPO="schjonhaug/canary-startos"
START9_BASE_REMOTE="start9"
START9_HOST="${START9_HOST:-}"
START9_RELEASE_NOTE_LOCALES=(en_US es_ES de_DE pl_PL fr_FR)
MYNODE_REPO="$REPOS_DIR/mynode"
MYNODE_CANARY_APP_DIR="$MYNODE_REPO/rootfs/standard/usr/share/mynode_apps/canary"
MYNODE_HOST="${MYNODE_HOST:-mynode.local}"
MYNODE_PACKAGE_PATH=""
UMBREL_RELEASE_BRANCH=""
START9_RELEASE_BRANCH=""
MYNODE_RELEASE_BRANCH=""
UMBREL_SOURCE_BRANCH=""
START9_SOURCE_BRANCH=""
MYNODE_SOURCE_BRANCH=""
UMBREL_BRANCH_SOURCE_KIND=""
START9_BRANCH_SOURCE_KIND=""
MYNODE_BRANCH_SOURCE_KIND=""
UMBREL_GALLERY_PR_URL=""

# Flags
DRY_RUN=false
SKIP_DOCKER=false
SKIP_RSYNC=false
SKIP_START9=false
SKIP_MYNODE=false
SKIP_MYNODE_TEST=false
FROM_PHASE=1

# Track if Docker images were pushed (for cleanup on failure)
DOCKER_IMAGES_PUSHED=false

# Release notes
# GITHUB_RELEASE_NOTES is the detailed canonical release body for schjonhaug/canary.
# NODE_DISTRO_RELEASE_NOTES is the short summary reused for node distro app stores and PRs.
RELEASE_NOTES=""
GITHUB_RELEASE_NOTES=""
NODE_DISTRO_RELEASE_NOTES=""

# Phase tracking
PHASE_NAMES=(
    "Canary version bump & screenshots"
    "Docker image build & push"
    "Start9 version bump"
    "Start9 build"
    "Start9 testing"
    "Umbrel app update"
    "Umbrel testing"
    "myNode version bump & build"
    "myNode testing"
    "Start9 PR"
    "Umbrel PR"
    "myNode PR"
    "Tag and GitHub release"
)
PHASE_STATUS=()
for i in "${!PHASE_NAMES[@]}"; do PHASE_STATUS+=("pending"); done

print_phase_list() {
    for i in "${!PHASE_NAMES[@]}"; do
        local num=$((i + 1))
        local name="${PHASE_NAMES[$i]}"
        local status="${PHASE_STATUS[$i]}"

        if [[ "$status" == "done" ]]; then
            echo -e "  ${GREEN}✓${NC} Phase $num: $name"
        elif [[ "$status" == "skipped" ]]; then
            echo -e "  ${YELLOW}−${NC} Phase $num: $name ${YELLOW}(skipped)${NC}"
        else
            echo "  ○ Phase $num: $name"
        fi
    done
}

run_phase() {
    local num=$1
    local func=$2
    if [[ $FROM_PHASE -gt $num ]]; then
        return 0
    fi
    $func
    if [[ "${PHASE_STATUS[$((num - 1))]}" != "skipped" ]]; then
        PHASE_STATUS[$((num - 1))]="done"
    fi
    echo ""
    print_phase_list
}

phase_will_run() {
    local num=$1
    [[ "$FROM_PHASE" -le "$num" ]]
}

needs_umbrel_repo() {
    phase_will_run 1 || { [[ "$SKIP_DOCKER" != true ]] && phase_will_run 2; } || phase_will_run 6 || { [[ "$SKIP_RSYNC" != true ]] && phase_will_run 7; } || phase_will_run 11
}

needs_start9_repo() {
    [[ "$SKIP_START9" != true ]] && { phase_will_run 3 || phase_will_run 4 || phase_will_run 5 || phase_will_run 10; }
}

needs_mynode_repo() {
    [[ "$SKIP_MYNODE" != true ]] && { phase_will_run 8 || phase_will_run 9 || phase_will_run 12; }
}

start9_release_tag() {
    printf 'v%s_0' "$NEW_VERSION"
}

start9_version_file_name() {
    printf 'v%s.0' "$NEW_VERSION"
}

start9_version_var_name() {
    printf 'v_%s_0' "${NEW_VERSION//./_}"
}

start9_uses_current_version_file() {
    local versions_index=${1:-startos/versions/index.ts}

    [[ -f "$versions_index" ]] &&
        grep -Eq "^import \{ current \} from './current'$" "$versions_index" &&
        grep -Eq '^[[:space:]]*current,[[:space:]]*$' "$versions_index"
}

start9_release_version_file() {
    local versions_index=${1:-startos/versions/index.ts}

    if start9_uses_current_version_file "$versions_index"; then
        printf 'startos/versions/current.ts'
    else
        printf 'startos/versions/%s.ts' "$(start9_version_file_name)"
    fi
}

start9_release_version_var() {
    local versions_index=${1:-startos/versions/index.ts}

    if start9_uses_current_version_file "$versions_index"; then
        printf 'current'
    else
        start9_version_var_name
    fi
}

escape_ts_template_literal() {
    # Escape `$` too so text such as PR titles cannot open a `${...}` substitution.
    sed 's/[`\\$]/\\&/g'
}

assert_no_git_operation_in_progress() {
    local repo_dir=$1
    local label=$2
    local git_dir
    git_dir=$(git -C "$repo_dir" rev-parse --absolute-git-dir 2>/dev/null || true)

    if [[ -z "$git_dir" ]]; then
        return
    fi

    if [[ -d "$git_dir/rebase-merge" || -d "$git_dir/rebase-apply" || -f "$git_dir/MERGE_HEAD" || -f "$git_dir/CHERRY_PICK_HEAD" ]]; then
        print_error "$label: git operation in progress; resolve it before running the release script"
        git -C "$repo_dir" status --short --branch
        print_info "Resolve the conflict and run the matching git continue command, or abort the operation manually if you want to retry from a clean state."
        exit 1
    fi
}

remote_url_matches_github_repo() {
    local remote_url=$1
    local github_repo=$2

    [[ "$remote_url" == *"github.com/$github_repo"* || "$remote_url" == *"github.com:$github_repo"* ]]
}

ensure_start9_remote() {
    cd "$CANARY_STARTOS_REPO"
    local remote_url="https://github.com/$START9_BASE_REPO.git"
    local existing_url

    if existing_url=$(git remote get-url "$START9_BASE_REMOTE" 2>/dev/null); then
        if ! remote_url_matches_github_repo "$existing_url" "$START9_BASE_REPO"; then
            print_error "canary-startos: remote $START9_BASE_REMOTE must point to authoritative upstream $START9_BASE_REPO"
            print_info "Current $START9_BASE_REMOTE URL: $existing_url"
            exit 1
        fi
        return 0
    fi

    print_info "Adding Start9 upstream remote: $START9_BASE_REMOTE -> $remote_url"
    git remote add "$START9_BASE_REMOTE" "$remote_url"
}

ensure_start9_head_remote() {
    cd "$CANARY_STARTOS_REPO"
    local origin_url

    if ! origin_url=$(git remote get-url origin 2>/dev/null); then
        print_error "canary-startos: origin remote is missing; it must point to working fork $START9_HEAD_REPO"
        exit 1
    fi

    if ! remote_url_matches_github_repo "$origin_url" "$START9_HEAD_REPO"; then
        print_error "canary-startos: origin must point to working fork $START9_HEAD_REPO"
        print_info "Current origin URL: $origin_url"
        exit 1
    fi
}

detect_open_canary_pr_branch() {
    local repo=$1
    local head_owner=$2
    local label=$3
    local filter="map(select(.headRepositoryOwner.login == \"$head_owner\" and ((.title | ascii_downcase | contains(\"canary\")) or (.headRefName | ascii_downcase | contains(\"canary\")))))"
    local count branch urls

    count=$(gh pr list \
        --repo "$repo" \
        --state open \
        --json headRefName,headRepositoryOwner,title,url \
        --jq "$filter | length" \
        2>/dev/null || echo "0")

    if [[ "$count" -gt 1 ]]; then
        print_error "$label: multiple open Canary PRs found in $repo from $head_owner"
        urls=$(gh pr list \
            --repo "$repo" \
            --state open \
            --json headRefName,headRepositoryOwner,title,url \
            --jq "$filter | .[] | \"  - \" + .headRefName + \": \" + .url" \
            2>/dev/null || true)
        [[ -n "$urls" ]] && echo "$urls"
        print_info "Specify the branch explicitly with the matching --*-branch option."
        exit 1
    fi

    if [[ "$count" == "1" ]]; then
        branch=$(gh pr list \
            --repo "$repo" \
            --state open \
            --json headRefName,headRepositoryOwner,title,url \
            --jq "$filter | .[0].headRefName" \
            2>/dev/null || true)
    else
        branch=""
    fi

    echo "$branch"
}

node_distro_work_branch() {
    printf 'canary-next-version'
}

node_distro_release_branch() {
    printf 'canary-v%s' "$NEW_VERSION"
}

create_release_branch_from_work_branch() {
    local repo_dir=$1
    local remote=$2
    local new_branch=$3
    local label=$4
    local work_branch
    work_branch="$(node_distro_work_branch)"

    if ! git -C "$repo_dir" ls-remote --exit-code --heads "$remote" "$work_branch" >/dev/null 2>&1; then
        return 1
    fi

    print_info "$label: copying $remote/$work_branch to local release branch $new_branch"
    git -C "$repo_dir" fetch "$remote" "$work_branch"
    git -C "$repo_dir" checkout -B "$new_branch" FETCH_HEAD
    print_success "$label: created local release branch $new_branch from $work_branch"
    return 0
}

wait_for_remote_branch() {
    local repo_dir=$1
    local remote=$2
    local branch=$3
    local attempts=20

    while [[ $attempts -gt 0 ]]; do
        if git -C "$repo_dir" ls-remote --exit-code --heads "$remote" "$branch" >/dev/null 2>&1; then
            return 0
        fi
        sleep 2
        attempts=$((attempts - 1))
    done

    return 1
}

rename_fork_branch_to_release_branch() {
    local repo_dir=$1
    local label=$2
    local remote=$3
    local head_repo=$4
    local source_branch=$5
    local release_branch=$6

    if [[ "$source_branch" == "$release_branch" ]]; then
        return 0
    fi

    local source_exists=false
    local release_exists=false
    git -C "$repo_dir" ls-remote --exit-code --heads "$remote" "$source_branch" >/dev/null 2>&1 && source_exists=true
    git -C "$repo_dir" ls-remote --exit-code --heads "$remote" "$release_branch" >/dev/null 2>&1 && release_exists=true

    if [[ "$release_exists" == true ]]; then
        if [[ "$source_exists" != true ]]; then
            print_success "$label: release branch $release_branch already exists"
            return 0
        fi
        print_error "$label: cannot rename $source_branch to $release_branch because $remote/$release_branch already exists"
        print_info "Resolve the branch conflict manually, then rerun the release script."
        exit 1
    fi

    if [[ "$source_exists" != true ]]; then
        print_error "$label: source branch $remote/$source_branch does not exist"
        exit 1
    fi

    print_info "$label: renaming $source_branch to $release_branch in $head_repo"
    gh api \
        -X POST \
        "repos/$head_repo/branches/$source_branch/rename" \
        -f "new_name=$release_branch" \
        >/dev/null

    if ! wait_for_remote_branch "$repo_dir" "$remote" "$release_branch"; then
        print_error "$label: renamed branch $remote/$release_branch did not appear in time"
        exit 1
    fi

    git -C "$repo_dir" fetch "$remote" --prune

    if git -C "$repo_dir" show-ref --verify --quiet "refs/heads/$release_branch"; then
        print_error "$label: local branch $release_branch already exists after remote rename"
        print_info "Delete or rename the local branch manually, then rerun the release script."
        exit 1
    fi

    if git -C "$repo_dir" show-ref --verify --quiet "refs/heads/$source_branch"; then
        local current_branch
        current_branch=$(git -C "$repo_dir" branch --show-current)
        if [[ "$current_branch" == "$source_branch" ]]; then
            git -C "$repo_dir" branch -m "$release_branch"
        else
            git -C "$repo_dir" branch -m "$source_branch" "$release_branch"
        fi
    fi

    print_success "$label: renamed branch to $release_branch"
}

ensure_release_branch_ready() {
    local repo_dir=$1
    local label=$2
    local remote=$3
    local head_repo=$4
    local source_branch=$5
    local release_branch=$6
    local source_kind=$7

    case "$source_kind" in
        open-pr|cli)
            rename_fork_branch_to_release_branch "$repo_dir" "$label" "$remote" "$head_repo" "$source_branch" "$release_branch"
            ;;
        work)
            if ! git -C "$repo_dir" ls-remote --exit-code --heads "$remote" "$release_branch" >/dev/null 2>&1; then
                create_release_branch_from_work_branch "$repo_dir" "$remote" "$release_branch" "$label" || true
            fi
            ;;
        existing|new)
            ;;
        *)
            print_error "$label: unknown branch source kind: $source_kind"
            exit 1
            ;;
    esac
}

cleanup_node_distro_work_branch() {
    local repo_dir=$1
    local label=$2
    local remote=$3
    local head_repo=$4
    local work_branch
    work_branch="$(node_distro_work_branch)"
    local remote_exists=false

    if git -C "$repo_dir" ls-remote --exit-code --heads "$remote" "$work_branch" >/dev/null 2>&1; then
        remote_exists=true
        print_info "$label: retiring stale work branch $work_branch"
        gh api \
            -X DELETE \
            "repos/$head_repo/git/refs/heads/$work_branch" \
            >/dev/null
    else
        print_info "$label: remote $work_branch branch already absent"
    fi

    git -C "$repo_dir" fetch "$remote" --prune >/dev/null 2>&1 || true

    if git -C "$repo_dir" show-ref --verify --quiet "refs/heads/$work_branch"; then
        git -C "$repo_dir" branch -D "$work_branch"
    fi

    if [[ "$remote_exists" == true ]]; then
        print_success "$label: retired $work_branch"
    else
        print_success "$label: cleared local $work_branch ref"
    fi
}

preflight_node_distro_rebase() {
    local repo_dir=$1
    local label=$2
    local source_branch=$3
    local release_branch=$4
    local source_kind=$5
    local head_remote=$6
    local base_remote=$7
    local base_branch="${8:-master}"
    local preflight_branch="${source_branch:-$release_branch}"

    if [[ "$source_kind" == "new" ]]; then
        return 0
    fi

    assert_no_git_operation_in_progress "$repo_dir" "$label"
    git -C "$repo_dir" fetch "$base_remote" "$base_branch"
    git -C "$repo_dir" fetch "$head_remote" "$preflight_branch" 2>/dev/null || true

    local branch_ref=""
    if git -C "$repo_dir" show-ref --verify --quiet "refs/heads/$preflight_branch"; then
        branch_ref="refs/heads/$preflight_branch"
    elif git -C "$repo_dir" show-ref --verify --quiet "refs/remotes/$head_remote/$preflight_branch"; then
        branch_ref="refs/remotes/$head_remote/$preflight_branch"
    else
        print_info "$label: no existing source branch to preflight"
        return 0
    fi

    if ! git -C "$repo_dir" merge-tree --write-tree --quiet "$branch_ref" "$base_remote/$base_branch" >/dev/null; then
        print_error "$label: $preflight_branch will conflict when rebased onto $base_remote/$base_branch"
        print_info "Resolve the branch against upstream before continuing:"
        echo "  cd $repo_dir"
        echo "  git checkout $preflight_branch"
        echo "  git fetch $base_remote $base_branch"
        echo "  git rebase $base_remote/$base_branch"
        exit 1
    fi

    print_success "$label: branch $preflight_branch can rebase cleanly onto $base_remote/$base_branch"
}

preflight_umbrel_rebase() {
    preflight_node_distro_rebase \
        "$UMBREL_APPS_REPO" \
        "umbrel-apps" \
        "$UMBREL_SOURCE_BRANCH" \
        "$UMBREL_RELEASE_BRANCH" \
        "$UMBREL_BRANCH_SOURCE_KIND" \
        "fork" \
        "origin" \
        "master"
}

preflight_start9_rebase() {
    ensure_start9_head_remote
    ensure_start9_remote
    preflight_node_distro_rebase \
        "$CANARY_STARTOS_REPO" \
        "canary-startos" \
        "$START9_SOURCE_BRANCH" \
        "$START9_RELEASE_BRANCH" \
        "$START9_BRANCH_SOURCE_KIND" \
        "origin" \
        "$START9_BASE_REMOTE" \
        "master"
}

preflight_mynode_rebase() {
    preflight_node_distro_rebase \
        "$MYNODE_REPO" \
        "mynode" \
        "$MYNODE_SOURCE_BRANCH" \
        "$MYNODE_RELEASE_BRANCH" \
        "$MYNODE_BRANCH_SOURCE_KIND" \
        "origin" \
        "upstream" \
        "master"
}

mynode_local_update_command() {
    if command -v mynode-local-update >/dev/null 2>&1; then
        command -v mynode-local-update
        return 0
    fi

    return 1
}

mynode_detect_dev_pc_ip() {
    local host="$1"
    local route_interface

    if [[ -n "${MYNODE_DEV_PC_IP:-}" ]]; then
        printf '%s\n' "$MYNODE_DEV_PC_IP"
        return 0
    fi

    if command -v route >/dev/null 2>&1 && command -v ipconfig >/dev/null 2>&1; then
        route_interface=$(route get "$host" 2>/dev/null | awk '/interface:/{print $2; exit}')
        if [[ -n "$route_interface" ]]; then
            ipconfig getifaddr "$route_interface" 2>/dev/null && return 0
        fi
    fi

    if command -v ip >/dev/null 2>&1; then
        ip route get "$host" 2>/dev/null | awk '{for (i=1; i<=NF; i++) if ($i == "src") {print $(i+1); exit}}'
        return 0
    fi

    return 1
}

strip_macos_metadata_from_mynode_rootfs() {
    local tarball rootfs_dir device

    find "$MYNODE_REPO/out" -type f -name '._*' -delete 2>/dev/null || true
    find "$MYNODE_REPO/out" -type f -name '.DS_Store' -delete 2>/dev/null || true

    for tarball in "$MYNODE_REPO"/out/mynode_rootfs_*.tar.gz; do
        [[ -f "$tarball" ]] || continue
        device="$(basename "$tarball")"
        device="${device#mynode_rootfs_}"
        device="${device%.tar.gz}"
        rootfs_dir="$MYNODE_REPO/out/rootfs_$device"

        if [[ -d "$rootfs_dir" ]]; then
            find "$rootfs_dir" -type f -name '._*' -delete 2>/dev/null || true
            find "$rootfs_dir" -type f -name '.DS_Store' -delete 2>/dev/null || true
            print_info "Repacking myNode rootfs tarball without macOS metadata: $(basename "$tarball")"
            (cd "$MYNODE_REPO" && rm -f "$tarball" && COPYFILE_DISABLE=1 tar --no-xattrs -zcf "$tarball" "out/rootfs_$device"/*)
        fi
    done
}

run_mynode_local_update() {
    local host="$1"
    local mode="${2:-www}"
    local update_cmd

    if update_cmd=$(mynode_local_update_command); then
        (cd "$MYNODE_REPO" && "$update_cmd" "$host" "$mode")
        return
    fi

    local dev_pc_ip ssh_target
    dev_pc_ip=$(mynode_detect_dev_pc_ip "$host")
    if [[ -z "$dev_pc_ip" ]]; then
        print_error "Could not determine local IP address for myNode to fetch files from"
        print_info "Set MYNODE_DEV_PC_IP to your Mac's LAN IP, then rerun the release script."
        exit 1
    fi

    ssh_target="${MYNODE_SSH_TARGET:-admin@$host}"

    print_info "mynode-local-update not found; using built-in myNode local update flow"
    print_info "Building myNode rootfs artifacts..."
    (cd "$MYNODE_REPO" && make rootfs)
    strip_macos_metadata_from_mynode_rootfs

    print_info "Starting myNode local file server..."
    (cd "$MYNODE_REPO" && make start_file_server)

    print_info "Running mynode-local-upgrade on $ssh_target using dev PC IP $dev_pc_ip..."
    ssh -t "$ssh_target" "sudo mynode-local-upgrade '$dev_pc_ip' '$mode'"
}

verify_mynode_local_source_sync() {
    local host="$1"
    local ssh_target="${MYNODE_SSH_TARGET:-admin@$host}"
    local local_hashes remote_hashes
    local compose_file="$MYNODE_REPO/rootfs/standard/usr/share/mynode_apps/canary/app_data/docker-compose.yml"
    local install_script="$MYNODE_REPO/rootfs/standard/usr/share/mynode_apps/canary/scripts/install_canary.sh"
    local pre_start_script="$MYNODE_REPO/rootfs/standard/usr/share/mynode_apps/canary/scripts/pre_canary.sh"
    local post_upgrade_script="$MYNODE_REPO/rootfs/standard/usr/bin/mynode_post_upgrade.sh"

    local_hashes=$(printf '%s\n%s\n%s\n%s\n' \
        "$(shasum -a 256 "$compose_file" | awk '{print $1}')" \
        "$(shasum -a 256 "$install_script" | awk '{print $1}')" \
        "$(shasum -a 256 "$pre_start_script" | awk '{print $1}')" \
        "$(shasum -a 256 "$post_upgrade_script" | awk '{print $1}')")

    if ! remote_hashes=$(ssh "$ssh_target" \
        "sha256sum /usr/share/mynode_apps/canary/app_data/docker-compose.yml /usr/share/mynode_apps/canary/scripts/install_canary.sh /usr/share/mynode_apps/canary/scripts/pre_canary.sh /usr/bin/mynode_post_upgrade.sh | awk '{print \$1}'"); then
        return 1
    fi

    [[ "$remote_hashes" == "$local_hashes" ]]
}

recover_mynode_local_update() {
    local host="$1"
    local ssh_target="${MYNODE_SSH_TARGET:-admin@$host}"
    local attempt

    print_warning "Attempting the idempotent myNode post-copy recovery step..."
    ping_host_or_retry "myNode" "$host"

    for attempt in 1 2 3 4 5; do
        print_info "myNode recovery attempt $attempt/5"
        if ssh -t "$ssh_target" \
            "sudo systemctl daemon-reload && { sudo mynode-manage-apps init || true; } && sudo systemctl restart www"; then
            if verify_mynode_local_source_sync "$host"; then
                print_success "myNode post-copy recovery completed and deployed source hashes match"
                return 0
            fi

            print_error "myNode recovery ran, but deployed source hashes do not match the local release branch"
            return 1
        fi

        if [[ "$attempt" -lt 5 ]]; then
            print_warning "myNode recovery attempt failed; retrying in 15 seconds..."
            sleep 15
        fi
    done

    return 1
}

print_mynode_local_update_recovery() {
    local host="$1"
    local ssh_target="${MYNODE_SSH_TARGET:-admin@$host}"

    print_error "myNode local source update did not complete"
    print_info "If the rootfs copy completed before the failure, recover the final myNode init/restart step with:"
    echo "  ssh -tt $ssh_target 'sudo systemctl daemon-reload; sudo mynode-manage-apps init || true; sudo systemctl restart www'"
    echo ""
    print_info "Then open the Canary Marketplace page and install or upgrade Canary:"
    echo "  http://$host/marketplace/canary"
    echo ""
    print_info "Rerun Phase 9 so the exact packaging commit can be tested and recorded:"
    echo "  scripts/release.sh $NEW_VERSION --from-phase=9"
}

require_clean_repo() {
    local label="$1"

    if [[ -n "$(git status --porcelain)" ]]; then
        print_error "$label: git working directory has uncommitted changes"
        git status --short
        print_info "Commit, push, stash, or remove these changes before continuing."
        exit 1
    fi
}

require_clean_repo_at() {
    local repo_dir="$1"
    local label="$2"

    if [[ -n "$(git -C "$repo_dir" status --porcelain)" ]]; then
        print_error "$label: git working directory has uncommitted changes"
        git -C "$repo_dir" status --short
        print_info "Commit, push, stash, or remove these changes before continuing."
        exit 1
    fi

    print_success "$label: git working directory clean"
}

commit_and_push_if_changed() {
    local label="$1"
    local remote="$2"
    local branch="$3"
    local message="$4"
    shift 4

    git add "$@"
    if git diff --cached --quiet; then
        print_info "$label: no changes to commit"
        return
    fi

    print_info "$label: committing release checkpoint"
    git commit -m "$message"

    print_info "$label: pushing $branch to $remote"
    git push --force-with-lease -u "$remote" "$branch"
    print_success "$label: checkpoint committed and pushed"
}

checkout_rebased_branch() {
    local label="$1"
    local branch="$2"
    local head_remote="$3"
    local base_remote="$4"
    local base_branch="${5:-master}"

    require_clean_repo "$label"
    git fetch "$base_remote" "$base_branch"
    git fetch "$head_remote" "$branch" 2>/dev/null || true

    if git show-ref --verify --quiet "refs/heads/$branch"; then
        git checkout "$branch"
    elif git show-ref --verify --quiet "refs/remotes/$head_remote/$branch"; then
        git checkout -b "$branch" "$head_remote/$branch"
    else
        git checkout -b "$branch" "$base_remote/$base_branch"
    fi

    require_clean_repo "$label"
    if git show-ref --verify --quiet "refs/remotes/$head_remote/$branch"; then
        git rebase "$head_remote/$branch"
    fi
    git rebase "$base_remote/$base_branch"
}

resolve_release_branch() {
    local var_name=$1
    local repo=$2
    local repo_dir=$3
    local label=$4
    local head_remote=$5
    local head_repo=$6
    local source_var_name=$7
    local source_kind_var_name=$8
    local release_branch
    local source_branch="${!var_name}"
    local source_kind=""
    local head_owner
    local detected_branch=""
    release_branch="$(node_distro_release_branch)"
    head_owner="${head_repo%%/*}"

    if [[ -n "$source_branch" ]]; then
        source_kind="cli"
        print_info "$label: using branch from CLI as source: $source_branch"
    else
        detected_branch="$(detect_open_canary_pr_branch "$repo" "$head_owner" "$label")"
    fi

    if [[ -z "$source_kind" ]] &&
        git -C "$repo_dir" ls-remote --exit-code --heads "$head_remote" "$release_branch" >/dev/null 2>&1; then
        if [[ -n "$detected_branch" && "$detected_branch" != "$release_branch" ]]; then
            print_error "$label: found open PR branch $detected_branch, but $head_remote/$release_branch already exists"
            print_info "Resolve the branch conflict manually, then rerun the release script."
            exit 1
        fi
        source_branch="$release_branch"
        source_kind="existing"
        print_success "$label: using existing release branch $release_branch"
    elif [[ -z "$source_kind" ]]; then
        if [[ -n "$detected_branch" ]]; then
            source_branch="$detected_branch"
            source_kind="open-pr"
            print_success "$label: detected open PR branch $source_branch"
        elif git -C "$repo_dir" ls-remote --exit-code --heads "$head_remote" "$(node_distro_work_branch)" >/dev/null 2>&1; then
            source_branch="$(node_distro_work_branch)"
            source_kind="work"
            print_success "$label: using unpublished work branch $source_branch as source"
        else
            source_branch="$release_branch"
            source_kind="new"
            print_info "$label: using new release branch $release_branch"
        fi
    fi

    if [[ "$source_branch" != "$release_branch" ]] &&
        git -C "$repo_dir" ls-remote --exit-code --heads "$head_remote" "$release_branch" >/dev/null 2>&1; then
        print_error "$label: cannot use source branch $source_branch because $head_remote/$release_branch already exists"
        print_info "Resolve the branch conflict manually, then rerun the release script."
        exit 1
    fi

    printf -v "$var_name" '%s' "$release_branch"
    printf -v "$source_var_name" '%s' "$source_branch"
    printf -v "$source_kind_var_name" '%s' "$source_kind"
}

resolve_start9_release_branch() {
    resolve_release_branch \
        START9_RELEASE_BRANCH \
        "$START9_BASE_REPO" \
        "$CANARY_STARTOS_REPO" \
        "canary-startos" \
        "origin" \
        "$START9_HEAD_REPO" \
        START9_SOURCE_BRANCH \
        START9_BRANCH_SOURCE_KIND
}

prepare_node_distro_repo() {
    local repo_dir=$1
    local label=$2
    local remote=$3
    local head_repo=$4
    local source_branch=$5
    local release_branch=$6
    local source_kind=$7
    local base_remote=$8
    local base_branch="${9:-master}"

    require_clean_repo "$label"
    ensure_release_branch_ready "$repo_dir" "$label" "$remote" "$head_repo" "$source_branch" "$release_branch" "$source_kind"
    checkout_rebased_branch "$label" "$release_branch" "$remote" "$base_remote" "$base_branch"
}

rewrite_start9_version_graph() {
    local versions_index=$1
    local version_import=$2
    local version_var=$3
    local prev_version_var=$4
    local prev_version_import=$5
    local previous_is_unpublished=$6
    local existing_imports existing_other new_imports new_other

    existing_imports=$(grep -E "^import \{ .* \} from '\./" "$versions_index" || true)
    existing_other=$(grep -oE 'other:[[:space:]]*\[[^]]*\]' "$versions_index" | sed 's/.*\[\(.*\)\].*/\1/' | tr -d ' ')

    new_imports=$(printf '%s\n' "$existing_imports" | grep -v "from '$version_import'" || true)
    if [[ "$previous_is_unpublished" == true ]]; then
        new_imports=$(printf '%s\n' "$new_imports" | grep -v "from '$prev_version_import'" || true)
        new_other=$(printf '%s' "$existing_other" | sed "s/^$prev_version_var,//; s/,$prev_version_var,/,/g; s/,$prev_version_var$//; s/^$prev_version_var$//")
    elif [[ "$prev_version_var" == "$version_var" ]]; then
        new_other="$existing_other"
    elif [[ -n "$existing_other" ]]; then
        new_other="$prev_version_var, $existing_other"
    else
        new_other="$prev_version_var"
    fi

    {
        echo "import { VersionGraph } from '@start9labs/start-sdk'"
        if [[ -n "$new_imports" ]]; then
            printf '%s\n' "$new_imports"
        fi
        echo "import { $version_var } from '$version_import'"
        echo ""
        echo "export const versionGraph = VersionGraph.of({"
        echo "  current: $version_var,"
        echo "  other: [$new_other],"
        echo "})"
    } > "$versions_index"
}

remove_start9_unpublished_version_file_if_needed() {
    local base_ref=$1
    local prev_version_import=$2
    local prev_version_file="$3"

    if [[ -z "$prev_version_import" || -z "$prev_version_file" || ! -f "$prev_version_file" ]]; then
        return 1
    fi

    if git cat-file -e "$base_ref:$prev_version_file" 2>/dev/null; then
        return 1
    fi

    rm -f "$prev_version_file"
    return 0
}

create_start9_version_file() {
    local version_file=$1
    local version_var=$2
    local start9_version=$3

    {
        echo "import { VersionInfo } from '@start9labs/start-sdk'"
        echo ""
        echo "export const $version_var = VersionInfo.of({"
        echo "  version: '$start9_version',"
        echo "  releaseNotes: {"
        local locale
        for locale in "${START9_RELEASE_NOTE_LOCALES[@]}"; do
            echo "    $locale: \`Release notes will be added after testing.\`,"
        done
        echo "  },"
        echo "  migrations: {},"
        echo "})"
    } > "$version_file"
}

create_start9_version_file_with_notes() {
    local version_file=$1
    local version_var=$2
    local localized_notes_file=$3

    validate_start9_localized_release_notes "$localized_notes_file"

    {
        echo "import { VersionInfo } from '@start9labs/start-sdk'"
        echo ""
        echo "export const $version_var = VersionInfo.of({"
        echo "  version: '$NEW_VERSION:0',"
        echo "  releaseNotes: {"
        local locale notes escaped_notes
        for locale in "${START9_RELEASE_NOTE_LOCALES[@]}"; do
            notes=$(jq -r --arg locale "$locale" '.[$locale]' "$localized_notes_file")
            escaped_notes=$(printf '%s' "$notes" | escape_ts_template_literal)
            printf '    %s: `%s`,\n' "$locale" "$escaped_notes"
        done
        echo "  },"
        echo "  migrations: {},"
        echo "})"
    } > "$version_file"
}

prepare_umbrel_repo() {
    cd "$UMBREL_APPS_REPO"
    assert_no_git_operation_in_progress "$UMBREL_APPS_REPO" "umbrel-apps"
    prepare_node_distro_repo \
        "$UMBREL_APPS_REPO" \
        "umbrel-apps" \
        "fork" \
        "schjonhaug/umbrel-apps" \
        "$UMBREL_SOURCE_BRANCH" \
        "$UMBREL_RELEASE_BRANCH" \
        "$UMBREL_BRANCH_SOURCE_KIND" \
        "origin" \
        "master"
    print_success "umbrel-apps: using release branch $UMBREL_RELEASE_BRANCH rebased on origin/master"
}

prepare_start9_repo() {
    cd "$CANARY_STARTOS_REPO"
    assert_no_git_operation_in_progress "$CANARY_STARTOS_REPO" "canary-startos"
    ensure_start9_head_remote
    ensure_start9_remote
    prepare_node_distro_repo \
        "$CANARY_STARTOS_REPO" \
        "canary-startos" \
        "origin" \
        "$START9_HEAD_REPO" \
        "$START9_SOURCE_BRANCH" \
        "$START9_RELEASE_BRANCH" \
        "$START9_BRANCH_SOURCE_KIND" \
        "$START9_BASE_REMOTE" \
        "master"
    print_success "canary-startos: using release branch $START9_RELEASE_BRANCH rebased on $START9_BASE_REMOTE/master"
}

prepare_mynode_repo() {
    cd "$MYNODE_REPO"
    assert_no_git_operation_in_progress "$MYNODE_REPO" "mynode"
    prepare_node_distro_repo \
        "$MYNODE_REPO" \
        "mynode" \
        "origin" \
        "schjonhaug/mynode" \
        "$MYNODE_SOURCE_BRANCH" \
        "$MYNODE_RELEASE_BRANCH" \
        "$MYNODE_BRANCH_SOURCE_KIND" \
        "upstream" \
        "master"
    print_success "mynode: using release branch $MYNODE_RELEASE_BRANCH rebased on upstream/master"
}

# Functions
print_header() {
    echo ""
    echo -e "${BLUE}════════════════════════════════════════════════════════════${NC}"
    echo -e "${BLUE}  $1${NC}"
    echo -e "${BLUE}════════════════════════════════════════════════════════════${NC}"
    echo ""
}

print_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}⚠ $1${NC}"
}

print_error() {
    echo -e "${RED}✗ $1${NC}"
}

print_info() {
    echo -e "${BLUE}ℹ $1${NC}"
}

read_multiline_input() {
    local prompt="$1"
    local value=""
    local line=""

    echo "$prompt" >&2
    echo "End with a single '.' on its own line." >&2
    while IFS= read -r line; do
        if [[ "$line" == "." ]]; then
            break
        fi
        if [[ -z "$value" ]]; then
            value="$line"
        else
            value="${value}"$'\n'"${line}"
        fi
    done
    printf '%s' "$value"
}

release_state_dir() {
    printf '%s/v%s' "$RELEASE_STATE_ROOT" "$NEW_VERSION"
}

release_candidate_file() {
    printf '%s/candidate.txt' "$(release_state_dir)"
}

release_gate_dir() {
    printf '%s/gates' "$(release_state_dir)"
}

release_gate_file() {
    local platform="$1"
    printf '%s/%s.passed' "$(release_gate_dir)" "$platform"
}

required_release_gate_platforms() {
    printf '%s\n' startos umbrel
    if [[ "$SKIP_MYNODE" != true && "$SKIP_MYNODE_TEST" != true ]]; then
        printf '%s\n' mynode
    fi
}

required_release_gate_platforms_csv() {
    required_release_gate_platforms | paste -sd, -
}

browser_auth_result_file() {
    local platform="$1"
    printf '%s/%s-browser-auth.json' "$(release_gate_dir)" "$platform"
}

release_candidate_value() {
    local key="$1"
    local candidate_file
    candidate_file="$(release_candidate_file)"
    sed -n "s/^${key}=//p" "$candidate_file" | head -n 1
}

write_release_candidate_identity() {
    local backend_digest="$1"
    local frontend_digest="$2"
    local state_dir candidate_file temp_file
    state_dir="$(release_state_dir)"
    candidate_file="$(release_candidate_file)"

    mkdir -p "$state_dir" "$(release_gate_dir)"
    temp_file=$(mktemp "$state_dir/candidate.XXXXXX")
    {
        printf 'version=%s\n' "$NEW_VERSION"
        printf 'canary_commit=%s\n' "$(git -C "$CANARY_REPO" rev-parse HEAD)"
        printf 'backend_digest=%s\n' "$backend_digest"
        printf 'frontend_digest=%s\n' "$frontend_digest"
        printf 'required_gates=%s\n' "$(required_release_gate_platforms_csv)"
    } > "$temp_file"

    if [[ -f "$candidate_file" ]] && cmp -s "$candidate_file" "$temp_file"; then
        rm -f "$temp_file"
        print_success "Release candidate identity is unchanged"
        return
    fi

    # Any changed app commit or image digest requires every distro to be retested.
    find "$(release_gate_dir)" -maxdepth 1 -type f \
        \( -name '*.passed' -o -name '*-browser-auth.json' \) -delete
    mv "$temp_file" "$candidate_file"
    print_success "Saved exact release candidate identity and invalidated prior platform gates"
}

ensure_release_candidate_identity() {
    local tag="v$NEW_VERSION"
    local backend_digest frontend_digest

    backend_digest=$(docker buildx imagetools inspect "$BACKEND_IMAGE:$tag" --format '{{json .Manifest.Digest}}' | tr -d '"')
    frontend_digest=$(docker buildx imagetools inspect "$FRONTEND_IMAGE:$tag" --format '{{json .Manifest.Digest}}' | tr -d '"')
    write_release_candidate_identity "$backend_digest" "$frontend_digest"
}

ensure_node_auth_playwright() {
    local playwright_dir="$CANARY_REPO/scripts/playwright"
    if [[ -d "$playwright_dir/node_modules/@playwright/test" ]]; then
        return
    fi

    print_info "Installing isolated Playwright dependencies for node authentication..."
    (
        cd "$playwright_dir"
        npm ci
        npx playwright install chromium
    )
}

run_node_browser_auth_gate() {
    local platform="$1"
    local password_var password dashboard_password result_file start9_host

    case "$platform" in
        startos) password_var="CANARY_STARTOS_ADMIN_PASSWORD" ;;
        umbrel) password_var="CANARY_UMBREL_ADMIN_PASSWORD" ;;
        mynode) password_var="CANARY_MYNODE_ADMIN_PASSWORD" ;;
        *)
            print_error "Unknown node authentication platform: $platform"
            exit 1
            ;;
    esac

    password="${!password_var:-}"
    if [[ -z "$password" ]]; then
        read -s -p "Enter the existing Canary password for $platform: " password
        echo ""
    fi

    if [[ "$platform" == "umbrel" ]]; then
        dashboard_password="${CANARY_UMBREL_DASHBOARD_PASSWORD:-}"
        if [[ -z "$dashboard_password" ]]; then
            read -s -p "Enter the Umbrel dashboard password for the fresh browser gate: " dashboard_password
            echo ""
        fi
        if [[ -z "$dashboard_password" ]]; then
            print_error "An Umbrel dashboard password is required for the fresh browser gate"
            exit 1
        fi
    fi
    if [[ -z "$password" ]]; then
        print_error "A Canary password is required for the $platform browser gate"
        exit 1
    fi

    ensure_node_auth_playwright
    result_file="$(browser_auth_result_file "$platform")"
    start9_host="$(start9_config_url || true)"

    print_info "Running fresh-browser authentication gate on $platform..."
    CANARY_NODE_PLATFORM="$platform" \
    CANARY_SELF_HOSTED_ADMIN_PASSWORD="$password" \
    CANARY_UMBREL_DASHBOARD_PASSWORD="$dashboard_password" \
    CANARY_NODE_AUTH_RESULT_FILE="$result_file" \
    START9_HOST="$start9_host" \
    CANARY_UMBREL_PUBLIC_HOST="$(host_from_ssh_target "$UMBREL_HOST")" \
    CANARY_UMBREL_SSH_TARGET="$UMBREL_HOST" \
    MYNODE_HOST="$MYNODE_HOST" \
    "$CANARY_REPO/scripts/test-node-authentication.sh"
    password=""
    dashboard_password=""

    if ! jq -e --arg platform "$platform" '
        .platform == $platform
        and .browser_authentication == "passed"
        and .after_install.browser_authentication == "passed"
        and .after_restart.browser_authentication == "passed"
    ' "$result_file" >/dev/null; then
        print_error "$platform browser-authentication evidence is invalid"
        exit 1
    fi

    print_success "$platform fresh-browser authentication passed"
}

record_release_gate() {
    local platform="$1"
    local repo_dir="$2"
    local candidate_file gate_file temp_file browser_result browser_result_sha
    candidate_file="$(release_candidate_file)"
    gate_file="$(release_gate_file "$platform")"

    if [[ ! -f "$candidate_file" ]]; then
        ensure_release_candidate_identity
    fi

    browser_result="$(browser_auth_result_file "$platform")"
    if [[ ! -f "$browser_result" ]] ||
       ! jq -e --arg platform "$platform" \
           '.platform == $platform and .browser_authentication == "passed"' \
           "$browser_result" >/dev/null; then
        print_error "Missing valid browser-authentication result for $platform"
        return 1
    fi
    browser_result_sha="$(shasum -a 256 "$browser_result" | awk '{print $1}')"

    mkdir -p "$(release_gate_dir)"
    temp_file=$(mktemp "$(release_gate_dir)/${platform}.XXXXXX")
    {
        cat "$candidate_file"
        printf 'platform=%s\n' "$platform"
        printf 'packaging_commit=%s\n' "$(git -C "$repo_dir" rev-parse HEAD)"
        printf 'browser_authentication=passed\n'
        printf 'browser_auth_result_sha256=%s\n' "$browser_result_sha"
        printf 'browser_url_after_install=%s\n' "$(jq -r '.after_install.public_url' "$browser_result")"
        printf 'browser_url_after_restart=%s\n' "$(jq -r '.after_restart.public_url' "$browser_result")"
    } > "$temp_file"
    mv "$temp_file" "$gate_file"
    print_success "Recorded $platform test pass for the exact release candidate"
}

release_gate_matches_packaging_head() {
    local platform="$1"
    local repo_dir="$2"
    local gate_file tested_packaging_commit current_packaging_commit
    gate_file="$(release_gate_file "$platform")"

    [[ -f "$gate_file" ]] || return 1
    tested_packaging_commit=$(sed -n 's/^packaging_commit=//p' "$gate_file" | head -n 1)
    current_packaging_commit=$(git -C "$repo_dir" rev-parse HEAD)

    [[ -n "$tested_packaging_commit" && "$tested_packaging_commit" == "$current_packaging_commit" ]]
}

release_commit_matches_candidate() {
    local expected_commit="$1"
    local actual_commit="$2"
    local expected_tree actual_tree

    git -C "$CANARY_REPO" merge-base --is-ancestor "$expected_commit" "$actual_commit" || return 1
    expected_tree=$(git -C "$CANARY_REPO" rev-parse "$expected_commit^{tree}") || return 1
    actual_tree=$(git -C "$CANARY_REPO" rev-parse "$actual_commit^{tree}") || return 1
    [[ "$expected_tree" == "$actual_tree" ]]
}

verify_release_gates() {
    local candidate_file tag expected_version expected_commit expected_backend expected_frontend expected_required_gates
    local actual_commit actual_backend actual_frontend platform gate_file
    local repo_dir remote tested_packaging_commit actual_packaging_commit release_branch
    local browser_result expected_browser_sha actual_browser_sha
    candidate_file="$(release_candidate_file)"
    tag="v$NEW_VERSION"

    if [[ ! -f "$candidate_file" ]]; then
        print_error "No exact release candidate identity is recorded for v$NEW_VERSION"
        print_info "Rebuild from Phase 2 before publishing."
        return 1
    fi

    expected_version=$(release_candidate_value version)
    expected_commit=$(release_candidate_value canary_commit)
    expected_backend=$(release_candidate_value backend_digest)
    expected_frontend=$(release_candidate_value frontend_digest)
    expected_required_gates=$(release_candidate_value required_gates)
    actual_commit=$(git -C "$CANARY_REPO" rev-parse HEAD)
    actual_backend=$(docker buildx imagetools inspect "$BACKEND_IMAGE:$tag" --format '{{json .Manifest.Digest}}' | tr -d '"')
    actual_frontend=$(docker buildx imagetools inspect "$FRONTEND_IMAGE:$tag" --format '{{json .Manifest.Digest}}' | tr -d '"')

    if [[ "$expected_version" != "$NEW_VERSION" ||
          "$expected_backend" != "$actual_backend" ||
          "$expected_frontend" != "$actual_frontend" ||
          -z "$expected_required_gates" ]]; then
        print_error "The recorded test candidate does not match the current commit or Docker images"
        print_info "Rebuild from Phase 2 and repeat all platform tests."
        return 1
    fi

    # A merge may change the commit ID without changing the tested source.
    # Preserve the original candidate evidence; require ancestry and exact tree identity.
    if ! release_commit_matches_candidate "$expected_commit" "$actual_commit"; then
        print_error "The current commit does not preserve the exact tested candidate source"
        print_info "Rebuild from Phase 2 and repeat all platform tests."
        return 1
    fi

    for platform in ${expected_required_gates//,/ }; do
        case "$platform" in
            startos|umbrel|mynode) ;;
            *)
                print_error "Invalid required release-gate platform: $platform"
                return 1
                ;;
        esac
        gate_file="$(release_gate_file "$platform")"
        if [[ ! -f "$gate_file" ]] || ! cmp -s "$candidate_file" <(sed '/^platform=/,$d' "$gate_file"); then
            print_error "Missing or stale $platform test gate for the exact v$NEW_VERSION candidate"
            print_info "Complete the $platform test phase before publishing."
            return 1
        fi

        if ! grep -Fqx "platform=$platform" "$gate_file"; then
            print_error "Invalid platform identity in $platform test gate"
            return 1
        fi

        if ! grep -Fqx 'browser_authentication=passed' "$gate_file"; then
            print_error "Missing browser-authentication evidence in $platform test gate"
            return 1
        fi
        browser_result="$(browser_auth_result_file "$platform")"
        expected_browser_sha=$(sed -n 's/^browser_auth_result_sha256=//p' "$gate_file" | head -n 1)
        actual_browser_sha=""
        if [[ -f "$browser_result" ]]; then
            actual_browser_sha="$(shasum -a 256 "$browser_result" | awk '{print $1}')"
        fi
        if [[ -z "$expected_browser_sha" || "$expected_browser_sha" != "$actual_browser_sha" ]]; then
            print_error "$platform browser-authentication result changed after its test pass"
            print_info "Repeat the $platform test phase before publishing."
            return 1
        fi

        case "$platform" in
            startos)
                repo_dir="$CANARY_STARTOS_REPO"
                remote="origin"
                ;;
            umbrel)
                repo_dir="$UMBREL_APPS_REPO"
                remote="fork"
                ;;
            mynode)
                repo_dir="$MYNODE_REPO"
                remote="origin"
                ;;
        esac
        release_branch="$(node_distro_release_branch)"
        tested_packaging_commit=$(sed -n 's/^packaging_commit=//p' "$gate_file" | head -n 1)
        actual_packaging_commit=$(git -C "$repo_dir" ls-remote "$remote" "refs/heads/$release_branch" | awk 'NR == 1 {print $1}')
        if [[ -z "$tested_packaging_commit" || "$tested_packaging_commit" != "$actual_packaging_commit" ]]; then
            print_error "$platform packaging branch changed after its test pass"
            print_info "Retest $platform before publishing."
            return 1
        fi
    done

    print_success "StartOS, Umbrel, and myNode gates match the exact release candidate"
}

app_store_release_notes_file() {
    printf '%s/app-store-release-notes.txt' "$(release_state_dir)"
}

github_release_notes_file() {
    printf '%s/github-release-notes.md' "$(release_state_dir)"
}

node_distro_release_notes_file() {
    printf '%s/node-distro-release-notes.txt' "$(release_state_dir)"
}

node_distro_release_notes_file_for() {
    local distro="$1"
    printf '%s/node-distro-%s-release-notes.txt' "$(release_state_dir)" "$distro"
}

start9_localized_release_notes_file() {
    printf '%s/startos-release-notes.json' "$(release_state_dir)"
}

validate_start9_localized_release_notes() {
    local notes_file="$1"
    if ! command -v jq &> /dev/null; then
        print_error "jq is required to validate localized StartOS release notes"
        return 1
    fi
    if [[ ! -f "$notes_file" ]]; then
        print_error "Localized StartOS release notes not found: $notes_file"
        return 1
    fi
    if ! jq -e 'type == "object" and (keys | sort) == ["de_DE", "en_US", "es_ES", "fr_FR", "pl_PL"] and all(.[]; type == "string")' "$notes_file" >/dev/null 2>&1; then
        print_error "StartOS release notes must contain exactly the five supported locale strings"
        return 1
    fi

    local locale notes
    for locale in "${START9_RELEASE_NOTE_LOCALES[@]}"; do
        notes=$(jq -r --arg locale "$locale" '.[$locale] // empty' "$notes_file" 2>/dev/null || true)
        if [[ -z "${notes//[[:space:]]/}" ]]; then
            print_error "StartOS release notes are missing $locale"
            return 1
        fi
        if [[ "$notes" == *"Release notes will be added after testing."* ]]; then
            print_error "StartOS release notes still contain the placeholder for $locale"
            return 1
        fi
    done
}

write_manual_start9_localized_release_notes() {
    local english_notes="$1"
    local output_file="$2"
    local es_notes de_notes pl_notes fr_notes

    es_notes=$(read_multiline_input "Enter the Spanish (es_ES) StartOS release notes:")
    de_notes=$(read_multiline_input "Enter the German (de_DE) StartOS release notes:")
    pl_notes=$(read_multiline_input "Enter the Polish (pl_PL) StartOS release notes:")
    fr_notes=$(read_multiline_input "Enter the French (fr_FR) StartOS release notes:")

    jq -n \
        --arg en_US "$english_notes" \
        --arg es_ES "$es_notes" \
        --arg de_DE "$de_notes" \
        --arg pl_PL "$pl_notes" \
        --arg fr_FR "$fr_notes" \
        '{en_US: $en_US, es_ES: $es_ES, de_DE: $de_DE, pl_PL: $pl_PL, fr_FR: $fr_FR}' \
        > "$output_file"
}

ensure_start9_localized_release_notes() {
    local english_notes="$1"
    local notes_file
    notes_file=$(start9_localized_release_notes_file)

    if release_notes_file_matches_current_version "$notes_file" &&
        validate_start9_localized_release_notes "$notes_file"; then
        print_success "Loaded localized StartOS release notes from $notes_file"
        return 0
    fi

    if ! command -v jq &> /dev/null; then
        print_error "jq is required to create localized StartOS release notes"
        return 1
    fi

    local generated_file
    generated_file=$(mktemp)
    if command -v codex &> /dev/null; then
        print_info "Translating StartOS release notes with Codex..."
        codex exec -C "$CANARY_REPO" --sandbox read-only --ask-for-approval never --output-last-message "$generated_file" "Translate the following StartOS app-store release notes into Spanish, German, Polish, and French. Return ONLY a valid JSON object with exactly these string keys: en_US, es_ES, de_DE, pl_PL, fr_FR. Preserve product names, version numbers, technical terms, URLs, and line breaks. The en_US value must reproduce the source text exactly. Source text: $english_notes" >/dev/null 2>&1 || true
    fi

    if ! validate_start9_localized_release_notes "$generated_file" ||
        [[ "$(jq -r '.en_US // empty' "$generated_file" 2>/dev/null || true)" != "$english_notes" ]]; then
        print_warning "Localized notes were not generated automatically; manual translations are required"
        write_manual_start9_localized_release_notes "$english_notes" "$generated_file"
    fi

    validate_start9_localized_release_notes "$generated_file"
    if [[ "$(jq -r '.en_US' "$generated_file")" != "$english_notes" ]]; then
        print_error "The localized StartOS notes changed the approved English source text"
        return 1
    fi
    print_info "Localized StartOS release notes:"
    jq -r 'to_entries[] | "\n[\(.key)]\n\(.value)"' "$generated_file"
    if ! confirm "Use these localized StartOS release notes?"; then
        write_manual_start9_localized_release_notes "$english_notes" "$generated_file"
        validate_start9_localized_release_notes "$generated_file"
    fi

    mkdir -p "$(release_state_dir)"
    mv "$generated_file" "$notes_file"
    save_release_notes_version_marker "$notes_file"
    print_success "Saved localized StartOS release notes to $notes_file"
}

release_notes_version_marker() {
    printf '%s.version' "$1"
}

save_release_notes_version_marker() {
    local notes_file="$1"
    printf '%s\n' "$NEW_VERSION" > "$(release_notes_version_marker "$notes_file")"
}

release_notes_file_matches_current_version() {
    local notes_file="$1"
    local marker_file
    marker_file="$(release_notes_version_marker "$notes_file")"

    [[ -f "$notes_file" && -f "$marker_file" ]] || return 1
    [[ "$(cat "$marker_file")" == "$NEW_VERSION" ]]
}

save_app_store_release_notes() {
    local notes="$1"
    [[ -n "$notes" ]] || return 0

    if [[ "$DRY_RUN" == true ]]; then
        print_warning "[DRY RUN] Would save app-store release notes to $(app_store_release_notes_file)"
        return 0
    fi

    mkdir -p "$(release_state_dir)"
    local notes_file
    notes_file="$(app_store_release_notes_file)"
    printf '%s\n' "$notes" > "$notes_file"
    save_release_notes_version_marker "$notes_file"
    print_success "Saved app-store release notes to $notes_file"
}

save_github_release_notes() {
    local notes="$1"
    [[ -n "$notes" ]] || return 0

    if [[ "$DRY_RUN" == true ]]; then
        print_warning "[DRY RUN] Would save GitHub release notes to $(github_release_notes_file)"
        return 0
    fi

    mkdir -p "$(release_state_dir)"
    printf '%s\n' "$notes" > "$(github_release_notes_file)"
    print_success "Saved GitHub release notes to $(github_release_notes_file)"
}


save_node_distro_release_notes() {
    local notes="$1"
    [[ -n "$notes" ]] || return 0

    if [[ "$DRY_RUN" == true ]]; then
        print_warning "[DRY RUN] Would save node distro release notes to $(node_distro_release_notes_file)"
        return 0
    fi

    mkdir -p "$(release_state_dir)"
    local notes_file
    notes_file="$(node_distro_release_notes_file)"
    printf '%s\n' "$notes" > "$notes_file"
    save_release_notes_version_marker "$notes_file"
    print_success "Saved node distro release notes to $notes_file"
}

load_node_distro_release_notes() {
    local notes_file
    notes_file="$(node_distro_release_notes_file)"

    if release_notes_file_matches_current_version "$notes_file"; then
        NODE_DISTRO_RELEASE_NOTES="$(cat "$notes_file")"
        RELEASE_NOTES="$NODE_DISTRO_RELEASE_NOTES"
        print_success "Loaded node distro release notes from $notes_file"
    elif [[ -f "$notes_file" ]]; then
        print_warning "Ignoring unversioned or stale node distro release notes at $notes_file"
    fi
}


save_node_distro_release_notes_for() {
    local distro="$1"
    local notes="$2"
    [[ -n "$notes" ]] || return 0

    if [[ "$DRY_RUN" == true ]]; then
        print_warning "[DRY RUN] Would save $distro release notes to $(node_distro_release_notes_file_for "$distro")"
        return 0
    fi

    mkdir -p "$(release_state_dir)"
    local notes_file
    notes_file="$(node_distro_release_notes_file_for "$distro")"
    printf '%s\n' "$notes" > "$notes_file"
    save_release_notes_version_marker "$notes_file"
    print_success "Saved $distro release notes to $notes_file"
}

load_node_distro_release_notes_for() {
    local distro="$1"
    local notes_file
    notes_file="$(node_distro_release_notes_file_for "$distro")"

    if release_notes_file_matches_current_version "$notes_file"; then
        NODE_DISTRO_RELEASE_NOTES="$(cat "$notes_file")"
        RELEASE_NOTES="$NODE_DISTRO_RELEASE_NOTES"
        print_success "Loaded $distro release notes from $notes_file"
        return 0
    elif [[ -f "$notes_file" ]]; then
        print_warning "Ignoring unversioned or stale $distro release notes at $notes_file"
    fi

    return 1
}

load_app_store_release_notes() {
    local notes_file
    notes_file="$(app_store_release_notes_file)"

    if release_notes_file_matches_current_version "$notes_file"; then
        RELEASE_NOTES="$(cat "$notes_file")"
        print_success "Loaded app-store release notes from $notes_file"
    elif [[ -f "$notes_file" ]]; then
        print_warning "Ignoring unversioned or stale app-store release notes at $notes_file"
    fi
}

extract_umbrel_release_notes() {
    local app_file="$UMBREL_APPS_REPO/canary/umbrel-app.yml"
    [[ -f "$app_file" ]] || return 1

    local umbrel_version
    umbrel_version=$(grep '^version: ' "$app_file" | sed 's/version: "\(.*\)"/\1/' || true)
    if [[ "$umbrel_version" != "$NEW_VERSION" ]]; then
        return 1
    fi

    local notes
    notes=$(awk '
        /^releaseNotes: >-$/ {
            in_notes = 1
            next
        }
        in_notes && /^[^[:space:]]/ {
            exit
        }
        in_notes {
            sub(/^  /, "")
            print
        }
    ' "$app_file" | sed '/^[[:space:]]*$/d')

    # Phase 6 updates the manifest version before Phase 11 replaces the notes.
    # Do not mistake an explicitly versioned release-notes block left over from
    # the previous release for notes belonging to the new version.
    if [[ "$notes" =~ Canary[[:space:]]+v([0-9]+\.[0-9]+\.[0-9]+) ]] &&
        [[ "${BASH_REMATCH[1]}" != "$NEW_VERSION" ]]; then
        return 1
    fi

    printf '%s\n' "$notes"
}

summarize_github_release_for_node_distro() {
    local distro_label="$1"
    local tag="v$NEW_VERSION"
    local release_body=""

    release_body=$(gh release view "$tag" --repo schjonhaug/canary --json body --jq '.body' 2>/dev/null || true)
    [[ -n "$release_body" ]] || return 1

    if command -v codex &> /dev/null; then
        local codex_output
        codex_output=$(mktemp)
        codex exec -C "$CANARY_REPO" --sandbox read-only --ask-for-approval never --output-last-message "$codex_output" "Convert these detailed GitHub release notes into a short $distro_label-specific node-distro summary for reviewers. Focus only on changes relevant to $distro_label packaging, self-hosted use, and local integrations. Do not mention Umbrel, StartOS, or myNode unless it is $distro_label. No markdown headings, no version prefix. Use 1-2 concise plain-text sentences. Release notes: $release_body" >/dev/null 2>&1 || true
        cat "$codex_output" 2>/dev/null || true
        rm -f "$codex_output"
    else
        echo "$release_body" | sed '/^#/d; /^$/d' | head -n 6
    fi
}

summarize_github_release_for_app_store() {
    summarize_github_release_for_node_distro "node distro"
}

recover_app_store_release_notes() {
    if [[ -n "$RELEASE_NOTES" ]]; then
        return 0
    fi

    load_app_store_release_notes
    if [[ -n "$RELEASE_NOTES" ]]; then
        return 0
    fi

    if [[ -d "$UMBREL_APPS_REPO/canary" ]]; then
        local umbrel_notes
        umbrel_notes="$(extract_umbrel_release_notes || true)"
        if [[ -n "$umbrel_notes" ]]; then
            RELEASE_NOTES="$umbrel_notes"
            print_success "Recovered app-store release notes from Umbrel app metadata"
            save_app_store_release_notes "$RELEASE_NOTES"
            return 0
        fi
    fi

    local generated_notes
    generated_notes="$(summarize_github_release_for_app_store || true)"
    if [[ -n "$generated_notes" ]]; then
        RELEASE_NOTES="$generated_notes"
        print_success "Recovered app-store release notes from GitHub release"
        save_app_store_release_notes "$RELEASE_NOTES"
        return 0
    fi

    print_warning "Could not recover app-store release notes automatically"
    RELEASE_NOTES="$(read_multiline_input "Enter release notes for app stores (Umbrel & Start9):")"
    save_app_store_release_notes "$RELEASE_NOTES"
}

confirm() {
    local prompt="$1"
    local default="${2:-y}"

    if [[ "$default" == "y" ]]; then
        prompt="$prompt [Y/n] "
    else
        prompt="$prompt [y/N] "
    fi

    read -p "$prompt" response
    response=${response:-$default}
    [[ "$response" =~ ^[Yy]$ ]]
}

ping_host_or_retry() {
    local label="$1"
    local host="$2"

    while true; do
        print_info "Checking $label reachability: $host"
        if ping -c 1 "$host" >/dev/null 2>&1; then
            print_success "$label is reachable"
            return 0
        fi

        print_warning "$label is not reachable at $host"
        if ! confirm "Retry $label reachability check?" "y"; then
            print_error "Cancelled before $label install/sync"
            exit 1
        fi
    done
}

host_from_ssh_target() {
    local target="$1"
    target="${target#*@}"
    target="${target%%:*}"
    printf '%s' "$target"
}

host_from_url() {
    local url="$1"
    url="${url#http://}"
    url="${url#https://}"
    url="${url%%/*}"
    url="${url%%:*}"
    printf '%s' "$url"
}

start9_config_url_from_file() {
    local config_file="$1"
    [[ -f "$config_file" ]] || return 1

    awk '
        /^[[:space:]]*host:[[:space:]]*https?:\/\// {
            sub(/^[[:space:]]*host:[[:space:]]*/, "")
            print
            exit
        }
        /^[[:space:]]*host:[[:space:]]*$/ {
            in_host = 1
            next
        }
        in_host && /^[^[:space:]#]/ {
            exit
        }
        in_host && /^[[:space:]]+default:[[:space:]]*https?:\/\// {
            sub(/^[[:space:]]*default:[[:space:]]*/, "")
            print
            exit
        }
    ' "$config_file"
}

start9_config_url() {
    if [[ -n "$START9_HOST" ]]; then
        if [[ "$START9_HOST" == http://* ]] || [[ "$START9_HOST" == https://* ]]; then
            printf '%s' "$START9_HOST"
        else
            printf 'https://%s' "$START9_HOST"
        fi
        return 0
    fi

    local workspace_url
    workspace_url="$(start9_config_url_from_file "$START9_WORKSPACE_DIR/.startos/config.yaml" || true)"
    if [[ -n "$workspace_url" ]] && [[ "$workspace_url" != "https://dev-vm.local" ]]; then
        printf '%s' "$workspace_url"
        return 0
    fi

    start9_config_url_from_file "$HOME/.startos/config.yaml"
}

start9_config_host() {
    local host_url
    host_url="$(start9_config_url || true)"
    [[ -n "$host_url" ]] || return 1
    host_from_url "$host_url"
}

set_start9_workspace_default_host() {
    local host_url="$1"
    local config_file="$START9_WORKSPACE_DIR/.startos/config.yaml"
    local temp_file
    temp_file="$(mktemp)"

    if ! awk -v host_url="$host_url" '
        /^[[:space:]]*host:[[:space:]]*$/ {
            in_host = 1
            print
            next
        }
        in_host && /^[[:space:]]+default:[[:space:]]*/ {
            print "  default: " host_url
            updated = 1
            in_host = 0
            next
        }
        { print }
        END {
            if (!updated) exit 1
        }
    ' "$config_file" > "$temp_file"; then
        rm -f "$temp_file"
        print_error "Could not update StartOS workspace host in $config_file"
        exit 1
    fi

    mv "$temp_file" "$config_file"
}

ensure_start9_packaging_workspace() {
    # start-cli 1.x requires every package repo to live below a workspace that
    # supplies its signing key and host profiles. Older CLIs do not expose the
    # initializer and continue to use ~/.startos directly.
    if ! start-cli s9pk init-workspace --help >/dev/null 2>&1; then
        return 0
    fi

    if [[ -f "$START9_WORKSPACE_DIR/.startos/build.key.pem" ]] && [[ -f "$START9_WORKSPACE_DIR/.startos/config.yaml" ]]; then
        return 0
    fi

    local legacy_host_url legacy_key
    legacy_host_url="$(start9_config_url_from_file "$HOME/.startos/config.yaml" || true)"
    legacy_key=""
    if [[ -f "$HOME/.startos/id.key.pem" ]]; then
        legacy_key="$HOME/.startos/id.key.pem"
    elif [[ -f "$HOME/.startos/developer.key.pem" ]]; then
        legacy_key="$HOME/.startos/developer.key.pem"
    fi

    print_info "Initializing the StartOS packaging workspace required by start-cli 1.x..."
    start-cli s9pk init-workspace "$START9_WORKSPACE_DIR"

    if [[ -n "$legacy_key" ]]; then
        cp "$legacy_key" "$START9_WORKSPACE_DIR/.startos/build.key.pem"
        chmod 600 "$START9_WORKSPACE_DIR/.startos/build.key.pem"
        print_success "Reused the existing StartOS signing key"
    fi

    if [[ -n "$legacy_host_url" ]]; then
        set_start9_workspace_default_host "$legacy_host_url"
        print_success "Migrated StartOS host to the workspace: $legacy_host_url"
    fi
}

extract_issue_numbers_from_text() {
    grep -Eo '#[0-9]+' | tr -d '#' || true
}

collect_release_contributors() {
    local prev_tag="$1"
    local tag="$2"
    local owner_login="${3:-schjonhaug}"

    if [[ -z "$prev_tag" ]] || ! command -v gh &> /dev/null || ! command -v jq &> /dev/null; then
        echo "No external contributors detected."
        return
    fi

    local start_date end_date pr_json
    start_date=$(git log -1 --format='%cI' "$prev_tag" 2>/dev/null | cut -dT -f1)
    end_date=$(git log -1 --format='%cI' "$tag" 2>/dev/null | cut -dT -f1)
    start_date=$(date -j -v+1d -f "%Y-%m-%d" "$start_date" "+%Y-%m-%d" 2>/dev/null || echo "$start_date")
    end_date=$(date -j -v+1d -f "%Y-%m-%d" "$end_date" "+%Y-%m-%d" 2>/dev/null || echo "$end_date")

    if [[ -z "$start_date" ]] || [[ -z "$end_date" ]]; then
        echo "No external contributors detected."
        return
    fi

    pr_json=$(gh pr list --repo schjonhaug/canary --state merged --base master --search "merged:$start_date..$end_date" --json number,title,body,author,closingIssuesReferences --limit 200 2>/dev/null || echo "[]")
    issue_json=$(gh issue list --repo schjonhaug/canary --state all --search "updated:$start_date..$end_date" --json number,title,author,state --limit 200 2>/dev/null || echo "[]")

    local pr_contributors=""
    while IFS= read -r login; do
        if [[ -n "$login" ]] && [[ "$login" != "$owner_login" ]]; then
            pr_contributors="${pr_contributors} @${login}"
        fi
    done < <(echo "$pr_json" | jq -r '.[].author.login // empty' | sort -u)

    local referenced_issue_numbers
    referenced_issue_numbers=$(
        {
            echo "$pr_json" | jq -r '.[].closingIssuesReferences[].number // empty'
            echo "$pr_json" | jq -r '.[] | .title, (.body // "")' | extract_issue_numbers_from_text
            git log "$prev_tag".."$tag" --format='%s%n%b' 2>/dev/null | extract_issue_numbers_from_text
        } | sort -nu
    )

    local issue_reporters=""
    while IFS= read -r issue_number; do
        local login
        login=$(gh issue view "$issue_number" --json author --jq '.author.login' 2>/dev/null || true)
        if [[ -n "$login" ]] && [[ "$login" != "$owner_login" ]]; then
            issue_reporters="${issue_reporters} #${issue_number} by @${login};"
        fi
    done <<< "$referenced_issue_numbers"

    local issue_candidates=""
    while IFS=$'\t' read -r number title state login; do
        [[ -n "$number" && -n "$title" && -n "$login" && "$login" != "$owner_login" ]] || continue
        issue_candidates="${issue_candidates} #${number} by @${login} (${state}): ${title};"
    done < <(echo "$issue_json" | jq -r '.[] | [.number, .title, .state, (.author.login // "")] | @tsv')

    pr_contributors=$(echo "$pr_contributors" | tr ' ' '\n' | awk 'NF' | sort -u | paste -sd ', ' -)

    if [[ -z "$pr_contributors" ]] && [[ -z "$issue_reporters" ]] && [[ -z "$issue_candidates" ]]; then
        echo "No external contributors detected."
        return
    fi

    if [[ -n "$pr_contributors" ]]; then
        echo "External PR contributors: $pr_contributors"
    fi
    if [[ -n "$issue_reporters" ]]; then
        echo "External issue reporters for referenced issues: $issue_reporters"
    fi
    if [[ -n "$issue_candidates" ]]; then
        echo "External issue candidates updated during this release window; inspect PRs/issues before acknowledging: $issue_candidates"
    fi
}

get_previous_release_tag_for() {
    local tag="$1"
    local tag_time

    tag_time=$(git log -1 --format='%ct' "$tag" 2>/dev/null || true)
    if [[ -z "$tag_time" ]]; then
        return 1
    fi

    git tag --list 'v[0-9]*.[0-9]*.[0-9]*' \
        --format='%(refname:short) %(creatordate:unix)' |
        awk -v current="$tag" -v current_time="$tag_time" '
            $1 != current && $2 <= current_time { print $2, $1 }
        ' |
        sort -nr |
        awk 'NR == 1 { print $2 }'
}

format_release_reference() {
    local pr_number="$1"
    local issue_numbers="$2"
    local ref="(#$pr_number"

    if [[ -n "$issue_numbers" ]]; then
        ref="${ref}; fixes ${issue_numbers}"
    fi

    printf '%s)' "$ref"
}

generate_basic_github_release_notes() {
    local prev_tag="$1"
    local tag="$2"
    local owner_login="${3:-schjonhaug}"

    if [[ -z "$prev_tag" ]] || ! command -v gh &> /dev/null || ! command -v jq &> /dev/null; then
        return 1
    fi

    local start_date end_date pr_json issue_json
    start_date=$(git log -1 --format='%cI' "$prev_tag" 2>/dev/null | cut -dT -f1)
    end_date=$(git log -1 --format='%cI' "$tag" 2>/dev/null | cut -dT -f1)
    start_date=$(date -j -v+1d -f "%Y-%m-%d" "$start_date" "+%Y-%m-%d" 2>/dev/null || echo "$start_date")
    end_date=$(date -j -v+1d -f "%Y-%m-%d" "$end_date" "+%Y-%m-%d" 2>/dev/null || echo "$end_date")

    if [[ -z "$start_date" ]] || [[ -z "$end_date" ]]; then
        return 1
    fi

    pr_json=$(gh pr list --repo schjonhaug/canary --state merged --base master --search "merged:$start_date..$end_date" --json number,title,body,closingIssuesReferences --limit 200 2>/dev/null || echo "[]")
    if [[ "$(echo "$pr_json" | jq 'length')" == "0" ]]; then
        return 1
    fi

    local new_items="" improvement_items="" fix_items=""
    local included_pr_numbers=""
    while IFS=$'\t' read -r number title issues; do
        [[ -n "$number" && -n "$title" ]] || continue

        local issue_refs=""
        if [[ -n "$issues" ]]; then
            issue_refs=$(printf '%s\n' "$issues" | tr ',' '\n' | awk 'NF { print "#" $1 }' | paste -sd ', ' -)
        fi

        local item="- ${title} $(format_release_reference "$number" "$issue_refs")"
        local lower_title
        lower_title=$(printf '%s' "$title" | tr '[:upper:]' '[:lower:]')

        case "$lower_title" in
            chore:*|chore\(*|build:*|build\(*|ci:*|test:*|docs:*|fix\(deps\):*|\
            bump\ version*|fix\ v*.version\ metadata*|refresh\ readme\ screenshots*|\
            *"[skip ci]"*|*pnpm*|*docker\ deploy*|*metadata*|*landing\ link*|\
            *landing\ page*|*public\ page*|*homepage*|*demo\ banner*|*agent\ loop*|\
            *stripe*|*cloud\ billing*|*btcpay*)
                continue
                ;;
        esac

        included_pr_numbers="${included_pr_numbers}${number}"$'\n'

        if [[ "$lower_title" == fix* || "$lower_title" == *" fix "* || "$lower_title" == *"bug"* || "$lower_title" == *"stuck"* ]]; then
            fix_items="${fix_items}${item}"$'\n'
        elif [[ "$lower_title" == improve* || "$lower_title" == redesign* || "$lower_title" == move* || "$lower_title" == show* || "$lower_title" == avoid* ]]; then
            improvement_items="${improvement_items}${item}"$'\n'
        else
            new_items="${new_items}${item}"$'\n'
        fi
    done < <(echo "$pr_json" | jq -r '.[] | [.number, .title, ([.closingIssuesReferences[].number] | join(","))] | @tsv')

    if [[ -z "$new_items" && -z "$improvement_items" && -z "$fix_items" ]]; then
        return 1
    fi

    if [[ -n "$new_items" ]]; then
        printf "## What's New\n\n%s\n" "$new_items"
    fi
    if [[ -n "$improvement_items" ]]; then
        printf "## Improvements\n\n%s\n" "$improvement_items"
    fi
    if [[ -n "$fix_items" ]]; then
        printf "## Bug Fixes\n\n%s\n" "$fix_items"
    fi

    printf "## Contributors\n\n"
    local referenced_issue_numbers
    referenced_issue_numbers=$(
        while IFS= read -r included_pr_number; do
            [[ -n "$included_pr_number" ]] || continue
            echo "$pr_json" | jq -r --argjson number "$included_pr_number" \
                '.[] | select(.number == $number) | .closingIssuesReferences[].number // empty'
            echo "$pr_json" | jq -r --argjson number "$included_pr_number" \
                '.[] | select(.number == $number) | .title, (.body // "")' | extract_issue_numbers_from_text
        done <<< "$included_pr_numbers" | sort -nu
    )

    local contributor_lines=""
    local seen_contributors=""
    while IFS= read -r issue_number; do
        [[ -n "$issue_number" ]] || continue

        local login
        login=$(gh issue view "$issue_number" --json author --jq '.author.login' 2>/dev/null || true)
        [[ -n "$login" && "$login" != "$owner_login" ]] || continue
        [[ "$login" != *"[bot]" && "$login" != app/* && "$login" != "dependabot" ]] || continue

        local key="|${login}:${issue_number}|"
        [[ "$seen_contributors" != *"$key"* ]] || continue
        seen_contributors="${seen_contributors}${key}"
        contributor_lines="${contributor_lines}- Thanks @${login} for reporting the issue fixed in #${issue_number}."$'\n'
    done <<< "$referenced_issue_numbers"

    if [[ -n "$contributor_lines" ]]; then
        printf "%s\n" "$contributor_lines"
    else
        printf -- "- No external contributors in this release.\n\n"
    fi

    printf "**Full Changelog**: https://github.com/schjonhaug/canary/compare/%s...%s\n" "$prev_tag" "$tag"
}

release_notes_require_manual_review() {
    local body="$1"

    [[ -n "$body" ]] || return 0
    [[ "$body" != *"@schjonhaug"* ]] || return 0
    [[ "$body" != *"## What's Changed"* ]] || return 0
    [[ "$body" != *"@app/"* ]] || return 0
    [[ "$body" != *$'\n- chore:'* && "$body" != *$'\n- chore('* ]] || return 0
    [[ "$body" != *$'\n- build:'* && "$body" != *$'\n- build('* ]] || return 0
    [[ "$body" != *$'\n- ci:'* && "$body" != *$'\n- test:'* ]] || return 0

    return 1
}

wait_for_confirmation() {
    local message="$1"
    echo ""
    echo -e "${YELLOW}$message${NC}"
    read -p "Press Enter to continue..."
}

get_current_version() {
    local package_version
    local tag_version

    package_version=$(get_backend_package_version)
    tag_version=$(get_latest_tag_version || true)

    if [[ -n "$tag_version" ]] && version_gte "$tag_version" "$package_version"; then
        printf '%s\n' "$tag_version"
    else
        printf '%s\n' "$package_version"
    fi
}

get_backend_package_version() {
    grep '^version = ' "$CANARY_REPO/backend/Cargo.toml" | sed 's/version = "\(.*\)"/\1/'
}

get_frontend_package_version() {
    sed -n 's/.*"version": "\(.*\)".*/\1/p' "$CANARY_REPO/frontend/package.json" | head -1
}

get_backend_package_version_at_ref() {
    local ref=$1
    git -C "$CANARY_REPO" show "$ref:backend/Cargo.toml" | sed -n 's/^version = "\(.*\)"/\1/p' | head -1
}

get_frontend_package_version_at_ref() {
    local ref=$1
    git -C "$CANARY_REPO" show "$ref:frontend/package.json" | sed -n 's/.*"version": "\(.*\)".*/\1/p' | head -1
}

get_latest_tag_version() {
    git -C "$CANARY_REPO" fetch --tags origin >/dev/null 2>&1 || true
    git -C "$CANARY_REPO" tag --sort=-v:refname --list 'v[0-9]*.[0-9]*.[0-9]*' | head -1 | sed 's/^v//'
}

validate_canary_version_state() {
    local backend_version
    local frontend_version
    local latest_tag_version
    backend_version=$(get_backend_package_version)
    frontend_version=$(get_frontend_package_version)
    latest_tag_version=$(get_latest_tag_version || true)

    if [[ -z "$backend_version" || -z "$frontend_version" ]]; then
        print_error "canary: could not read backend/frontend package versions"
        return 1
    fi

    if [[ "$backend_version" != "$frontend_version" ]]; then
        print_error "canary: backend/frontend package versions differ"
        print_info "backend/Cargo.toml: $backend_version"
        print_info "frontend/package.json: $frontend_version"
        return 1
    fi

    if [[ "$FROM_PHASE" -le 1 && -n "$latest_tag_version" && "$backend_version" != "$latest_tag_version" ]]; then
        print_error "canary: package metadata version does not match latest release tag"
        print_info "backend/Cargo.toml: $backend_version"
        print_info "frontend/package.json: $frontend_version"
        print_info "latest tag: v$latest_tag_version"
        print_info "Fix or investigate the canary repo version metadata before starting a new release."
        return 1
    fi

    print_success "canary: package metadata version verified ($backend_version)"
    return 0
}

version_gte() {
    local left="$1"
    local right="$2"
    local left_major left_minor left_patch
    local right_major right_minor right_patch

    IFS='.' read -r left_major left_minor left_patch <<< "$left"
    IFS='.' read -r right_major right_minor right_patch <<< "$right"

    if (( left_major != right_major )); then
        (( left_major > right_major ))
    elif (( left_minor != right_minor )); then
        (( left_minor > right_minor ))
    else
        (( left_patch >= right_patch ))
    fi
}

increment_version() {
    local version="$1"
    local type="$2"

    IFS='.' read -r major minor patch <<< "$version"

    case "$type" in
        major)
            echo "$((major + 1)).0.0"
            ;;
        minor)
            echo "$major.$((minor + 1)).0"
            ;;
        patch)
            echo "$major.$minor.$((patch + 1))"
            ;;
        *)
            echo "$version"
            ;;
    esac
}

validate_version() {
    local version="$1"
    if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
        print_error "Invalid version format: $version"
        print_info "Expected format: X.Y.Z (e.g., 1.2.3)"
        exit 1
    fi
}

check_prerequisites() {
    print_header "Checking Prerequisites"

    local failed=false
    local needs_umbrel=false
    local needs_start9=false
    local needs_mynode=false

    needs_umbrel_repo && needs_umbrel=true
    needs_start9_repo && needs_start9=true
    needs_mynode_repo && needs_mynode=true

    # Check docker
    if command -v docker &> /dev/null; then
        print_success "docker installed"
    else
        print_error "docker not found"
        failed=true
    fi

    # Check docker buildx
    if docker buildx version &> /dev/null; then
        print_success "docker buildx available"
    else
        print_error "docker buildx not available"
        failed=true
    fi

    # Check Docker Hub authentication (by checking if we can inspect our own image)
    if docker buildx imagetools inspect "$BACKEND_IMAGE:v1.0.1" &> /dev/null; then
        print_success "Docker Hub access verified for $DOCKER_USER"
    else
        print_error "Docker Hub not authenticated or no push access (run: docker login)"
        failed=true
    fi

    # Check gh CLI
    if command -v gh &> /dev/null; then
        print_success "gh CLI installed"
        if gh auth status &> /dev/null; then
            print_success "gh CLI authenticated"
        else
            print_error "gh CLI not authenticated (run: gh auth login)"
            failed=true
        fi
    else
        print_error "gh CLI not found"
        failed=true
    fi

    # Check pnpm
    if command -v pnpm &> /dev/null; then
        print_success "pnpm installed"
    else
        print_error "pnpm not found"
        failed=true
    fi

    # Check jq
    if command -v jq &> /dev/null; then
        print_success "jq installed"
    else
        print_warning "jq not found (release contributor detection will use basic format)"
    fi

    # Check Codex CLI
    if command -v codex &> /dev/null; then
        print_success "codex CLI installed"
    else
        print_warning "codex CLI not found (release notes will use basic format)"
    fi

    # Check repos exist
    if [[ -d "$CANARY_REPO/backend" ]]; then
        print_success "canary repo found"
    else
        print_error "canary repo not found at $CANARY_REPO"
        failed=true
    fi

    if [[ "$needs_umbrel" == true ]]; then
        if [[ -d "$UMBREL_APPS_REPO/canary" ]]; then
            print_success "umbrel-apps repo found"
        else
            print_error "umbrel-apps repo not found at $UMBREL_APPS_REPO"
            failed=true
        fi
    fi

    # Check Start9 repo (if a remaining phase needs it)
    if [[ "$needs_start9" == true ]]; then
        if [[ -d "$CANARY_STARTOS_REPO" ]]; then
            print_success "canary-startos repo found"
        else
            print_error "canary-startos repo not found at $CANARY_STARTOS_REPO"
            failed=true
        fi

        # Check start-cli (required for Start9 0.4.0)
        if command -v start-cli &> /dev/null; then
            print_success "start-cli installed"
        else
            print_error "start-cli not found (required for Start9 0.4.0 builds)"
            print_info "Download from: https://github.com/Start9Labs/start-os/releases"
            failed=true
        fi

        # Check npm (required for Start9 0.4.0)
        if command -v npm &> /dev/null; then
            print_success "npm installed"
        else
            print_error "npm not found (required for Start9 0.4.0 builds)"
            failed=true
        fi

        # Check mksquashfs (required for Start9 0.4.0)
        if command -v mksquashfs &> /dev/null; then
            print_success "mksquashfs installed"
        else
            print_error "mksquashfs not found (install: brew install squashfs)"
            failed=true
        fi
    fi

    # Check myNode repo (if a remaining phase needs it)
    if [[ "$needs_mynode" == true ]]; then
        if [[ -d "$MYNODE_CANARY_APP_DIR" ]]; then
            print_success "mynode repo Canary app found"
        else
            print_error "mynode Canary app not found at $MYNODE_CANARY_APP_DIR"
            failed=true
        fi

        if command -v mynode-sdk &> /dev/null; then
            print_success "mynode-sdk installed"
        else
            print_error "mynode-sdk not found (install: pip3 install mynodesdk)"
            failed=true
        fi

        local mynode_update_cmd
        if mynode_update_cmd=$(mynode_local_update_command); then
            print_success "mynode-local-update available: $mynode_update_cmd"
        else
            print_info "mynode-local-update not found; release.sh will use built-in myNode local update flow"
            if command -v make &> /dev/null; then
                print_success "make installed"
            else
                print_error "make not found (required for built-in myNode local update flow)"
                failed=true
            fi
            if command -v ssh &> /dev/null; then
                print_success "ssh installed"
            else
                print_error "ssh not found (required for built-in myNode local update flow)"
                failed=true
            fi
        fi
    fi

    # Check git status
    cd "$CANARY_REPO"
    assert_no_git_operation_in_progress "$CANARY_REPO" "canary"
    if [[ -z "$(git status --porcelain)" ]]; then
        print_success "canary: git working directory clean"
    else
        print_warning "canary: git working directory has uncommitted changes"
        git status --short
        if ! confirm "Continue anyway?"; then
            exit 1
        fi
    fi

    # Pull latest changes from all repos
    print_info "Pulling latest changes from remotes..."

    cd "$CANARY_REPO"
    git pull --rebase origin master 2>/dev/null || git pull --rebase origin main 2>/dev/null || true
    print_success "canary: up to date"
    if ! validate_canary_version_state; then
        failed=true
    fi

    if [[ "$needs_umbrel" == true ]]; then
        assert_no_git_operation_in_progress "$UMBREL_APPS_REPO" "umbrel-apps"
        require_clean_repo_at "$UMBREL_APPS_REPO" "umbrel-apps"
        resolve_release_branch \
            UMBREL_RELEASE_BRANCH \
            "getumbrel/umbrel-apps" \
            "$UMBREL_APPS_REPO" \
            "umbrel-apps" \
            "fork" \
            "schjonhaug/umbrel-apps" \
            UMBREL_SOURCE_BRANCH \
            UMBREL_BRANCH_SOURCE_KIND
        preflight_umbrel_rebase
    fi
    if [[ "$needs_start9" == true ]]; then
        assert_no_git_operation_in_progress "$CANARY_STARTOS_REPO" "canary-startos"
        require_clean_repo_at "$CANARY_STARTOS_REPO" "canary-startos"
        ensure_start9_head_remote
        ensure_start9_remote
        resolve_start9_release_branch
        preflight_start9_rebase
    fi
    if [[ "$needs_mynode" == true ]]; then
        assert_no_git_operation_in_progress "$MYNODE_REPO" "mynode"
        require_clean_repo_at "$MYNODE_REPO" "mynode"
        resolve_release_branch \
            MYNODE_RELEASE_BRANCH \
            "mynodebtc/mynode" \
            "$MYNODE_REPO" \
            "mynode" \
            "origin" \
            "schjonhaug/mynode" \
            MYNODE_SOURCE_BRANCH \
            MYNODE_BRANCH_SOURCE_KIND
        preflight_mynode_rebase
    fi

    if [[ "$failed" == true ]]; then
        print_error "Prerequisites check failed"
        exit 1
    fi

    print_success "All prerequisites satisfied"
}

# Phase 1: Version Bump
refresh_release_screenshots() {
    local frontend_status
    frontend_status=$(curl -sS -o /dev/null --connect-timeout 3 --max-time 10 -w '%{http_code}' http://localhost:3001/ || true)
    if [[ "$frontend_status" == "000" ]]; then
        print_error "The local Canary frontend is not reachable at http://localhost:3001"
        print_info "Start the current self-hosted backend and frontend before releasing:"
        echo "  cd $CANARY_REPO/backend && cargo run"
        echo "  cd $CANARY_REPO/frontend && pnpm dev"
        print_info "The screenshot script will prepare the isolated regtest fixture automatically."
        exit 1
    fi

    print_info "Refreshing README, myNode-source, and Umbrel gallery screenshots..."
    "$CANARY_REPO/scripts/update-readme-screenshots.sh"

    local screenshot
    for screenshot in \
        "$CANARY_REPO"/screenshots/screenshot-{00,01,02,03,04,05}.png \
        "$CANARY_REPO"/screenshots/umbrel/{1,2,3}.jpg; do
        if [[ ! -s "$screenshot" ]]; then
            print_error "Screenshot generation did not produce $screenshot"
            exit 1
        fi
    done

    print_success "Refreshed six raw screenshots and three Umbrel gallery cards"
    if command -v open >/dev/null 2>&1; then
        open "$CANARY_REPO/screenshots" "$CANARY_REPO/screenshots/umbrel"
    fi
    if ! confirm "Do all refreshed screenshots show the correct v$NEW_VERSION UI?" "n"; then
        print_error "Screenshot review was not approved"
        print_info "Adjust the app or screenshot fixture, then rerun Phase 1."
        exit 1
    fi
}

phase_version_bump() {
    print_header "Phase 1: Canary Version Bump & Screenshots"

    local current_version=$(get_current_version)
    print_info "Current version: $current_version"
    print_info "New version: $NEW_VERSION"

    if [[ "$DRY_RUN" == true ]]; then
        print_warning "[DRY RUN] Would update version to $NEW_VERSION and refresh release screenshots"
        return
    fi

    cd "$CANARY_REPO"

    # Update backend/Cargo.toml
    print_info "Updating backend/Cargo.toml..."
    sed -i '' "s/^version = \".*\"/version = \"$NEW_VERSION\"/" backend/Cargo.toml
    print_success "Updated backend/Cargo.toml"

    # Update frontend/package.json
    print_info "Updating frontend/package.json..."
    sed -i '' "s/\"version\": \".*\"/\"version\": \"$NEW_VERSION\"/" frontend/package.json
    print_success "Updated frontend/package.json"

    # Regenerate lock files
    print_info "Regenerating Cargo.lock..."
    cd "$CANARY_REPO/backend"
    cargo check --quiet
    print_success "Updated Cargo.lock"

    print_info "Regenerating pnpm-lock.yaml..."
    cd "$CANARY_REPO/frontend"
    pnpm install --silent
    print_success "Updated pnpm-lock.yaml"

    cd "$CANARY_REPO"
    refresh_release_screenshots

    # Commit changes (local only - push happens after smoke testing)
    cd "$CANARY_REPO"
    print_info "Committing version bump (local only)..."
    git add \
        backend/Cargo.toml \
        backend/Cargo.lock \
        frontend/package.json \
        frontend/pnpm-lock.yaml \
        screenshots/screenshot-{00,01,02,03,04,05}.png \
        screenshots/umbrel/{1,2,3}.jpg
    git commit -m "Bump version to $NEW_VERSION [skip ci]"
    print_success "Committed version bump locally"

    print_info "Running the complete local upgrade and browser-authentication gate..."
    "$CANARY_REPO/.agent-loop/checks.sh" upgrade

    print_success "Phase 1 complete: Version bumped to $NEW_VERSION (not pushed yet)"
}

# Phase 2: Docker Build & Push
phase_docker_build() {
    print_header "Phase 2: Docker Image Build & Push"

    if [[ "$SKIP_DOCKER" == true ]]; then
        print_warning "Skipping Docker build (--skip-docker)"
        return
    fi

    local tag="v$NEW_VERSION"

    if [[ "$DRY_RUN" == true ]]; then
        print_warning "[DRY RUN] Would build and push:"
        print_info "  $BACKEND_IMAGE:$tag"
        print_info "  $FRONTEND_IMAGE:$tag"
        return
    fi

    # Ensure buildx builder exists
    if ! docker buildx inspect canary-builder &> /dev/null; then
        print_info "Creating buildx builder..."
        docker buildx create --name canary-builder --use
    else
        docker buildx use canary-builder
    fi

    # Build and push backend
    print_info "Building and pushing backend image..."
    cd "$CANARY_REPO/backend"
    docker buildx build \
        --platform linux/amd64,linux/arm64 \
        --tag "$BACKEND_IMAGE:$tag" \
        --push \
        .
    print_success "Backend image pushed: $BACKEND_IMAGE:$tag"

    # Build and push frontend (with self-hosted mode for Umbrel)
    print_info "Building and pushing frontend image..."
    cd "$CANARY_REPO/frontend"
    docker buildx build \
        --platform linux/amd64,linux/arm64 \
        --build-arg NEXT_PUBLIC_CANARY_MODE=self-hosted \
        --tag "$FRONTEND_IMAGE:$tag" \
        --push \
        .
    print_success "Frontend image pushed: $FRONTEND_IMAGE:$tag"

    # Get SHA256 digests
    print_info "Fetching image digests..."
    BACKEND_DIGEST=$(docker buildx imagetools inspect "$BACKEND_IMAGE:$tag" --format '{{json .Manifest.Digest}}' | tr -d '"')
    FRONTEND_DIGEST=$(docker buildx imagetools inspect "$FRONTEND_IMAGE:$tag" --format '{{json .Manifest.Digest}}' | tr -d '"')

    print_success "Backend digest: $BACKEND_DIGEST"
    print_success "Frontend digest: $FRONTEND_DIGEST"

    write_release_candidate_identity "$BACKEND_DIGEST" "$FRONTEND_DIGEST"

    # Update docker-compose.yml with new image tags (needed for Umbrel testing in Phase 7)
    print_info "Updating umbrel-apps docker-compose.yml..."
    prepare_umbrel_repo
    cd "$UMBREL_APPS_REPO"

    # Update backend image
    sed -i '' "s|image: $BACKEND_IMAGE:v[^@]*@sha256:[a-f0-9]*|image: $BACKEND_IMAGE:$tag@$BACKEND_DIGEST|" canary/docker-compose.yml

    # Update frontend image
    sed -i '' "s|image: $FRONTEND_IMAGE:v[^@]*@sha256:[a-f0-9]*|image: $FRONTEND_IMAGE:$tag@$FRONTEND_DIGEST|" canary/docker-compose.yml

    print_success "Updated docker-compose.yml with new image tags"
    commit_and_push_if_changed \
        "umbrel-apps" \
        "fork" \
        "$UMBREL_RELEASE_BRANCH" \
        "canary: Update Docker images to v$NEW_VERSION" \
        canary/docker-compose.yml

    DOCKER_IMAGES_PUSHED=true
    print_success "Phase 2 complete: Docker images built and pushed"
}

# Phase 6: Umbrel App Update (version only - release notes added in Phase 11)
phase_umbrel_update() {
    print_header "Phase 6: Umbrel App Update"

    local tag="v$NEW_VERSION"

    if [[ "$DRY_RUN" == true ]]; then
        print_warning "[DRY RUN] Would update umbrel-apps files"
        return
    fi

    prepare_umbrel_repo
    cd "$UMBREL_APPS_REPO"

    # Update umbrel-app.yml version only (release notes added in Phase 11 after testing)
    print_info "Updating canary/umbrel-app.yml version..."
    sed -i '' "s/^version: \".*\"/version: \"$NEW_VERSION\"/" canary/umbrel-app.yml

    print_success "Updated umbrel-app.yml version"

    # Note: docker-compose.yml was already updated in Phase 2

    # Show changes
    print_info "Changes in umbrel-apps:"
    git diff canary/

    commit_and_push_if_changed \
        "umbrel-apps" \
        "fork" \
        "$UMBREL_RELEASE_BRANCH" \
        "canary: Update app version to v$NEW_VERSION" \
        canary/umbrel-app.yml

    print_success "Phase 6 complete: Umbrel app files updated (release notes will be added after testing)"
}

# Delete Docker Hub tags for failed release
delete_docker_tags() {
    local tag="v$NEW_VERSION"

    print_info "Deleting Docker Hub tags..."
    print_warning "This requires Docker Hub credentials"

    # Get Docker Hub token
    echo ""
    read -s -p "Enter Docker Hub password for $DOCKER_USER: " HUB_PASSWORD
    echo ""

    # Build the JSON with jq so passwords containing quotes or backslashes stay
    # valid, and send it on stdin to keep the password off the process list.
    local token=$(jq -n --arg username "$DOCKER_USER" --arg password "$HUB_PASSWORD" \
        '{username: $username, password: $password}' | \
        curl -s -X POST "https://hub.docker.com/v2/users/login/" \
            -H "Content-Type: application/json" --data-binary @- | \
        jq -r '.token // empty')
    HUB_PASSWORD=""

    if [[ -z "$token" ]]; then
        print_error "Failed to authenticate with Docker Hub"
        print_info "Delete manually at:"
        print_info "  https://hub.docker.com/r/$BACKEND_IMAGE/tags"
        print_info "  https://hub.docker.com/r/$FRONTEND_IMAGE/tags"
        return 1
    fi

    # Delete backend tag
    local backend_result=$(curl -s -o /dev/null -w "%{http_code}" \
        -X DELETE "https://hub.docker.com/v2/repositories/$BACKEND_IMAGE/tags/$tag/" \
        -H "Authorization: Bearer $token")

    if [[ "$backend_result" == "204" ]]; then
        print_success "Deleted $BACKEND_IMAGE:$tag"
    else
        print_error "Failed to delete $BACKEND_IMAGE:$tag (HTTP $backend_result)"
    fi

    # Delete frontend tag
    local frontend_result=$(curl -s -o /dev/null -w "%{http_code}" \
        -X DELETE "https://hub.docker.com/v2/repositories/$FRONTEND_IMAGE/tags/$tag/" \
        -H "Authorization: Bearer $token")

    if [[ "$frontend_result" == "204" ]]; then
        print_success "Deleted $FRONTEND_IMAGE:$tag"
    else
        print_error "Failed to delete $FRONTEND_IMAGE:$tag (HTTP $frontend_result)"
    fi
}

# Reset Umbrel app store to upstream version
reset_umbrel_to_upstream() {
    print_info "Resetting Umbrel app store to upstream version..."

    cd "$UMBREL_APPS_REPO"

    # Fetch latest from upstream (origin points to getumbrel/umbrel-apps)
    print_info "Fetching latest from origin..."
    git fetch origin master

    # Checkout canary folder from origin
    print_info "Restoring canary/ from origin master..."
    git checkout origin/master -- canary/

    # Sync the upstream version back to Umbrel
    print_info "Syncing upstream version to Umbrel..."
    ping_host_or_retry "Umbrel" "$(host_from_ssh_target "$UMBREL_HOST")"
    rsync -avz "$UMBREL_APPS_REPO/canary/" "$UMBREL_HOST:$UMBREL_APP_STORE_PATH"

    # Get the upstream version for display
    local upstream_version=$(grep '^version: ' canary/umbrel-app.yml | sed 's/version: "\(.*\)"/\1/')
    print_success "Reset to upstream version $upstream_version"

    # Discard local changes to canary/
    git checkout HEAD -- canary/

    print_info "You can now uninstall and reinstall Canary from the App Store"
}

# Phase 7: Local Testing (rsync)
phase_local_testing() {
    print_header "Phase 7: Umbrel Testing"

    if [[ "$SKIP_RSYNC" == true ]]; then
        print_warning "Skipping rsync (--skip-rsync)"
        return
    fi

    if [[ "$DRY_RUN" == true ]]; then
        print_warning "[DRY RUN] Would rsync to $UMBREL_HOST:$UMBREL_APP_STORE_PATH"
        return
    fi

    prepare_umbrel_repo
    ping_host_or_retry "Umbrel" "$(host_from_ssh_target "$UMBREL_HOST")"
    print_info "Syncing to Umbrel..."
    rsync -avz "$UMBREL_APPS_REPO/canary/" "$UMBREL_HOST:$UMBREL_APP_STORE_PATH"

    print_success "Files synced to Umbrel"

    echo ""
    print_info "Check for updates (expecting v$NEW_VERSION):"
    echo "  http://umbrel.local/app-store?dialog=updates"
    echo ""

    if ! confirm "Did Umbrel testing pass successfully?"; then
        print_error "Testing failed"
        echo ""

        # Offer to reset Umbrel app store
        if confirm "Reset Umbrel app store to upstream version?" "y"; then
            reset_umbrel_to_upstream
        fi

        # Offer to delete Docker Hub tags if they were pushed
        echo ""
        if [[ "$DOCKER_IMAGES_PUSHED" == true ]] || [[ "$FROM_PHASE" -gt 2 ]]; then
            print_warning "Docker images v$NEW_VERSION were pushed to Docker Hub"
            if confirm "Delete broken Docker images from Docker Hub?" "n"; then
                delete_docker_tags
            else
                print_info "Images will be overwritten when you retry the release"
            fi
        fi

        # Reset the version bump commit
        echo ""
        cd "$CANARY_REPO"
        local last_commit_msg=$(git log -1 --format=%s)
        if [[ "$last_commit_msg" == *"Bump version to $NEW_VERSION"* ]]; then
            git reset --hard HEAD~1
            print_success "Reset version bump commit"
        fi

        echo ""
        print_info "To retry from Phase 1 (version bump): scripts/release.sh $NEW_VERSION"
        print_info "To retry from Phase 2 (rebuild Docker): scripts/release.sh $NEW_VERSION --from-phase=2"
        exit 1
    fi

    run_node_browser_auth_gate "umbrel"
    record_release_gate "umbrel" "$UMBREL_APPS_REPO"

    print_success "Phase 7 complete: Local testing passed"
}

# Phase 4: Start9 Build (s9pk)
phase_start9_build() {
    print_header "Phase 4: Start9 Build"

    if [[ "$SKIP_START9" == true ]]; then
        print_warning "Skipping Start9 build (--skip-start9)"
        return
    fi

    if [[ "$DRY_RUN" == true ]]; then
        print_warning "[DRY RUN] Would build s9pk package"
        return
    fi

    prepare_start9_repo
    ensure_start9_packaging_workspace
    cd "$CANARY_STARTOS_REPO"

    # The SDK clean target removes node_modules, but Makefile includes the SDK
    # makefile while parsing. Install once so clean can load its target, then
    # reinstall from the committed lockfile for the build.
    print_info "Preparing npm dependencies for cleanup..."
    npm ci

    # Clean previous build artifacts (including node_modules)
    print_info "Cleaning previous build artifacts..."
    make clean

    print_info "Installing npm dependencies for the build..."
    npm ci
    print_success "npm dependencies installed"

    # Build s9pk for Start9's configured architectures
    print_info "Building s9pk package (this takes a while)..."
    make

    local package_count
    package_count=$(find . -maxdepth 1 -name '*.s9pk' | wc -l | tr -d ' ')
    if [[ "$package_count" == "0" ]]; then
        print_error "No Start9 .s9pk packages were built"
        exit 1
    fi

    print_success "Built Start9 package(s):"
    find . -maxdepth 1 -name '*.s9pk' -print0 | while IFS= read -r -d '' pkg; do
        local pkg_size
        pkg_size=$(du -h "$pkg" | cut -f1)
        echo "  - ${pkg#./} ($pkg_size)"
    done

    print_success "Phase 4 complete: s9pk package built"
}

# Phase 5: Start9 Local Testing
phase_start9_testing() {
    print_header "Phase 5: Start9 Testing"

    if [[ "$SKIP_START9" == true ]]; then
        print_warning "Skipping Start9 testing (--skip-start9)"
        return
    fi

    if [[ "$DRY_RUN" == true ]]; then
        print_warning "[DRY RUN] Would test s9pk on StartOS"
        return
    fi

    prepare_start9_repo
    ensure_start9_packaging_workspace
    cd "$CANARY_STARTOS_REPO"

    echo ""
    print_info "Testing instructions:"
    echo "  Preferred path: use start-cli sideload when configured."
    echo "  Manual fallback:"
    echo "    1. Go to your StartOS dashboard"
    echo "    2. Navigate to System -> Sideload Service"
    echo "    3. Upload the newest .s9pk from: $CANARY_STARTOS_REPO"
    echo ""

    local start9_url
    start9_url="$(start9_config_url || true)"
    if [[ -n "$start9_url" ]]; then
        if confirm "Install/sideload via CLI to StartOS?"; then
            local start9_host
            start9_host="$(host_from_url "$start9_url")"
            if [[ -n "$start9_host" ]]; then
                ping_host_or_retry "StartOS" "$start9_host"
            else
                print_warning "Could not detect the StartOS hostname; continuing to CLI install"
            fi
            print_info "Installing to StartOS via CLI..."
            local selected_package
            selected_package="$(start-cli -H "$start9_url" s9pk select ./*.s9pk)"
            start-cli -H "$start9_url" package install -s "$selected_package"
            print_success "Package installed to StartOS"
        fi
    else
        print_warning "No StartOS host configured; set START9_HOST or use the manual sideload fallback."
    fi

    echo ""
    print_info "Verify on StartOS:"
    echo "  1. Confirm the service installs or upgrades successfully"
    echo "  2. Confirm the service reaches a healthy/running state"
    echo "  3. Open Canary from the StartOS dashboard"
    echo "  4. Smoke-test the release-specific user flow for v$NEW_VERSION"
    echo "  5. Restart Canary and confirm the app still starts cleanly"
    echo ""

    if ! confirm "Did Start9 testing pass successfully?"; then
        print_error "Start9 testing failed"
        echo ""
        print_info "You can retry from Phase 4 (Start9 build): scripts/release.sh $NEW_VERSION --from-phase=4"
        exit 1
    fi

    run_node_browser_auth_gate "startos"
    record_release_gate "startos" "$CANARY_STARTOS_REPO"

    print_success "Phase 5 complete: Start9 local testing passed"
}

# Phase 3: Start9 Version Bump
phase_start9_version_bump() {
    print_header "Phase 3: Start9 Version Bump"

    local app_tag="v$NEW_VERSION"
    local start9_version="$NEW_VERSION:0"
    local start9_tag
    start9_tag=$(start9_release_tag)

    if [[ "$DRY_RUN" == true ]]; then
        print_warning "[DRY RUN] Would update Start9 package metadata to $start9_version ($start9_tag)"
        return
    fi

    # Update Start9 manifest.ts and version files (if not skipping Start9)
    if [[ "$SKIP_START9" != true ]]; then
        prepare_start9_repo
        cd "$CANARY_STARTOS_REPO"

        local manifest_file="startos/manifest/index.ts"
        local versions_dir="startos/versions"
        local versions_index="$versions_dir/index.ts"

        if [[ ! -f "$manifest_file" || ! -f "$versions_index" ]]; then
            print_error "canary-startos does not look like the current Start9-Community wrapper layout"
            print_info "Expected $manifest_file and $versions_index after rebasing on $START9_BASE_REMOTE/master"
            exit 1
        fi

        # Update docker tags in the Start9 manifest.
        print_info "Updating Start9 docker tags to $app_tag..."
        sed -i '' "s|dockerTag: 'schjonhaug/canary-frontend:v[^']*'|dockerTag: 'schjonhaug/canary-frontend:$app_tag'|" "$manifest_file"
        sed -i '' "s|dockerTag: 'schjonhaug/canary-backend:v[^']*'|dockerTag: 'schjonhaug/canary-backend:$app_tag'|" "$manifest_file"
        print_success "Updated Start9 manifest docker tags"

        # Create new version file (version format: X.Y.Z:0).
        local uses_current_version_file=false
        if start9_uses_current_version_file "$versions_index"; then
            uses_current_version_file=true
        fi

        local version_file
        version_file=$(start9_release_version_file "$versions_index")
        local version_import="./$(basename "$version_file" .ts)"
        local version_var
        version_var=$(start9_release_version_var "$versions_index")

        # Update versions/index.ts to use new version as current.
        print_info "Updating Start9 version graph..."
        local prev_version_var
        local prev_version_import
        if [[ "$uses_current_version_file" == true ]]; then
            prev_version_var="current"
            prev_version_import="./current"
        else
            prev_version_var=$(grep -oE 'current:[[:space:]]*[A-Za-z0-9_]+' "$versions_index" | head -1 | sed 's/current:[[:space:]]*//')
            prev_version_import=$(grep -E "import .*${prev_version_var}.* from " "$versions_index" | sed "s/.*from '\(.*\)'.*/\1/" | head -1)
        fi

        if [[ -z "$prev_version_var" || -z "$prev_version_import" ]]; then
            print_error "Could not detect current Start9 version from $versions_index"
            exit 1
        fi

        local prev_version_file="$versions_dir/${prev_version_import#./}.ts"
        local previous_is_unpublished=false
        if [[ "$prev_version_var" != "$version_var" ]] &&
            remove_start9_unpublished_version_file_if_needed "$START9_BASE_REMOTE/master" "$prev_version_import" "$prev_version_file"; then
            previous_is_unpublished=true
            print_info "Removed unpublished Start9 version file superseded by v$NEW_VERSION: $prev_version_file"
        fi

        print_info "Creating version file: $version_file..."
        local localized_notes_file
        localized_notes_file=$(start9_localized_release_notes_file)
        if release_notes_file_matches_current_version "$localized_notes_file" &&
            validate_start9_localized_release_notes "$localized_notes_file"; then
            create_start9_version_file_with_notes "$version_file" "$version_var" "$localized_notes_file"
            print_info "Preserved approved localized release notes in the StartOS package"
        else
            create_start9_version_file "$version_file" "$version_var" "$start9_version"
        fi
        print_success "Created version file"

        if [[ "$uses_current_version_file" != true ]]; then
            rewrite_start9_version_graph \
                "$versions_index" \
                "$version_import" \
                "$version_var" \
                "$prev_version_var" \
                "$prev_version_import" \
                "$previous_is_unpublished"
        fi
        print_success "Updated Start9 version graph"

        local commit_paths=("$manifest_file" "$version_file")
        if [[ "$uses_current_version_file" != true ]]; then
            commit_paths+=("$versions_index")
        fi
        if [[ "$previous_is_unpublished" == true ]]; then
            commit_paths+=("$prev_version_file")
        fi

        commit_and_push_if_changed \
            "canary-startos" \
            "origin" \
            "$START9_RELEASE_BRANCH" \
            "Update Canary metadata to v$NEW_VERSION" \
            "${commit_paths[@]}"
    fi

    print_success "Phase 3 complete: Start9 metadata updated"
}

ensure_node_distro_release_notes() {
    if [[ -n "$NODE_DISTRO_RELEASE_NOTES" ]]; then
        RELEASE_NOTES="$NODE_DISTRO_RELEASE_NOTES"
        return
    fi

    load_node_distro_release_notes
    if [[ -n "$NODE_DISTRO_RELEASE_NOTES" ]]; then
        return
    fi

    local generated_notes
    generated_notes="$(summarize_github_release_for_app_store || true)"
    if [[ -n "$generated_notes" ]]; then
        NODE_DISTRO_RELEASE_NOTES="$generated_notes"
        RELEASE_NOTES="$NODE_DISTRO_RELEASE_NOTES"
        print_success "Generated node distro release notes from GitHub release"
        save_node_distro_release_notes "$NODE_DISTRO_RELEASE_NOTES"
        return
    fi

    # Fallback for pre-release phases where the GitHub release does not exist yet.
    recover_app_store_release_notes
    NODE_DISTRO_RELEASE_NOTES="$RELEASE_NOTES"
}


fallback_node_distro_release_notes_for() {
    local distro="$1"
    local notes="$2"

    case "$distro" in
        umbrel)
            printf '%s' "$notes" | sed 's/Umbrel\/StartOS/Umbrel/g; s/Umbrel, StartOS, and myNode/Umbrel/g; s/StartOS\/Umbrel/Umbrel/g'
            ;;
        startos)
            printf '%s' "$notes" | sed 's/Umbrel\/StartOS/StartOS/g; s/Umbrel, StartOS, and myNode/StartOS/g; s/StartOS\/Umbrel/StartOS/g'
            ;;
        mynode)
            printf '%s' "$notes" | sed 's/Umbrel\/StartOS/myNode/g; s/Umbrel, StartOS, and myNode/myNode/g; s/StartOS\/Umbrel/myNode/g; s/ntfy settings, //g; s/ and ntfy settings//g; s/Umbrel\/StartOS local integrations/myNode local integrations/g'
            ;;
        *)
            printf '%s' "$notes"
            ;;
    esac
}

ensure_node_distro_release_notes_for() {
    local distro="$1"
    local distro_label="$2"

    if load_node_distro_release_notes_for "$distro"; then
        return
    fi

    local generated_notes
    generated_notes="$(summarize_github_release_for_node_distro "$distro_label" || true)"
    if [[ -n "$generated_notes" ]]; then
        NODE_DISTRO_RELEASE_NOTES="$generated_notes"
        RELEASE_NOTES="$NODE_DISTRO_RELEASE_NOTES"
        print_success "Generated $distro_label release notes from GitHub release"
        save_node_distro_release_notes_for "$distro" "$NODE_DISTRO_RELEASE_NOTES"
        return
    fi

    ensure_node_distro_release_notes
    NODE_DISTRO_RELEASE_NOTES="$(fallback_node_distro_release_notes_for "$distro" "$NODE_DISTRO_RELEASE_NOTES")"
    RELEASE_NOTES="$NODE_DISTRO_RELEASE_NOTES"
    save_node_distro_release_notes_for "$distro" "$NODE_DISTRO_RELEASE_NOTES"
}

ensure_app_store_release_notes() {
    recover_app_store_release_notes
    if [[ -n "$RELEASE_NOTES" ]]; then
        return
    fi

    # Get release notes from Codex after testing is complete.
    print_info "Generating release notes with Codex..."
    cd "$CANARY_REPO"
    local prev_tag=$(git describe --tags --abbrev=0 HEAD^ 2>/dev/null || echo "")
    local commits=""
    if [[ -n "$prev_tag" ]]; then
        commits=$(git log "$prev_tag"..HEAD --oneline)
    else
        commits=$(git log --oneline -10)
    fi

    # Generate release notes for Umbrel and Start9 app stores.
    local release_notes=""
    local codex_available=false
    command -v codex &> /dev/null && codex_available=true

    if [[ "$codex_available" == true ]]; then
        local codex_output
        codex_output=$(mktemp)
        codex exec -C "$CANARY_REPO" --sandbox read-only --ask-for-approval never --output-last-message "$codex_output" "Generate release notes for app store listings (Umbrel and Start9). Based on these commits, focus on SELF-HOSTED user-facing changes only. IGNORE any commits mentioning: Cloud, billing, Stripe, SaaS, SMS notifications, or email notifications.

FORMAT: Use your judgment based on the scope of changes:
- Few minor changes: Write 1-2 concise sentences
- Many changes or significant updates: Use a bullet list like this (no leading spaces on first line, 2 spaces before each dash):
This update contains various bug fixes and improvements:
  - First change description
  - Second change description

RULES:
- NO markdown (no **, no #), NO version prefix
- Start bullet points with a verb (Fixed, Added, Improved, etc.)

Commits: $commits" >/dev/null 2>&1 || true
        release_notes=$(cat "$codex_output" 2>/dev/null || echo "")
        rm -f "$codex_output"
    fi

    if [[ -z "$release_notes" ]]; then
        print_warning "Could not generate release notes with Codex"
        echo "Recent commits:"
        echo "$commits"
        echo ""
        release_notes=$(read_multiline_input "Enter release notes for app stores (Umbrel & Start9):")
    else
        # Loop to refine release notes with Codex.
        while true; do
            print_info "Generated release notes: $release_notes"
            echo ""
            echo "Options:"
            echo "  y) Use these release notes"
            echo "  n) Enter custom release notes manually"
            if [[ "$codex_available" == true ]]; then
                echo "  r) Refine with Codex (provide feedback)"
            fi
            echo ""
            read -p "Choice [y/n/r]: " choice

            case "$choice" in
                [Yy]*)
                    break
                    ;;
                [Rr]*)
                    if [[ "$codex_available" == true ]]; then
                        read -p "How should the release notes be improved? " feedback
                        print_info "Refining release notes..."
                        local codex_output
                        codex_output=$(mktemp)
                        codex exec -C "$CANARY_REPO" --sandbox read-only --ask-for-approval never --output-last-message "$codex_output" "Improve these release notes based on feedback. Current notes: '$release_notes'. Feedback: '$feedback'. Rules: Output ONLY the improved release notes, 1-2 sentences, plain text, NO markdown, NO version prefix:" >/dev/null 2>&1 || true
                        release_notes=$(cat "$codex_output" 2>/dev/null || echo "$release_notes")
                        rm -f "$codex_output"
                    else
                        print_warning "Codex CLI not available"
                    fi
                    ;;
                [Nn]*)
                    release_notes=$(read_multiline_input "Enter custom release notes:")
                    break
                    ;;
                *)
                    print_warning "Invalid choice, please enter y, n, or r"
                    ;;
            esac
        done
    fi

    RELEASE_NOTES="$release_notes"
    save_app_store_release_notes "$RELEASE_NOTES"
}

write_node_distro_pr_body() {
    local output_file="$1"
    local package_label="$2"
    local test_plan="$3"
    local extra_note="${4:-}"

    {
        echo "## Summary"
        echo "Update Canary $package_label to v$NEW_VERSION."
        echo ""
        if [[ -n "$extra_note" ]]; then
            echo "$extra_note"
            echo ""
        fi
        echo "## Release summary"
        echo "${NODE_DISTRO_RELEASE_NOTES:-See the Canary GitHub release.}"
        echo ""
        echo "## Upstream release"
        echo "https://github.com/schjonhaug/canary/releases/tag/v$NEW_VERSION"
        echo ""
        echo "## Test plan"
        printf '%s\n' "$test_plan"
    } > "$output_file"
}

create_or_update_draft_pr() {
    local repo="$1"
    local head="$2"
    local base="$3"
    local title="$4"
    local body_file="$5"

    local pr_number pr_url
    pr_number=$(gh api -X GET "repos/$repo/pulls" \
        -f state=open \
        -f head="$head" \
        -f base="$base" \
        --jq '.[0].number // empty')
    if [[ -z "$pr_number" ]]; then
        print_info "Creating draft PR..." >&2
        if ! pr_url=$(gh pr create \
            --repo "$repo" \
            --base "$base" \
            --head "$head" \
            --title "$title" \
            --body-file "$body_file" \
            --draft); then
            print_error "Failed to create draft PR for $repo:$head" >&2
            return 1
        fi
    else
        print_info "Updating PR #$pr_number title and body..." >&2
        if ! gh pr edit "$pr_number" --repo "$repo" --title "$title" --body-file "$body_file" >/dev/null; then
            print_error "Failed to update PR #$pr_number in $repo" >&2
            return 1
        fi
        pr_url=$(gh pr view "$pr_number" --repo "$repo" --json url --jq '.url')
    fi

    printf '%s' "$pr_url"
}

prepare_umbrel_gallery_repo() {
    local branch="canary-v$NEW_VERSION-gallery"

    if ! gh repo view "$UMBREL_GALLERY_HEAD_REPO" >/dev/null 2>&1; then
        print_info "Creating the Umbrel gallery fork $UMBREL_GALLERY_HEAD_REPO..."
        gh repo fork "$UMBREL_GALLERY_BASE_REPO" --clone=false
    fi

    if [[ ! -d "$UMBREL_GALLERY_REPO/.git" ]]; then
        print_info "Cloning the Umbrel gallery fork..."
        git clone "git@github.com:$UMBREL_GALLERY_HEAD_REPO.git" "$UMBREL_GALLERY_REPO"
    fi

    assert_no_git_operation_in_progress "$UMBREL_GALLERY_REPO" "umbrel-apps-gallery"
    require_clean_repo_at "$UMBREL_GALLERY_REPO" "umbrel-apps-gallery"

    local origin_url upstream_url
    origin_url=$(git -C "$UMBREL_GALLERY_REPO" remote get-url origin 2>/dev/null || true)
    if ! remote_url_matches_github_repo "$origin_url" "$UMBREL_GALLERY_HEAD_REPO"; then
        print_error "umbrel-apps-gallery: origin must point to $UMBREL_GALLERY_HEAD_REPO"
        print_info "Current origin URL: ${origin_url:-missing}"
        exit 1
    fi

    upstream_url=$(git -C "$UMBREL_GALLERY_REPO" remote get-url upstream 2>/dev/null || true)
    if [[ -z "$upstream_url" ]]; then
        git -C "$UMBREL_GALLERY_REPO" remote add upstream "https://github.com/$UMBREL_GALLERY_BASE_REPO.git"
    elif ! remote_url_matches_github_repo "$upstream_url" "$UMBREL_GALLERY_BASE_REPO"; then
        print_error "umbrel-apps-gallery: upstream must point to $UMBREL_GALLERY_BASE_REPO"
        print_info "Current upstream URL: $upstream_url"
        exit 1
    fi

    git -C "$UMBREL_GALLERY_REPO" fetch upstream master
    git -C "$UMBREL_GALLERY_REPO" fetch origin "$branch" 2>/dev/null || true

    if git -C "$UMBREL_GALLERY_REPO" show-ref --verify --quiet "refs/heads/$branch"; then
        git -C "$UMBREL_GALLERY_REPO" checkout "$branch"
    elif git -C "$UMBREL_GALLERY_REPO" show-ref --verify --quiet "refs/remotes/origin/$branch"; then
        git -C "$UMBREL_GALLERY_REPO" checkout -b "$branch" "origin/$branch"
    else
        git -C "$UMBREL_GALLERY_REPO" checkout -b "$branch" "upstream/master"
    fi

    if git -C "$UMBREL_GALLERY_REPO" show-ref --verify --quiet "refs/remotes/origin/$branch"; then
        git -C "$UMBREL_GALLERY_REPO" rebase "origin/$branch"
    fi
    git -C "$UMBREL_GALLERY_REPO" rebase upstream/master
}

publish_umbrel_gallery_screenshots() {
    local branch="canary-v$NEW_VERSION-gallery"
    local source_file
    for source_file in "$CANARY_REPO"/screenshots/umbrel/{1,2,3}.jpg; do
        if [[ ! -s "$source_file" ]]; then
            print_error "Missing Umbrel gallery asset: $source_file"
            print_info "Run Phase 1 to generate and approve the release screenshots first."
            exit 1
        fi
    done

    print_info "Publishing the approved Umbrel gallery cards..."
    prepare_umbrel_gallery_repo
    mkdir -p "$UMBREL_GALLERY_REPO/canary"
    cp "$CANARY_REPO"/screenshots/umbrel/{1,2,3}.jpg "$UMBREL_GALLERY_REPO/canary/"

    cd "$UMBREL_GALLERY_REPO"
    commit_and_push_if_changed \
        "umbrel-apps-gallery" \
        "origin" \
        "$branch" \
        "Update Canary screenshots for v$NEW_VERSION" \
        canary/1.jpg canary/2.jpg canary/3.jpg

    if git diff --quiet upstream/master...HEAD -- canary/1.jpg canary/2.jpg canary/3.jpg; then
        print_success "Umbrel gallery already contains the approved Canary screenshots"
        return
    fi

    local body_file
    body_file=$(mktemp)
    {
        echo "## Summary"
        echo "Refresh the Canary gallery for v$NEW_VERSION with the approved release screenshots."
        echo ""
        echo "The cards retain Umbrel's yellow branded presentation while showing the current Canary UI."
        echo ""
        echo "## Source"
        echo "https://github.com/schjonhaug/canary/releases/tag/v$NEW_VERSION"
    } > "$body_file"
    UMBREL_GALLERY_PR_URL=$(create_or_update_draft_pr \
        "$UMBREL_GALLERY_BASE_REPO" \
        "schjonhaug:$branch" \
        "master" \
        "Update Canary screenshots for v$NEW_VERSION" \
        "$body_file")
    rm -f "$body_file"
    print_success "Umbrel gallery draft PR ready: $UMBREL_GALLERY_PR_URL"
}

ensure_canary_version_bump_merged() {
    cd "$CANARY_REPO"

    local branch="canary-v$NEW_VERSION"
    local remote_backend_version=""
    local remote_frontend_version=""

    git fetch origin master
    remote_backend_version=$(get_backend_package_version_at_ref "origin/master" || true)
    remote_frontend_version=$(get_frontend_package_version_at_ref "origin/master" || true)

    if [[ "$remote_backend_version" == "$NEW_VERSION" && "$remote_frontend_version" == "$NEW_VERSION" ]]; then
        print_success "canary: origin/master already has version metadata for v$NEW_VERSION"
        git pull --rebase origin master
        return 0
    fi

    if [[ "$(get_backend_package_version)" != "$NEW_VERSION" || "$(get_frontend_package_version)" != "$NEW_VERSION" ]]; then
        print_error "canary: local version metadata is not v$NEW_VERSION"
        print_info "Run Phase 1 first, or resolve the canary repo state before tagging."
        exit 1
    fi

    print_info "canary: pushing version bump branch $branch"
    git push -u origin "HEAD:refs/heads/$branch"

    local body_file pr_number pr_url
    body_file=$(mktemp)
    {
        echo "## Summary"
        echo "Bump Canary package metadata to v$NEW_VERSION before tagging the release."
        echo ""
        echo "## Test plan"
        echo "- [x] Node distro smoke testing completed before release tagging"
    } > "$body_file"

    # gh pr list expects the branch name (without owner) for branches hosted in
    # the base repository. Using owner:branch here fails to find an existing PR
    # and makes a resumed release try to create a duplicate.
    pr_number=$(gh pr list --repo "schjonhaug/canary" --state open --head "$branch" --json number --jq '.[0].number')
    if [[ -z "$pr_number" ]]; then
        pr_url=$(gh pr create \
            --repo "schjonhaug/canary" \
            --base "master" \
            --head "schjonhaug:$branch" \
            --title "Bump version to v$NEW_VERSION" \
            --body-file "$body_file")
    else
        gh pr edit "$pr_number" --repo "schjonhaug/canary" --title "Bump version to v$NEW_VERSION" --body-file "$body_file" >/dev/null
        pr_url=$(gh pr view "$pr_number" --repo "schjonhaug/canary" --json url --jq '.url')
    fi
    rm -f "$body_file"

    print_warning "Merge the Canary version-bump PR before continuing:"
    echo "  $pr_url"

    while true; do
        wait_for_confirmation "Press Enter after the PR is merged."

        git fetch origin master
        remote_backend_version=$(get_backend_package_version_at_ref "origin/master" || true)
        remote_frontend_version=$(get_frontend_package_version_at_ref "origin/master" || true)
        if [[ "$remote_backend_version" == "$NEW_VERSION" && "$remote_frontend_version" == "$NEW_VERSION" ]]; then
            break
        fi

        print_warning "canary: origin/master still does not contain version metadata for v$NEW_VERSION"
        print_info "backend/Cargo.toml on origin/master: ${remote_backend_version:-unknown}"
        print_info "frontend/package.json on origin/master: ${remote_frontend_version:-unknown}"
        print_info "Merge the PR, then press Enter to check again:"
        echo "  $pr_url"
    done

    git pull --rebase origin master
    print_success "canary: version bump is merged to origin/master"
}

# Phase 11: Umbrel PR Preparation
phase_umbrel_pr() {
    print_header "Phase 11: Umbrel PR"

    if [[ "$DRY_RUN" == true ]]; then
        print_warning "[DRY RUN] Would add release notes and create the Umbrel app and gallery draft PRs"
        return
    fi

    ensure_node_distro_release_notes_for "umbrel" "Umbrel"
    local release_notes="$NODE_DISTRO_RELEASE_NOTES"

    prepare_umbrel_repo
    cd "$UMBREL_APPS_REPO"

    # Update releaseNotes in umbrel-app.yml. Pass multiline notes through a
    # temporary file because awk -v treats embedded newlines as source text.
    print_info "Updating releaseNotes in umbrel-app.yml..."
    local notes_file output_file
    notes_file=$(mktemp)
    output_file=$(mktemp)
    printf '%s\n' "$release_notes" > "$notes_file"
    if ! awk -v notes_file="$notes_file" '
        /^releaseNotes: >-$/ {
            print
            # Skip the full existing YAML block, including blank separator
            # lines, until the next non-indented key.
            while ((getline line) > 0) {
                if (line ~ /^[[:space:]]/ || line ~ /^$/) continue
                break
            }
            # Print new release notes with YAML block indentation.
            while ((getline note_line < notes_file) > 0) print "  " note_line
            close(notes_file)
            # Print the line that ended the block (non-indented)
            print line
            next
        }
        { print }
    ' canary/umbrel-app.yml > "$output_file"; then
        rm -f "$notes_file" "$output_file"
        print_error "Failed to update Umbrel release notes"
        return 1
    fi
    mv "$output_file" canary/umbrel-app.yml
    rm -f "$notes_file"
    print_success "Updated releaseNotes"

    # Show final changes
    print_info "Final changes in umbrel-apps:"
    git diff canary/

    commit_and_push_if_changed \
        "umbrel-apps" \
        "fork" \
        "$UMBREL_RELEASE_BRANCH" \
        "canary: Add release notes for v$NEW_VERSION" \
        canary/umbrel-app.yml

    if ! release_gate_matches_packaging_head "umbrel" "$UMBREL_APPS_REPO"; then
        print_warning "Umbrel packaging changed after its recorded test pass"
        print_info "Repeating the Umbrel browser gate against the final packaging commit..."
        phase_local_testing
    fi

    local body_file pr_url
    body_file=$(mktemp)
    write_node_distro_pr_body \
        "$body_file" \
        "Umbrel app" \
        "- [x] Upgraded a local Umbrel installation from its previously published Canary package directly to v$NEW_VERSION
- [x] Verified existing wallets, notification contacts, destinations, settings, and history were preserved
- [x] Verified live delivery through Umbrel's detected local ntfy server and inactive-contact non-delivery
- [x] Restarted Canary and verified the migrated data remained intact without duplicate delivery"
    pr_url=$(create_or_update_draft_pr \
        "getumbrel/umbrel-apps" \
        "schjonhaug:$UMBREL_RELEASE_BRANCH" \
        "master" \
        "canary: Update to v$NEW_VERSION" \
        "$body_file")
    rm -f "$body_file"

    print_success "Draft PR ready: $pr_url"
    echo ""
    print_info "Review the PR and mark it ready for review when satisfied:"
    echo "  $pr_url"
    echo ""

    publish_umbrel_gallery_screenshots
    if [[ -n "$UMBREL_GALLERY_PR_URL" ]]; then
        print_info "Review the gallery PR and mark it ready for review when satisfied:"
        echo "  $UMBREL_GALLERY_PR_URL"
        echo ""
    fi

    print_success "Phase 11 complete: Umbrel app and gallery draft PRs ready"
}

# Phase 10: Start9 PR
phase_start9_submission() {
    print_header "Phase 10: Start9 PR"

    if [[ "$SKIP_START9" == true ]]; then
        print_warning "Skipping Start9 PR (--skip-start9)"
        return
    fi

    if [[ "$DRY_RUN" == true ]]; then
        print_warning "[DRY RUN] Would commit/push canary-startos and create a draft PR to $START9_BASE_REPO"
        return
    fi

    ensure_node_distro_release_notes_for "startos" "StartOS"
    if [[ -z "$RELEASE_NOTES" ]]; then
        print_error "StartOS release notes are empty"
        return 1
    fi
    ensure_start9_localized_release_notes "$RELEASE_NOTES"

    prepare_start9_repo
    cd "$CANARY_STARTOS_REPO"

    print_info "Updating localized release notes in Start9 version file..."
    local versions_index="startos/versions/index.ts"
    local version_file localized_notes_file version_var
    version_file=$(start9_release_version_file "$versions_index")
    localized_notes_file=$(start9_localized_release_notes_file)
    version_var=$(start9_release_version_var "$versions_index")
    if [[ ! -f "$version_file" ]]; then
        print_error "Version file not found: $version_file"
        return 1
    fi
    create_start9_version_file_with_notes "$version_file" "$version_var" "$localized_notes_file"
    print_success "Updated localized Start9 release notes"

    local branch
    branch=$(git rev-parse --abbrev-ref HEAD)
    commit_and_push_if_changed \
        "canary-startos" \
        "origin" \
        "$branch" \
        "Add Canary v$NEW_VERSION release notes" \
        "$version_file"

    if ! release_gate_matches_packaging_head "startos" "$CANARY_STARTOS_REPO"; then
        print_warning "StartOS packaging changed after its recorded test pass"
        print_info "Rebuilding and repeating the StartOS browser gate against the final packaging commit..."
        phase_start9_build
        phase_start9_testing
    fi

    local pr_url
    pr_url=$(gh pr list \
        --repo "$START9_BASE_REPO" \
        --head "${START9_HEAD_REPO%%/*}:$branch" \
        --json url \
        --jq '.[0].url')

    local body_file
    body_file=$(mktemp)
    write_node_distro_pr_body \
        "$body_file" \
        "StartOS package" \
        "- [x] Built with \`make\`
- [x] Upgraded a local StartOS installation from its previously published Canary package directly to v$NEW_VERSION
- [x] Verified existing wallets, notification contacts, destinations, settings, and history were preserved
- [x] Verified live notification delivery and restart without duplicate delivery"
    pr_url=$(create_or_update_draft_pr \
        "$START9_BASE_REPO" \
        "${START9_HEAD_REPO%%/*}:$branch" \
        "master" \
        "Update Canary to v$NEW_VERSION" \
        "$body_file")
    rm -f "$body_file"

    print_success "Draft PR ready: $pr_url"
    echo ""
    print_info "Review the PR and mark it ready for review when satisfied:"
    echo "  $pr_url"
    echo ""
    print_info "After Start9 merges it, their workflow will tag and publish the StartOS package."

    print_success "Phase 10 complete: Start9 draft PR ready"
}

# Phase 8: myNode Version Bump & Build
phase_mynode_build() {
    print_header "Phase 8: myNode Version Bump & Build"

    if [[ "$SKIP_MYNODE" == true ]]; then
        print_warning "Skipping myNode build (--skip-mynode)"
        return
    fi

    local tag="v$NEW_VERSION"

    if [[ "$DRY_RUN" == true ]]; then
        print_warning "[DRY RUN] Would update myNode Canary app to $tag and build canary.tar.gz from a temporary staging directory"
        return
    fi

    prepare_mynode_repo
    cd "$MYNODE_REPO"

    print_info "Updating myNode package version to $tag..."
    sed -i '' "s|\"latest_version\": \"v[^\"]*\"|\"latest_version\": \"$tag\"|" "$MYNODE_CANARY_APP_DIR/canary.json"
    sed -i '' "s|VERSION=\"\${VERSION:-v[^\"]*}\"|VERSION=\"\${VERSION:-$tag}\"|" "$MYNODE_CANARY_APP_DIR/scripts/install_canary.sh"
    sed -i '' "s|echo v[0-9][^)}]*|echo $tag|" "$MYNODE_CANARY_APP_DIR/scripts/pre_canary.sh"
    sed -i '' "s|image: .*canary-backend.*|image: $BACKEND_IMAGE:$tag|" "$MYNODE_CANARY_APP_DIR/app_data/docker-compose.yml"
    sed -i '' "s|image: .*canary-frontend.*|image: $FRONTEND_IMAGE:$tag|" "$MYNODE_CANARY_APP_DIR/app_data/docker-compose.yml"
    print_success "Updated myNode version references"

    print_info "Refreshing myNode Marketplace screenshots from the approved Canary release set..."
    local source_index target_index
    for source_index in 00 01 02 03 04 05; do
        target_index=$(printf '%02d' "$((10#$source_index + 1))")
        cp \
            "$CANARY_REPO/screenshots/screenshot-$source_index.png" \
            "$MYNODE_CANARY_APP_DIR/screenshots/screenshot-$target_index.png"
    done
    print_success "Updated six myNode Marketplace screenshots"

    commit_and_push_if_changed \
        "mynode" \
        "origin" \
        "$MYNODE_RELEASE_BRANCH" \
        "Update Canary to v$NEW_VERSION" \
        rootfs/standard/usr/share/mynode_apps/canary/canary.json \
        rootfs/standard/usr/share/mynode_apps/canary/app_data/docker-compose.yml \
        rootfs/standard/usr/share/mynode_apps/canary/scripts/install_canary.sh \
        rootfs/standard/usr/share/mynode_apps/canary/scripts/uninstall_canary.sh \
        rootfs/standard/usr/share/mynode_apps/canary/scripts/pre_canary.sh \
        rootfs/standard/usr/share/mynode_apps/canary/screenshots/

    local build_dir="/tmp/canary-mynode-release-$NEW_VERSION"
    MYNODE_PACKAGE_PATH="$build_dir/canary/dist/canary.tar.gz"

    print_info "Preparing temporary myNode build directory: $build_dir"
    rm -rf "$build_dir"
    mkdir -p "$build_dir"
    cp -R "$MYNODE_CANARY_APP_DIR" "$build_dir/canary"

    print_info "Building myNode package from temporary directory..."
    (cd "$build_dir" && mynode-sdk build canary)

    if [[ ! -f "$MYNODE_PACKAGE_PATH" ]]; then
        print_error "myNode package was not created at $MYNODE_PACKAGE_PATH"
        exit 1
    fi

    local pkg_size=$(du -h "$MYNODE_PACKAGE_PATH" | cut -f1)
    print_success "Built $MYNODE_PACKAGE_PATH ($pkg_size)"
}

# Phase 9: myNode Testing
phase_mynode_testing() {
    print_header "Phase 9: myNode Testing"

    if [[ "$SKIP_MYNODE" == true ]]; then
        print_warning "Skipping myNode testing (--skip-mynode)"
        return
    fi

    if [[ "$SKIP_MYNODE_TEST" == true ]]; then
        print_warning "Skipping live myNode testing (--skip-mynode-test); build and PR phases remain enabled"
        return
    fi

    if [[ "$DRY_RUN" == true ]]; then
        print_warning "[DRY RUN] Would test myNode package"
        return
    fi

    echo ""
    ping_host_or_retry "myNode" "$MYNODE_HOST"

    print_info "Checking deployed myNode web/app files..."
    if verify_mynode_local_source_sync "$MYNODE_HOST"; then
        print_success "myNode already has the exact local release sources; skipping rootfs copy"
    elif ! run_mynode_local_update "$MYNODE_HOST" www; then
        if ! recover_mynode_local_update "$MYNODE_HOST"; then
            print_mynode_local_update_recovery "$MYNODE_HOST"
            exit 1
        fi
    elif ! verify_mynode_local_source_sync "$MYNODE_HOST"; then
        print_error "myNode local source update completed, but deployed source hashes do not match"
        print_mynode_local_update_recovery "$MYNODE_HOST"
        exit 1
    fi

    print_info "Testing instructions:"
    if [[ -z "$MYNODE_PACKAGE_PATH" ]]; then
        MYNODE_PACKAGE_PATH="/tmp/canary-mynode-release-$NEW_VERSION/canary/dist/canary.tar.gz"
    fi
    echo "  1. Open: http://$MYNODE_HOST/marketplace"
    echo "  2. Open Canary from the Marketplace"
    echo "  3. Install Canary, or Reinstall if it is already installed"
    echo "  4. Wait for the myNode install/reinstall flow and reboot to complete"
    echo "  5. Confirm the app reports Installed Version v$NEW_VERSION and Latest Version v$NEW_VERSION"
    echo "  6. Confirm canary.service and the Canary containers reach a healthy/running state"
    echo "  7. Open Canary from myNode"
    echo "  8. Smoke-test the release-specific user flow for v$NEW_VERSION"
    echo "  9. Restart Canary and confirm the app still starts cleanly"
    echo ""
    echo "  Built tarball for packaging sanity check: $MYNODE_PACKAGE_PATH"
    echo ""

    if ! confirm "Did myNode testing pass successfully?"; then
        print_error "myNode testing failed"
        echo ""
        print_info "You can retry from Phase 8 (myNode build): scripts/release.sh $NEW_VERSION --from-phase=8"
        exit 1
    fi

    run_node_browser_auth_gate "mynode"
    record_release_gate "mynode" "$MYNODE_REPO"

    print_success "Phase 9 complete: myNode local testing passed"
}

# Phase 12: myNode PR
phase_mynode_pr() {
    print_header "Phase 12: myNode PR"

    if [[ "$SKIP_MYNODE" == true ]]; then
        print_warning "Skipping myNode PR (--skip-mynode)"
        return
    fi

    if [[ "$DRY_RUN" == true ]]; then
        print_warning "[DRY RUN] Would commit/push mynode and create draft PR"
        return
    fi

    ensure_node_distro_release_notes_for "mynode" "myNode"

    prepare_mynode_repo
    cd "$MYNODE_REPO"

    require_clean_repo "mynode"

    local body_file pr_url
    body_file=$(mktemp)
    write_node_distro_pr_body \
        "$body_file" \
        "myNode app package" \
        "- [x] Built with \`mynode-sdk build canary\`
- [x] Upgraded a local myNode installation from its previously published Canary package directly to v$NEW_VERSION
- [x] Verified both wallets, contacts, destinations, settings, and notification history were preserved
- [x] Verified the exact release image digests, database integrity, and restart without duplicate delivery"
    pr_url=$(create_or_update_draft_pr \
        "mynodebtc/mynode" \
        "schjonhaug:$MYNODE_RELEASE_BRANCH" \
        "master" \
        "Update Canary to v$NEW_VERSION" \
        "$body_file")
    rm -f "$body_file"

    print_success "Draft PR ready: $pr_url"
}

# Phase 13: Tag and GitHub Release
phase_github_release() {
    print_header "Phase 13: Tag and GitHub Release"

    local tag="v$NEW_VERSION"

    if [[ "$DRY_RUN" == true ]]; then
        print_warning "[DRY RUN] Would create GitHub release for $tag"
        return
    fi

    cd "$CANARY_REPO"

    ensure_canary_version_bump_merged
    verify_release_gates

    # Create and push tag only after all distro smoke tests pass.
    if git rev-parse "$tag" >/dev/null 2>&1; then
        print_info "Tag $tag already exists locally"
    else
        print_info "Creating tag $tag..."
        git tag "$tag"
        print_success "Created tag $tag"
    fi

    if git ls-remote --tags origin | grep -q "refs/tags/$tag"; then
        print_info "Tag $tag already exists on remote"
    else
        print_info "Pushing tag to origin..."
        git push origin "$tag"
        print_success "Pushed tag to origin"
    fi

    # Generate release notes with Codex
    print_info "Generating release notes..."
    local prev_tag
    prev_tag=$(get_previous_release_tag_for "$tag" || true)
    local commits=""
    if [[ -n "$prev_tag" ]]; then
        commits=$(git log "$prev_tag".."$tag" --oneline)
    else
        commits=$(git log --oneline -10)
    fi
    local contributors
    contributors=$(collect_release_contributors "$prev_tag" "$tag" "schjonhaug")

    local release_body=""
    if command -v codex &> /dev/null; then
        print_info "Using Codex to generate release notes..."
        local codex_output
        codex_output=$(mktemp)
        codex exec -C "$CANARY_REPO" --sandbox read-only --ask-for-approval never --output-last-message "$codex_output" "Generate GitHub release notes for Canary based only on commits from ${prev_tag:-the previous release} to $tag. Write for users installing/upgrading Canary, not maintainers. Match the established Canary release style exactly: start directly with section headers like ## What's New, ## Improvements, ## Bug Fixes, ## Security, and ## Contributors; never use ## What's Changed. Every user-facing bullet must include attribution to a PR number or issue number, for example (#123) or (#123; fixes #122). Do not mention, tag, or thank @schjonhaug. Be concise. Focus on SELF-HOSTED user-facing changes in the Canary app itself. IGNORE release/admin/internal commits such as version bumps, metadata-only fixes, README screenshot refreshes, CI/build/dependency housekeeping, Docker deploy fixes, public landing/listing page polish, and app-store PR mechanics. IGNORE Cloud/billing/Stripe/SaaS commits unless they affect self-hosted releases. Mention Umbrel, StartOS, or myNode only when the change affects users at runtime or during install/upgrade on that node distro; do not mention distro public listing/landing-page changes by default. Before writing Contributors, inspect the contributor context plus the PRs/issues referenced there. Credit external PR contributors and external issue reporters when their report matches a fixed user-facing issue, even if the PR did not formally use 'fixes #123'. If there are no external contributors or reporters, write '- No external contributors in this release.' End with the full changelog comparison link when a previous tag exists. Contributor context: $contributors Commits: $commits" >/dev/null 2>&1 || true
        release_body=$(cat "$codex_output" 2>/dev/null || echo "")
        rm -f "$codex_output"
    fi

    if release_notes_require_manual_review "$release_body"; then
        print_warning "Generated release notes were missing or did not match Canary release style"
        release_body=$(generate_basic_github_release_notes "$prev_tag" "$tag" "schjonhaug" || true)
    fi

    if release_notes_require_manual_review "$release_body"; then
        print_warning "Could not generate acceptable GitHub release notes automatically"
        release_body=$(read_multiline_input "Enter GitHub release notes. Use Canary section headers, include PR/issue refs, and do not mention @schjonhaug:")
    fi

    if release_notes_require_manual_review "$release_body"; then
        print_error "Release notes still do not match Canary release rules"
        print_info "Required: section headers like ## What's New, no ## What's Changed, and no @schjonhaug mention."
        exit 1
    fi

    # Save to temp file
    local temp_file=$(mktemp)
    echo "$release_body" > "$temp_file"
    GITHUB_RELEASE_NOTES="$release_body"
    save_github_release_notes "$GITHUB_RELEASE_NOTES"

    print_info "Generated release notes:"
    echo "$release_body"
    echo ""

    if ! confirm "Use these release notes?"; then
        local custom_file=""
        read -p "Enter custom GitHub release notes file path, or leave blank to edit notes manually: " custom_file
        if [[ -n "$custom_file" ]]; then
            GITHUB_RELEASE_NOTES="$(cat "$custom_file")"
        else
            GITHUB_RELEASE_NOTES="$(read_multiline_input "Enter custom GitHub release notes:")"
        fi

        if release_notes_require_manual_review "$GITHUB_RELEASE_NOTES"; then
            print_error "Custom release notes do not match Canary release rules"
            rm "$temp_file"
            exit 1
        fi

        echo "$GITHUB_RELEASE_NOTES" > "$temp_file"
        save_github_release_notes "$GITHUB_RELEASE_NOTES"
    fi

    local existing_release_draft
    existing_release_draft=$(gh release view "$tag" --repo schjonhaug/canary --json isDraft --jq '.isDraft' 2>/dev/null || true)
    if [[ "$existing_release_draft" == "true" ]]; then
        print_info "Publishing the existing GitHub draft for $tag..."
        gh release edit "$tag" --repo schjonhaug/canary --title "v$NEW_VERSION" --notes-file "$temp_file" --draft=false --latest
    elif [[ "$existing_release_draft" == "false" ]]; then
        print_warning "GitHub release $tag is already published; leaving it unchanged"
    else
        gh release create "$tag" --repo schjonhaug/canary --title "v$NEW_VERSION" --notes-file "$temp_file"
    fi

    rm "$temp_file"

    print_success "GitHub release created"
    print_info "Release URL: https://github.com/schjonhaug/canary/releases/tag/$tag"

    print_success "Phase 13 complete: GitHub release published"
}

x_post_draft_fallback() {
    local release_url="https://github.com/schjonhaug/canary/releases/tag/v$NEW_VERSION"

    cat <<EOF
Canary Wallet v$NEW_VERSION is here 🐤

The latest self-hosted wallet monitoring improvements, notification controls, and fixes are ready.

Release notes: $release_url
EOF
}

generate_x_post_draft() {
    local release_url="https://github.com/schjonhaug/canary/releases/tag/v$NEW_VERSION"
    local release_body="${GITHUB_RELEASE_NOTES:-}"
    local draft=""

    if [[ "$DRY_RUN" != true && -z "$release_body" ]]; then
        release_body=$(gh release view "v$NEW_VERSION" --repo schjonhaug/canary --json body --jq '.body' 2>/dev/null || true)
    fi

    if [[ "$DRY_RUN" != true && -n "$release_body" ]] && command -v codex &> /dev/null; then
        local codex_output
        codex_output=$(mktemp)
        codex exec -C "$CANARY_REPO" --sandbox read-only --ask-for-approval never --output-last-message "$codex_output" "Write one copy-ready X post announcing Canary Wallet v$NEW_VERSION. It MUST be 280 Unicode characters or fewer including the exact release URL below. Return plain text only: no quotes, markdown fences, commentary, or hashtags. Start with exactly 'Canary Wallet v$NEW_VERSION is here 🐤'. Summarize two to four of the most important user-facing self-hosted changes from the release notes, using concise emoji bullets when useful. End with exactly 'Release notes: $release_url'. Do not invent claims. Release notes: $release_body" >/dev/null 2>&1 || true
        draft=$(cat "$codex_output" 2>/dev/null || true)
        rm -f "$codex_output"
    fi

    if [[ -z "$draft" || ${#draft} -gt 280 || "$draft" != *"Canary Wallet v$NEW_VERSION is here 🐤"* || "$draft" != *"$release_url"* ]]; then
        draft=$(x_post_draft_fallback)
    fi

    printf '%s\n' "$draft"
}

# Keep helpers sourceable for isolated release-script tests.
if [[ "${BASH_SOURCE[0]}" != "$0" ]]; then
    return 0
fi

# Main
usage() {
    echo "Usage: $0 <version|major|minor|patch> [options]"
    echo ""
    echo "Arguments:"
    echo "  version     Explicit version (e.g., 1.2.3)"
    echo "  major       Increment major version"
    echo "  minor       Increment minor version"
    echo "  patch       Increment patch version"
    echo ""
    echo "Options:"
    echo "  --dry-run       Preview changes without executing"
    echo "  --skip-docker   Skip Docker build phase"
    echo "  --skip-rsync    Skip rsync testing phase"
    echo "  --skip-start9   Skip Start9 build/test/PR phases"
    echo "  --skip-mynode   Skip myNode build/test/PR phases"
    echo "  --skip-mynode-test   Skip only the live myNode test/browser gate; still build and open/update its PR"
    echo "  --umbrel-branch <branch>   Umbrel draft PR branch to update (auto-detected if omitted)"
    echo "  --start9-branch <branch>   StartOS draft PR branch to update (auto-detected if omitted)"
    echo "  --mynode-branch <branch>   myNode draft PR branch to update (auto-detected if omitted)"
    echo "  --from-phase=N  Start from phase N (1-13)"
    echo "                   Phase 13 requires the exact-candidate gate set recorded during Phase 2"
    echo ""
    echo "Phases:"
    echo "  1: Canary version bump & release screenshots"
    echo "  2: Docker image build & push"
    echo "  3: Start9 version bump"
    echo "  4: Start9 build"
    echo "  5: Start9 testing"
    echo "  6: Umbrel app update"
    echo "  7: Umbrel testing"
    echo "  8: myNode version bump & build"
    echo "  9: myNode testing"
    echo "  10: Start9 PR"
    echo "  11: Umbrel PR"
    echo "  12: myNode PR"
    echo "  13: Tag and GitHub release"
    echo ""
    echo "Examples:"
    echo "  $0 1.2.0                  # Release version 1.2.0"
    echo "  $0 minor                  # Increment minor version"
    echo "  $0 1.2.0 --dry-run        # Preview release"
    echo "  $0 1.2.0 --from-phase=4   # Resume from Start9 build"
    echo "  $0 1.2.0 --skip-start9    # Skip Start9"
    echo "  $0 1.2.0 --skip-mynode    # Skip myNode"
    echo "  $0 1.2.0 --skip-mynode-test # Package myNode without a live upgrade gate"
    echo "  $0 1.2.0 --umbrel-branch my-branch --start9-branch my-startos-branch --mynode-branch my-mynode-branch"
}

# Parse arguments
if [[ $# -lt 1 ]]; then
    usage
    exit 1
fi

VERSION_ARG="$1"
shift

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        --skip-docker)
            SKIP_DOCKER=true
            shift
            ;;
        --skip-rsync)
            SKIP_RSYNC=true
            shift
            ;;
        --skip-start9)
            SKIP_START9=true
            shift
            ;;
        --skip-mynode)
            SKIP_MYNODE=true
            shift
            ;;
        --skip-mynode-test)
            SKIP_MYNODE_TEST=true
            shift
            ;;
        --umbrel-branch)
            [[ $# -ge 2 ]] || { echo "--umbrel-branch requires a value"; exit 1; }
            UMBREL_RELEASE_BRANCH="$2"
            shift 2
            ;;
        --start9-branch)
            [[ $# -ge 2 ]] || { echo "--start9-branch requires a value"; exit 1; }
            START9_RELEASE_BRANCH="$2"
            shift 2
            ;;
        --mynode-branch)
            [[ $# -ge 2 ]] || { echo "--mynode-branch requires a value"; exit 1; }
            MYNODE_RELEASE_BRANCH="$2"
            shift 2
            ;;
        --from-phase=*)
            FROM_PHASE="${1#*=}"
            shift
            ;;
        *)
            echo "Unknown option: $1"
            usage
            exit 1
            ;;
    esac
done

# Determine version
CURRENT_VERSION=$(get_current_version)

case "$VERSION_ARG" in
    major|minor|patch)
        NEW_VERSION=$(increment_version "$CURRENT_VERSION" "$VERSION_ARG")
        ;;
    *)
        NEW_VERSION="$VERSION_ARG"
        ;;
esac

validate_version "$NEW_VERSION"
load_app_store_release_notes

# Print banner
echo ""
echo -e "${GREEN}╔═══════════════════════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║                 CANARY RELEASE SCRIPT                     ║${NC}"
echo -e "${GREEN}║                   Version $NEW_VERSION                          ║${NC}"
echo -e "${GREEN}╚═══════════════════════════════════════════════════════════╝${NC}"
echo ""

if [[ "$DRY_RUN" == true ]]; then
    print_warning "DRY RUN MODE - No changes will be made"
fi

# Check prerequisites (always)
check_prerequisites

# Pre-mark skipped phases
for i in $(seq 1 13); do
    [[ $FROM_PHASE -gt $i ]] && PHASE_STATUS[$((i - 1))]="skipped"
done
[[ "$SKIP_DOCKER" == true ]] && PHASE_STATUS[1]="skipped"
[[ "$SKIP_RSYNC" == true ]] && PHASE_STATUS[6]="skipped"
if [[ "$SKIP_START9" == true ]]; then
    PHASE_STATUS[2]="skipped"
    PHASE_STATUS[3]="skipped"
    PHASE_STATUS[4]="skipped"
    PHASE_STATUS[9]="skipped"
fi
if [[ "$SKIP_MYNODE" == true ]]; then
    PHASE_STATUS[7]="skipped"
    PHASE_STATUS[8]="skipped"
    PHASE_STATUS[11]="skipped"
fi

# Confirm before starting
echo ""
print_info "Release plan: $CURRENT_VERSION → $NEW_VERSION"
needs_umbrel_repo && print_info "Umbrel branch: $UMBREL_RELEASE_BRANCH"
needs_start9_repo && print_info "StartOS branch: $START9_RELEASE_BRANCH"
needs_mynode_repo && print_info "myNode branch: $MYNODE_RELEASE_BRANCH"
echo ""
print_phase_list
echo ""

if ! confirm "Proceed with release?"; then
    echo "Aborted."
    exit 0
fi

# Execute phases
run_phase 1 phase_version_bump
run_phase 2 phase_docker_build
run_phase 3 phase_start9_version_bump
run_phase 4 phase_start9_build
run_phase 5 phase_start9_testing
run_phase 6 phase_umbrel_update
run_phase 7 phase_local_testing
run_phase 8 phase_mynode_build
run_phase 9 phase_mynode_testing
run_phase 10 phase_start9_submission
run_phase 11 phase_umbrel_pr
run_phase 12 phase_mynode_pr
run_phase 13 phase_github_release

if needs_umbrel_repo; then
    cleanup_node_distro_work_branch \
        "$UMBREL_APPS_REPO" \
        "umbrel-apps" \
        "fork" \
        "schjonhaug/umbrel-apps"
fi

if needs_start9_repo; then
    cleanup_node_distro_work_branch \
        "$CANARY_STARTOS_REPO" \
        "canary-startos" \
        "origin" \
        "$START9_HEAD_REPO"
fi

if needs_mynode_repo; then
    cleanup_node_distro_work_branch \
        "$MYNODE_REPO" \
        "mynode" \
        "origin" \
        "schjonhaug/mynode"
fi

# Final summary
print_header "Release Complete!"
echo -e "${GREEN}Version $NEW_VERSION has been released successfully!${NC}"
echo ""
echo "Summary:"
echo "  - Version bumped in canary repo"
echo "  - README, myNode, and Umbrel gallery screenshots refreshed"
echo "  - Docker images pushed to Docker Hub"
if [[ "$SKIP_START9" != true ]]; then
echo "  - Start9 s9pk package built and tested"
echo "  - Draft PR ready for Start9"
fi
if [[ "$SKIP_MYNODE" != true ]]; then
echo "  - myNode package built and tested"
echo "  - Draft PR ready for myNode"
fi
echo "  - Draft PR ready for Umbrel"
echo "  - Umbrel gallery screenshots refreshed and submitted"
echo "  - Git tag v$NEW_VERSION created"
echo "  - GitHub release published"
echo ""
echo "Next steps:"
echo "  - Review and mark the Umbrel draft PR as ready for review"
echo "  - Review and mark the Umbrel gallery draft PR as ready for review"
if [[ "$SKIP_MYNODE" != true ]]; then
echo "  - Review and mark the myNode draft PR as ready for review"
fi
echo "  - Monitor the Umbrel PR for merge"
if [[ "$SKIP_START9" != true ]]; then
echo "  - Review and mark the Start9 draft PR as ready for review"
echo "  - After Start9 merges it, their workflow will tag and publish the StartOS package"
echo "  - Sync schjonhaug/canary-startos master from Start9-Community/master after merge"
fi
echo "  - Announce the release if needed"
echo ""

print_header "X Post Draft"
generate_x_post_draft
echo ""
