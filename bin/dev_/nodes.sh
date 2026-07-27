#!/bin/bash
# Node lifecycle + tmux session management.

TMUX_SESSION="zim-dev"
ZIM_BIN="${ZIM_BIN:-$PROJECT_ROOT/target/debug/zim}"

cmd_clean() {
    # Wiping state under running services is undefined behavior — the hub
    # keeps serving from deleted sqlite handles and half-recreates its
    # data dir; daemons ditto. Stop everything (incl. orphans holding
    # dev-band ports) before touching disk.
    cmd_kill --force
    echo -e "${YELLOW}Cleaning dev data...${NC}"
    for node in $(get_node_names); do
        rm -rf "$DATA_DIR/$node"
    done
    # Fixture mountpoints (data/mnt-*) are plain dirs once unmounted.
    rm -rf "$DATA_DIR"/mnt-* 2>/dev/null || true
    # Also wipe the hub's state DB (users, user_peers, escrow, blob index).
    # Daemons get fresh identities above; if the hub roster survived it would
    # keep their *old* keys, and every browser-created vault would then be
    # shared with those dead keys — poisoning sync. The minio bucket is
    # separate; orphaned blobs there are harmless.
    rm -rf "$DATA_DIR/zim-hub"
    echo -e "${GREEN}Done${NC}"
}

cmd_kill() {
    local force=false
    [[ "$1" == "--force" || "$1" == "-f" ]] && force=true

    echo -e "${YELLOW}Killing $TMUX_SESSION tmux session...${NC}"
    if tmux kill-session -t "$TMUX_SESSION" 2>/dev/null; then
        echo -e "${GREEN}Done${NC}"
    else
        echo -e "${YELLOW}No session found${NC}"
    fi

    if $force; then
        echo -e "${YELLOW}Force killing processes on dev ports...${NC}"
        kill_dev_ports
    fi
}

kill_dev_ports() {
    local killed=0
    # Daemon ports + the hub port. The hub runs in its own tmux window
    # under `cargo watch`/confit, which can outlive `kill-session` — so
    # without this a stale hub keeps holding :17190 and the next `hub up`
    # fails to bind.
    local ports
    ports=$(for node in $(get_node_names); do get_api_port "$node"; done)
    ports="$ports ${HUB_PORT:-17190}"
    for port in $ports; do
        local pid=$(lsof -ti tcp:"$port" 2>/dev/null)
        if [[ -n "$pid" ]]; then
            echo -e "  killing pid $pid on port $port"
            kill -9 $pid 2>/dev/null && killed=$((killed + 1))
        fi
    done
    if (( killed == 0 )); then
        echo -e "${GREEN}No orphaned processes found${NC}"
    else
        echo -e "${GREEN}Killed $killed process(es)${NC}"
    fi
}

cmd_status() {
    echo -e "${BLUE}Dev environment status${NC}"
    echo ""

    if tmux has-session -t "$TMUX_SESSION" 2>/dev/null; then
        echo -e "tmux session: ${GREEN}running${NC} ($TMUX_SESSION)"
    else
        echo -e "tmux session: ${YELLOW}not running${NC}"
    fi
    echo ""

    printf "%-10s %-7s %-10s %s\n" "PEER" "PORT" "STATUS" "ZIM_HOME"
    for node in $(get_node_names); do
        local port=$(get_api_port "$node")
        local home=$(get_data_dir "$node")
        local status="${RED}down${NC}"
        if curl -sf "http://127.0.0.1:$port/_status/livez" >/dev/null 2>&1; then
            status="${GREEN}up${NC}"
        fi
        printf "%-10s %-7s %-20b %s\n" "$node" "$port" "$status" "$home"
    done
}

# Cargo features for the dev daemon build:
#   - `hub` is ALWAYS on — the dev workflow (`hub up/enroll`, `--hub`, and
#     `zim hub peers sync`) needs the `zim hub` subcommand. It's cheap.
#   - `fuse` is opt-in via `./bin/dev run --fuse` (or `ZIM_DEV_FUSE=1`), and
#     only when the native lib is present. Export `ZIM_DEV_FUSE=1` to make it
#     sticky across `cli`/`shell`/`seed` (avoids fuse/non-fuse rebuild churn).
zim_build_features() {
    local feats="hub"
    if [[ -n "${ZIM_DEV_FUSE:-}" ]] && fuse_lib_present; then
        feats="$feats,fuse"
    fi
    echo "--features $feats"
}

# The crate `src/` dirs the daemon compiles from. `cargo watch` rebuilds +
# restarts a node when any of these change, so daemons track our edits live
# (same idea as the hub's `make hub`). Scoped to the daemon's own crates so an
# unrelated wasm/hub-only edit doesn't needlessly bounce the daemons (and lose
# their iroh discovery state).
zim_watch_dirs() {
    echo "-w crates/zim/src -w crates/zim-peer/src -w crates/zim-core/src -w crates/zim-crypto/src -w crates/zim-did/src"
}

# True when the platform's FUSE lib is installed (macFUSE / libfuse).
fuse_lib_present() {
    if [[ "$(uname -s)" == "Darwin" && -d /Library/Filesystems/macfuse.fs ]]; then
        return 0
    fi
    pkg-config --exists fuse3 2>/dev/null || pkg-config --exists fuse 2>/dev/null
}

# True when FUSE was requested but no native lib is installed to build it.
fuse_requested_but_missing() {
    [[ -n "${ZIM_DEV_FUSE:-}" ]] && ! fuse_lib_present
}

# Build the zim binary up-front so each pane doesn't redo it. Always runs (it's
# incremental — near-instant when up to date) so the binary reflects the
# current source + the fuse feature, rather than a stale earlier build.
ensure_zim_built() {
    local features
    features="$(zim_build_features)"
    # Status to stderr so `eval "$(./bin/dev shell)"` (which captures stdout)
    # only ever sees the function definitions, never this build line.
    echo -e "${BLUE}Building zim ${features:+($features) }...${NC}" >&2
    # shellcheck disable=SC2086
    (cd "$PROJECT_ROOT" && cargo build -p zim $features --quiet) || {
        echo -e "${RED}cargo build failed${NC}" >&2
        exit 1
    }
}

# Block until every node's daemon answers /_status/livez (or time out).
wait_for_daemons() {
    local tries=0
    while true; do
        local all_up=true
        for node in $(get_node_names); do
            local port
            port=$(get_api_port "$node")
            if ! curl -sf "http://127.0.0.1:$port/_status/livez" >/dev/null 2>&1; then
                all_up=false
            fi
        done
        if $all_up; then return 0; fi
        tries=$((tries + 1))
        if (( tries > 30 )); then return 1; fi
        sleep 1
    done
}

# Block until the hub serves its DID document (i.e. it's booted).
wait_for_hub() {
    local tries=0
    until curl -sf "$(hub_url)/.well-known/did.json" >/dev/null 2>&1; do
        tries=$((tries + 1))
        if (( tries > 90 )); then return 1; fi
        sleep 1
    done
    return 0
}

# `--hub` end-to-end: once the daemons are up, start the hub, seed vault
# fixtures, and enroll the daemons (stands in for `zim login`). Leaves a
# fully-wired environment you only have to sign into.
orchestrate_hub() {
    echo -e "${BLUE}[--hub] waiting for daemons…${NC}"
    if ! wait_for_daemons; then
        echo -e "${RED}[--hub] daemons did not come up — skipping hub setup${NC}"
        return 1
    fi

    echo -e "${BLUE}[--hub] starting hub (minio + zim-hub)…${NC}"
    hub_up

    echo -e "${BLUE}[--hub] waiting for hub to boot…${NC}"
    if ! wait_for_hub; then
        echo -e "${YELLOW}[--hub] hub not reachable yet — check the 'hub' tmux window${NC}"
    fi

    echo -e "${BLUE}[--hub] seeding vault fixtures…${NC}"
    cmd_seed

    echo -e "${BLUE}[--hub] enrolling daemons into the hub…${NC}"
    hub_enroll

    echo -e "${GREEN}[--hub] ready.${NC} Open $(hub_url), sign in as ${SEED_EMAIL}, mint your web key."
}

cmd_run() {
    local background=false hub=false
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --background|-b) background=true ;;
            --fuse)          export ZIM_DEV_FUSE=1 ;;
            --hub)           hub=true ;;
            *) echo -e "${RED}unknown run arg: $1${NC}"; exit 1 ;;
        esac
        shift
    done

    if fuse_requested_but_missing; then
        echo -e "${RED}--fuse requested but no macFUSE/libfuse found${NC}"
        echo "  install macFUSE (macOS) or libfuse3-dev (Linux), or drop --fuse"
        exit 1
    fi

    if ! command -v tmux >/dev/null 2>&1; then
        echo -e "${RED}tmux not installed${NC}"
        exit 1
    fi

    if tmux has-session -t "$TMUX_SESSION" 2>/dev/null; then
        echo -e "${YELLOW}Session $TMUX_SESSION already running. Attach with: tmux attach -t $TMUX_SESSION${NC}"
        if $hub; then
            echo -e "${YELLOW}(already running — run hub setup by hand: ./bin/dev hub up && ./bin/dev seed && ./bin/dev hub enroll)${NC}"
        fi
        $background || tmux attach -t "$TMUX_SESSION"
        return 0
    fi

    ensure_zim_built

    mkdir -p "$DATA_DIR"
    seed_node_configs

    local nodes=($(get_node_names))
    local n=${#nodes[@]}

    # Each node runs under `cargo watch` so editing daemon/sync source
    # rebuilds + restarts it live — the harness tracks our edits.
    local features watch_dirs
    features="$(zim_build_features)"
    watch_dirs="$(zim_watch_dirs)"

    # Window 0: a pane per node.
    tmux new-session -d -s "$TMUX_SESSION" -n nodes

    for (( i = 1; i < n; i++ )); do
        tmux split-window -v -t "$TMUX_SESSION:0"
    done
    tmux select-layout -t "$TMUX_SESSION:0" even-vertical >/dev/null

    for (( i = 0; i < n; i++ )); do
        local node="${nodes[$i]}"
        local port=$(get_api_port "$node")
        local home=$(get_data_dir "$node")
        local header="${GREEN}=== $node on :$port ===${NC}"

        tmux send-keys -t "$TMUX_SESSION:0.$i" \
            "cd $PROJECT_ROOT && printf '%b\n' '$header' && ZIM_HOME='$home' ZIM_LOG='\${ZIM_LOG:-zim=info,zim_peer=info}' cargo watch $watch_dirs -x 'run -p zim $features -- daemon run --port $port'" \
            C-m
    done

    # Window 1: one shell pane per node. Each pane pins $ZIM_HOME so a
    # bare `zim …` targets that peer, and sources the dev shell functions
    # so you can also reach the others (`alice …`, `bob …`). The function
    # set is generated once (stdout only — see ensure_zim_built) and each
    # pane just sources it.
    local devrc="$DATA_DIR/.dev-shell.sh"
    cmd_shell >"$devrc" 2>/dev/null || true

    tmux new-window -t "$TMUX_SESSION:1" -n cli
    for (( i = 1; i < n; i++ )); do
        tmux split-window -v -t "$TMUX_SESSION:1"
    done
    tmux select-layout -t "$TMUX_SESSION:1" even-vertical >/dev/null

    for (( i = 0; i < n; i++ )); do
        local node="${nodes[$i]}"
        local home=$(get_data_dir "$node")
        tmux send-keys -t "$TMUX_SESSION:1.$i" \
            "cd $PROJECT_ROOT && export ZIM_HOME='$home' && source '$devrc' && printf '%b\n' '${GREEN}=== $node ready — bare \"zim …\" targets $node ===${NC}'" \
            C-m
    done

    # `--hub`: bring up + wire the hub before we hand the terminal back.
    if $hub; then
        orchestrate_hub || true
    fi

    # Show the nodes window first.
    tmux select-window -t "$TMUX_SESSION:0"

    if $background; then
        echo -e "${GREEN}Session $TMUX_SESSION started in background.${NC}"
        echo "Attach with: tmux attach -t $TMUX_SESSION"
        echo "Or run commands: ./bin/dev cli <nick> <args>"
    else
        tmux attach -t "$TMUX_SESSION"
    fi
}

# `./bin/dev cli <nick> <args>` — run zim against the named peer.
# Sets $ZIM_HOME; the CLI reads the per-peer port from its
# config.toml (seeded by `seed_node_configs`).
cmd_cli() {
    if [[ -z "$1" ]]; then
        echo -e "${RED}Usage: ./bin/dev cli <nick> <args...>${NC}"
        echo "Available nicks:"
        for node in $(get_node_names); do
            echo "  $node (port $(get_api_port "$node"))"
        done
        exit 1
    fi

    local nick="$1"
    shift

    if ! get_api_port "$nick" >/dev/null 2>&1; then
        echo -e "${RED}Unknown nick: $nick${NC}"
        exit 1
    fi
    local home=$(get_data_dir "$nick")

    # `cargo run`, not the prebuilt binary, so the CLI always reflects the
    # current source. Fast in practice: the node watchers keep `zim` built, so
    # this is usually a no-op compile + run. `--quiet` hides the cargo line.
    local features
    features="$(zim_build_features)"
    # shellcheck disable=SC2086
    (cd "$PROJECT_ROOT" && ZIM_HOME="$home" cargo run -p zim $features --quiet -- "$@")
}

# Seed each node's $ZIM_HOME with a config.toml that pins its
# `api_port`. Idempotent — overwrites the file every time so changes
# to nodes.toml propagate. Shared by `cmd_run` and `cmd_shell` so the
# CLI resolves the right daemon port regardless of whether the user
# started the tmux session first.
seed_node_configs() {
    for node in $(get_node_names); do
        local home=$(get_data_dir "$node")
        local port=$(get_api_port "$node")
        mkdir -p "$home"
        printf 'api_port = %s\nlog_level = "info"\n' "$port" > "$home/config.toml"
    done
}

# `./bin/dev shell` — emit shell functions, one per nick, that wrap
# the zim binary with the right $ZIM_HOME baked in. Activate in the
# current terminal with:
#
#     eval "$(./bin/dev shell)"
#
# After that, `alice vault head demo` and `bob vault list` just work.
# Regenerate after editing nodes.toml. Seeds each node's config.toml
# so commands don't accidentally hit the default port 17171 because a
# config wasn't written yet.
cmd_shell() {
    seed_node_configs
    local features
    features="$(zim_build_features)"
    echo "# zim per-peer shells — generated $(date '+%Y-%m-%d %H:%M:%S')"
    echo "# eval \"\$(./bin/dev shell)\" to activate"
    for node in $(get_node_names); do
        local home=$(get_data_dir "$node")
        # `cargo run` so a bare `alice …` reflects current source (the node
        # watchers keep it built). Subshell `cd` so it works from any dir.
        printf '%s() { ( cd %q && ZIM_HOME=%q cargo run -p zim %s --quiet -- "$@" ); }\n' \
            "$node" "$PROJECT_ROOT" "$home" "$features"
    done
}

# `./bin/dev attach` (alias `a`) — attach to the running tmux session.
# Windows: `nodes` (daemon panes), `cli` (per-node shells), `hub`. Detach
# with `Ctrl-b d`.
cmd_attach() {
    if ! tmux has-session -t "$TMUX_SESSION" 2>/dev/null; then
        echo -e "${RED}No session '$TMUX_SESSION' — start it with: ./bin/dev run -b${NC}"
        exit 1
    fi
    tmux attach -t "$TMUX_SESSION"
}

# `./bin/dev logs <nick> [-f]` — the node's daemon log. Reads the clean
# `$ZIM_HOME/state/daemon.log` file (tracing output only), not the tmux pane,
# which under `cargo watch` is interleaved with build output. `-f` follows.
cmd_logs() {
    if [[ -z "$1" ]]; then
        echo -e "${RED}Usage: ./bin/dev logs <nick> [-f]${NC}"
        exit 1
    fi
    local nick="$1"
    if ! get_api_port "$nick" >/dev/null 2>&1; then
        echo -e "${RED}Unknown nick: $nick${NC}"
        exit 1
    fi
    local log
    log="$(get_data_dir "$nick")/state/daemon.log"
    if [[ ! -f "$log" ]]; then
        echo -e "${RED}No log yet at $log — is $nick running? (./bin/dev run -b)${NC}" >&2
        exit 1
    fi
    if [[ "${2:-}" == "-f" ]]; then
        tail -f "$log"
    else
        tail -n "${ZIM_DEV_LOG_LINES:-200}" "$log"
    fi
}
