#!/bin/bash
# Seed the dev environment with vaults + shares so syncing is testable
# without a browser. Daemon-only — needs no docker/minio/hub.
#
# Cross-adds every peer to every other's address book (so `OfferShare`
# passes the spam gate and dialing resolves), then the first node
# ("alice") creates two vaults, drops some content, and shares `demo`
# with every other peer. Idempotent — safe to re-run.

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

# Wait until `from` can actually dial `to` (iroh discovery converged).
# Fresh daemons take a few seconds to publish/resolve; sharing before
# then means the fire-and-forget `OfferShare` is dropped. Returns
# non-zero (with a warning) if it never becomes reachable.
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
    local owner="${nodes[0]}"

    echo -e "${BLUE}Seeding dev vaults (owner: $owner)…${NC}"

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

    # Owner creates a couple of vaults with content.
    seed_cli "$owner" vaults create demo >/dev/null 2>&1 || true
    printf 'hello from %s\n' "$owner" \
        | seed_cli "$owner" vault demo add /readme.md >/dev/null 2>&1 || true
    seed_cli "$owner" vaults create notes >/dev/null 2>&1 || true
    printf '# notes\n\nseeded by ./bin/dev seed\n' \
        | seed_cli "$owner" vault notes add /index.md >/dev/null 2>&1 || true

    # Share `demo` with every other peer (direct `did:key`). The share
    # is recorded on the owner regardless; we give discovery a short
    # window to converge so the fire-and-forget OfferShare actually
    # lands. If the peer's still unreachable, the share is set and will
    # sync once discovery catches up (or on a re-run).
    for b in "${nodes[@]:1}"; do
        if wait_dialable "$owner" "$b"; then
            seed_cli "$owner" vault demo shares add "$(node_id "$b")" >/dev/null 2>&1 \
                && echo -e "  shared ${GREEN}demo${NC} → $b ${GREEN}(reachable)${NC}"
        else
            seed_cli "$owner" vault demo shares add "$(node_id "$b")" >/dev/null 2>&1 \
                && echo -e "  shared ${GREEN}demo${NC} → $b ${YELLOW}(unreachable — syncs once discovery converges)${NC}"
        fi
    done

    echo -e "${GREEN}Seeded.${NC} Verify sync propagated:"
    local peer="${nodes[1]:-$owner}"
    echo "  ./bin/dev cli $peer vaults list"
    echo "  ./bin/dev cli $peer vault demo cat /readme.md"
}
