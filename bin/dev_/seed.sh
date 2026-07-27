#!/bin/bash
# Seed the dev environment: peer plumbing, then declarative fixtures.
#
# Cross-adds every peer to every other's address book (so `OfferShare`
# passes the spam gate and dialing resolves), then applies
# `fixtures.toml` — vaults, content, shares, FUSE checks — via
# fixtures.sh. Daemon-only — needs no docker/minio/hub. Idempotent.

# Run zim against a node by nick (internal sibling of `cmd_cli`).
seed_cli() {
    local nick="$1"
    shift
    ZIM_HOME="$(get_data_dir "$nick")" "$ZIM_BIN" "$@"
}

# A node's raw hex pubkey (== iroh NodeId == `did:key` body).
node_id() {
    seed_cli "$1" id 2>/dev/null | tail -1
}

# Cross-introduce every daemon's iroh NodeAddr to every other, so
# local dials go direct over loopback instead of waiting on pkarr/DHT
# discovery. This is what makes seeded e2e runs deterministic and
# hermetic — no public network in the local dial path.
introduce_all() {
    local nodes=("$@")
    for a in "${nodes[@]}"; do
        local a_port addr_json
        a_port=$(get_api_port "$a")
        addr_json=$(curl -sf -X POST "http://127.0.0.1:$a_port/api/v0/peers/addr" \
            -H 'Content-Type: application/json' -d '{}') || continue
        for b in "${nodes[@]}"; do
            [[ "$a" == "$b" ]] && continue
            local b_port
            b_port=$(get_api_port "$b")
            curl -sf -X POST "http://127.0.0.1:$b_port/api/v0/peers/introduce" \
                -H 'Content-Type: application/json' -d "$addr_json" >/dev/null 2>&1 || true
        done
    done
}

# Wait until `from` can actually dial `to` (iroh discovery converged).
# With `introduce_all` this converges near-instantly; the loop remains
# as a guard. Returns non-zero (with a warning) if never reachable.
wait_dialable() {
    local from="$1" to="$2" tries=0
    until seed_cli "$from" peers ping "$to" >/dev/null 2>&1; do
        tries=$((tries + 1))
        ((tries > 15)) && return 1
        sleep 1
    done
    return 0
}

require_daemons_up() {
    for n in $(get_node_names); do
        local port
        port=$(get_api_port "$n")
        if ! curl -sf "http://127.0.0.1:$port/_status/livez" >/dev/null 2>&1; then
            echo -e "${RED}daemon '$n' is down — start it first: ./bin/dev run -b${NC}"
            exit 1
        fi
    done
}

cmd_seed() {
    ensure_zim_built
    seed_node_configs
    require_daemons_up

    local nodes
    nodes=($(get_node_names))

    echo -e "${BLUE}Seeding: peer plumbing…${NC}"

    # Bootstrap each peer's local state (idempotent).
    for n in "${nodes[@]}"; do
        seed_cli "$n" init >/dev/null 2>&1 || true
    done

    # Cross-add address books so shares are accepted + peers dialable.
    for a in "${nodes[@]}"; do
        for b in "${nodes[@]}"; do
            [[ "$a" == "$b" ]] && continue
            seed_cli "$a" peers add "$b" "$(node_id "$b")" >/dev/null 2>&1 || true
        done
    done

    # Direct NodeAddr exchange: local dials skip DHT discovery entirely.
    introduce_all "${nodes[@]}"

    # Data + FUSE checks are declarative — see fixtures.toml.
    fixtures_apply || return 1

    echo -e "${GREEN}Seeded.${NC} Verify sync propagated:"
    local peer="${nodes[1]:-${nodes[0]}}"
    echo "  ./bin/dev cli $peer vault list"
    echo "  ./bin/dev cli $peer vault cat demo /readme.md"
}
