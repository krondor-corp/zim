#!/bin/bash
# TOML config parsing for the dev environment.

# Read a value from nodes.toml.
# Usage: toml_get <section> <key>
toml_get() {
    local section="$1"
    local key="$2"
    local in_section=false

    while IFS= read -r line; do
        # Skip comments.
        [[ "$line" =~ ^[[:space:]]*# ]] && continue

        # Section header.
        if [[ "$line" =~ ^\[([a-zA-Z0-9_]+)\]$ ]]; then
            if [[ "${BASH_REMATCH[1]}" == "$section" ]]; then
                in_section=true
            else
                in_section=false
            fi
            continue
        fi

        # key = value in current section.
        if $in_section && [[ "$line" =~ ^${key}[[:space:]]*=[[:space:]]*(.+)$ ]]; then
            local value="${BASH_REMATCH[1]}"
            # Strip inline comments.
            value="${value%%#*}"
            # Trim trailing whitespace.
            value="${value%"${value##*[![:space:]]}"}"
            # Strip surrounding quotes.
            value="${value%\"}"
            value="${value#\"}"
            echo "$value"
            return 0
        fi
    done < "$CONFIG_FILE"

    return 1
}

# Every section name in nodes.toml is a peer nick. Sorted + dedup'd.
get_node_names() {
    grep -E '^\[[a-zA-Z0-9_]+\]$' "$CONFIG_FILE" | tr -d '[]' | sort -u
}

# Convenience accessors.
get_api_port() { toml_get "$1" api_port; }

# Data dir for a node. The nick == the section name == the directory.
get_data_dir() {
    echo "$DATA_DIR/$1"
}
