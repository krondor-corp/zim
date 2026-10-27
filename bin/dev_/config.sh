#!/bin/bash
# Node config for the dev environment: names from nodes.toml, ports
# from this worktree's confit env (see ports.sh).

# Every section name in nodes.toml is a peer nick. Sorted + dedup'd.
get_node_names() {
    grep -E '^\[[a-zA-Z0-9_]+\]$' "$CONFIG_FILE" | tr -d '[]' | sort -u
}

# A node's daemon API port: `ZIM_DEV_<NICK>_PORT`, which confit.toml's
# `[env.dev]` maps from the node's `ports.services.<nick>` lane. Fails
# for a nick that isn't in nodes.toml, so callers can use it to
# validate one.
get_api_port() {
    get_node_names | grep -qx "$1" || return 1
    local var
    var="ZIM_DEV_$(echo "$1" | tr '[:lower:]' '[:upper:]')_PORT"
    if [[ -z "${!var:-}" ]]; then
        echo -e "${RED}no port for node '$1': add a ports.services.$1 lane and $var to confit.toml${NC}" >&2
        return 1
    fi
    echo "${!var}"
}

# Data dir for a node. The nick == the section name == the directory.
get_data_dir() {
    echo "$DATA_DIR/$1"
}
