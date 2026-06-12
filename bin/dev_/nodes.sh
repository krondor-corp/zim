#!/bin/bash
# Node lifecycle + tmux session management.

TMUX_SESSION="zim-dev"
ZIM_BIN="${ZIM_BIN:-$PROJECT_ROOT/target/debug/zim}"

cmd_clean() {
    echo -e "${YELLOW}Cleaning dev data...${NC}"
    for node in $(get_node_names); do
        rm -rf "$DATA_DIR/$node"
    done
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
    for node in $(get_node_names); do
        local port=$(get_api_port "$node")
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

# Build the zim binary up-front so each pane doesn't redo it.
ensure_zim_built() {
    if [[ ! -x "$ZIM_BIN" ]]; then
        echo -e "${BLUE}Building zim...${NC}"
        (cd "$PROJECT_ROOT" && cargo build -p zim --quiet) || {
            echo -e "${RED}cargo build failed${NC}"
            exit 1
        }
    fi
}

cmd_run() {
    local background=false
    [[ "$1" == "--background" || "$1" == "-b" ]] && background=true

    if ! command -v tmux >/dev/null 2>&1; then
        echo -e "${RED}tmux not installed${NC}"
        exit 1
    fi

    if tmux has-session -t "$TMUX_SESSION" 2>/dev/null; then
        echo -e "${YELLOW}Session $TMUX_SESSION already running. Attach with: tmux attach -t $TMUX_SESSION${NC}"
        $background || tmux attach -t "$TMUX_SESSION"
        return 0
    fi

    ensure_zim_built

    mkdir -p "$DATA_DIR"
    seed_node_configs

    local nodes=($(get_node_names))
    local n=${#nodes[@]}

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
            "cd $PROJECT_ROOT && printf '%b\n' '$header' && ZIM_HOME='$home' ZIM_LOG='\${ZIM_LOG:-zim=info,zim_peer=info}' '$ZIM_BIN' daemon run --port $port" \
            C-m
    done

    # Window 1: a control shell, ready for CLI commands.
    tmux new-window -t "$TMUX_SESSION:1" -n cli "cd $PROJECT_ROOT && exec \$SHELL"

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

    ensure_zim_built
    ZIM_HOME="$home" "$ZIM_BIN" "$@"
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
# After that, `alice vault demo head` and `bob vaults list` just work.
# Regenerate after editing nodes.toml. Seeds each node's config.toml
# so commands don't accidentally hit the default port 17171 because a
# config wasn't written yet.
cmd_shell() {
    ensure_zim_built
    seed_node_configs
    echo "# zim per-peer shells — generated $(date '+%Y-%m-%d %H:%M:%S')"
    echo "# eval \"\$(./bin/dev shell)\" to activate"
    for node in $(get_node_names); do
        local home=$(get_data_dir "$node")
        # Quote $home for safety; $ZIM_BIN is already an absolute path.
        printf '%s() { ZIM_HOME=%q %q "$@"; }\n' "$node" "$home" "$ZIM_BIN"
    done
}

# `./bin/dev logs <nick>` — tail the tmux pane for that node.
cmd_logs() {
    if [[ -z "$1" ]]; then
        echo -e "${RED}Usage: ./bin/dev logs <nick>${NC}"
        exit 1
    fi
    local nick="$1"
    if ! get_api_port "$nick" >/dev/null 2>&1; then
        echo -e "${RED}Unknown nick: $nick${NC}"
        exit 1
    fi
    if ! tmux has-session -t "$TMUX_SESSION" 2>/dev/null; then
        echo -e "${RED}No tmux session — start it with: ./bin/dev${NC}"
        exit 1
    fi
    # Pane index = position in the sorted node list.
    local nodes=($(get_node_names))
    local idx=0
    for (( i = 0; i < ${#nodes[@]}; i++ )); do
        if [[ "${nodes[$i]}" == "$nick" ]]; then
            idx=$i
            break
        fi
    done
    tmux capture-pane -t "$TMUX_SESSION:0.$idx" -p
}
