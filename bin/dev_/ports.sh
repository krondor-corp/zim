#!/bin/bash
# Per-worktree ports and names, from confit.toml's `[env.dev]`.
#
# confit gives each worktree of this clone a slot (0 on `main`, else
# the lowest free 1-9, recorded in <git-common-dir>/confit/ports.toml)
# and each service lane a port at band + lane + slot. Everything a
# worktree's dev env owns derives from that, so several worktrees run
# side by side:
#   - daemon ports    ZIM_DEV_<NICK>_PORT   (get_api_port)
#   - hub port        HUB_PORT              (+ its did:web and cookie name)
#   - tmux session    zim-dev[-<slug>]
#   - minio bucket    zim-blobs[-<slug>]    (the container itself is shared)
# minio's ports are `ports.infra.*`: the same on every branch.

CONFIT_INSTALL="curl -fsSL https://raw.githubusercontent.com/krondor-corp/confit/main/install.sh | bash"

if ! command -v confit >/dev/null 2>&1; then
    echo -e "${RED}confit not installed (required, like cargo). Install:${NC}" >&2
    echo "  $CONFIT_INSTALL" >&2
    exit 1
fi

# Resolve once; every later step reads the exported vars.
if ! dev_env="$(cd "$PROJECT_ROOT" && confit export --profile dev --format shell)"; then
    echo -e "${RED}confit could not resolve [env.dev] — \`[ports]\` needs confit >= 0.5 (confit update)${NC}" >&2
    exit 1
fi
eval "$dev_env"
unset dev_env

# `main` (slot 0) keeps the bare names; other worktrees suffix the slug.
if [[ "$ZIM_DEV_SLOT" == "0" ]]; then
    TMUX_SESSION="zim-dev"
    HUB_BUCKET="zim-blobs"
else
    TMUX_SESSION="zim-dev-$ZIM_DEV_SLUG"
    # S3 bucket names: lowercase, at most 63 chars, no trailing dash.
    HUB_BUCKET="$(echo "zim-blobs-$ZIM_DEV_SLUG" | tr '[:upper:]' '[:lower:]' | cut -c1-63)"
    HUB_BUCKET="${HUB_BUCKET%-}"
fi

# `./bin/dev ports` — this worktree's slot, ports, and names.
cmd_ports() {
    local branch
    branch="$(git -C "$PROJECT_ROOT" rev-parse --abbrev-ref HEAD 2>/dev/null)"
    echo -e "${BLUE}Dev ports for ${branch:-?} (slot $ZIM_DEV_SLOT)${NC}"
    echo ""
    printf "%-14s %s\n" "SERVICE" "PORT"
    for node in $(get_node_names); do
        printf "%-14s %s\n" "$node" "$(get_api_port "$node")"
    done
    printf "%-14s %s\n" "hub" "$HUB_PORT"
    printf "%-14s %s  (shared)\n" "minio api" "$MINIO_API_PORT"
    printf "%-14s %s  (shared)\n" "minio console" "$MINIO_CONSOLE_PORT"
    echo ""
    printf "%-14s %s\n" "tmux session" "$TMUX_SESSION"
    printf "%-14s %s\n" "hub url" "http://127.0.0.1:$HUB_PORT"
    printf "%-14s %s\n" "minio bucket" "$HUB_BUCKET"
}
