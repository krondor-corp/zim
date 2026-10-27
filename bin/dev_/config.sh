#!/bin/bash
# Node config for the dev environment: confit.toml's `[dev.nodes]`,
# resolved once into $DEV_NODES (`nick=port` lines) by ports.sh.

# Every key in [dev.nodes] is a peer nick. Sorted + dedup'd.
get_node_names() {
    echo "$DEV_NODES" | cut -d= -f1 | sort -u
}

# A node's daemon API port for this worktree. Fails for an unknown
# nick, so callers can use it to validate one.
get_api_port() {
    local port
    port="$(echo "$DEV_NODES" | grep "^$1=" | cut -d= -f2)"
    [[ -n "$port" ]] || return 1
    echo "$port"
}

# Data dir for a node. The nick == the [dev.nodes] key == the directory.
get_data_dir() {
    echo "$DATA_DIR/$1"
}
