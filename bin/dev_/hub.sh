#!/bin/bash
# Optional zim-hub node for the dev environment.
#
# Boots `zim-hub` in a tmux window alongside the daemons, with dev
# defaults. When `confit` is installed it resolves *real* Google OAuth
# credentials (like `bin/hub`) so the web view is usable; otherwise it
# falls back to dummy creds (web login disabled, daemon<->hub sync
# still works — that path is pure iroh and never touches OAuth).
#
# `hub enroll` stands in for `zim login`: it writes the user + daemon
# device rows directly into the hub DB (via the `zim-hub-devseed` bin),
# so the dev daemons are enrolled without the device-code browser
# dance. The web user is seeded to $ZIM_DEV_SEED_EMAIL (default
# al@krondor.org). You still mint the browser web-key manually in the
# onboarding UI.
#
# Needs docker (minio, via bin/minio) for the S3 blob store.
#
#   ./bin/dev hub up       Start minio + the hub (tmux window 'hub')
#   ./bin/dev hub enroll   Seed the hub user + enroll all dev daemons
#   ./bin/dev hub did      Print the hub's did:web (for `peers add`)
#   ./bin/dev hub down     Kill the hub window

HUB_PORT="${HUB_PORT:-8080}"
HUB_HOME="$DATA_DIR/zim-hub"
HUB_BIN="$PROJECT_ROOT/target/debug/zim-hub"
# The hub runs under `cargo watch` too, watching the shared sync crates — so an
# edit to e.g. `zim-peer` (the wire protocol) rebuilds the hub *and* the
# daemons together, never leaving them on skewed protocol versions. (The SPA
# bundle is separate: `bin/build-web` / `trunk watch`.)
HUB_WATCH_DIRS="-w crates/zim-hub/src -w crates/zim/src -w crates/zim-peer/src -w crates/zim-core/src -w crates/zim-crypto/src -w crates/zim-did/src -w crates/zim-runtime/src"
HUB_WINDOW="hub"
# Web user the hub seeds + enrolls daemons under. Override with the env var.
SEED_EMAIL="${ZIM_DEV_SEED_EMAIL:-al@krondor.org}"

hub_url() { echo "http://127.0.0.1:$HUB_PORT"; }

cmd_hub() {
    case "${1:-up}" in
        up)     hub_up ;;
        enroll) hub_enroll ;;
        did)    hub_did ;;
        down)   hub_down ;;
        *)      echo -e "${RED}usage: ./bin/dev hub [up|enroll|did|down]${NC}"; exit 1 ;;
    esac
}

hub_up() {
    command -v tmux >/dev/null 2>&1 || { echo -e "${RED}tmux not installed${NC}"; exit 1; }

    echo -e "${BLUE}Starting minio (blob store)…${NC}"
    "$PROJECT_ROOT/bin/minio" up >/dev/null || {
        echo -e "${RED}minio failed to start — is docker running?${NC}"
        exit 1
    }

    echo -e "${BLUE}Building web SPA…${NC}"
    "$PROJECT_ROOT/bin/build-web" || {
        echo -e "${RED}web SPA build failed${NC}"
        exit 1
    }

    echo -e "${BLUE}Building zim-hub…${NC}"
    (cd "$PROJECT_ROOT" && cargo build --bin zim-hub --quiet) || {
        echo -e "${RED}cargo build failed${NC}"
        exit 1
    }
    mkdir -p "$HUB_HOME"

    if ! tmux has-session -t "$TMUX_SESSION" 2>/dev/null; then
        tmux new-session -d -s "$TMUX_SESSION" -n nodes
    fi
    if tmux list-windows -t "$TMUX_SESSION" -F '#W' 2>/dev/null | grep -qx "$HUB_WINDOW"; then
        echo -e "${YELLOW}hub window already running${NC}"
        return 0
    fi

    # Env the hub reads. `ZIM_HUB_HOST` is percent-encoded so did:web
    # resolves to the URL. Admin email defaults to the seed user so a
    # Google login as that account is auto-promoted to admin.
    local env_prefix="\
ZIM_HUB_LISTEN='127.0.0.1:$HUB_PORT' \
ZIM_HUB_HOME='$HUB_HOME' \
ZIM_HUB_HOST='127.0.0.1%3A$HUB_PORT' \
ZIM_HUB_ADMIN_EMAILS='${ZIM_HUB_ADMIN_EMAILS:-$SEED_EMAIL}' \
ZIM_HUB_S3_ENDPOINT='http://localhost:9000' \
ZIM_HUB_S3_ACCESS_KEY='minioadmin' \
ZIM_HUB_S3_SECRET_KEY='minioadmin' \
ZIM_HUB_S3_BUCKET='zim-blobs'"

    # Real Google OAuth via confit when available (web view usable),
    # else dummy creds (boots, but web login is disabled — sync only).
    # Run under `cargo watch` (not the prebuilt binary) so server/sync edits
    # rebuild + restart the hub live, in lockstep with the daemons.
    #
    # No `eval`: the command is parsed once by the tmux pane shell, which
    # interprets both the `$env_prefix` value quotes *and* the `-x 'run …'`
    # quoting. An `eval` here would double-parse and strip the inner quotes,
    # leaving cargo-watch to fall back to a bare `cargo run`.
    # `--bin zim-hub`: the package also ships `zim-hub-devseed`, so the bin
    # must be named explicitly.
    local run_hub="cargo watch $HUB_WATCH_DIRS -x 'run --bin zim-hub'"
    local launch
    if command -v confit >/dev/null 2>&1; then
        echo -e "${BLUE}Using real Google OAuth via confit (web view enabled).${NC}"
        launch="$env_prefix confit run credentials.app --upper -- $run_hub"
    else
        echo -e "${YELLOW}confit not found — dummy OAuth (web login disabled; sync only).${NC}"
        launch="$env_prefix GOOGLE_O_AUTH_CLIENT_ID='dev' GOOGLE_O_AUTH_CLIENT_SECRET='dev' $run_hub"
    fi

    tmux new-window -t "$TMUX_SESSION" -n "$HUB_WINDOW" \
        "cd $PROJECT_ROOT && printf '%b\n' '${GREEN}=== zim-hub on :$HUB_PORT ===${NC}' && $launch"

    echo -e "${GREEN}hub starting on $(hub_url)${NC} (tmux window '$HUB_WINDOW')"
    echo "  enroll: ./bin/dev hub enroll"
    echo "  did:    ./bin/dev hub did"
    echo "  logs:   tail -f $HUB_HOME/hub.log   (clean; pane has cargo-watch noise)"
}

# Stand in for `zim login` across all dev daemons: write the hub-side
# user + daemon device rows directly (via the zim-hub-devseed bin), so
# the daemons are enrolled without the device-code browser approval.
# Needs the daemons up — we read each node's pubkey off `zim id` (which
# hits the daemon API). The hub itself need not be running (the seeder
# opens the hub DB directly; WAL makes that safe alongside a live hub).
hub_enroll() {
    require_daemons_up

    echo -e "${BLUE}Building zim-hub-devseed…${NC}"
    (cd "$PROJECT_ROOT" && cargo build --bin zim-hub-devseed --quiet) || {
        echo -e "${RED}cargo build failed${NC}"; exit 1
    }
    local devseed="$PROJECT_ROOT/target/debug/zim-hub-devseed"

    # Collect LABEL=PUBKEY for every dev node.
    local args=()
    for n in $(get_node_names); do
        local pk
        pk="$(node_id "$n")"
        if [[ -z "$pk" ]]; then
            echo -e "${YELLOW}  skipping $n — could not read its pubkey${NC}"
            continue
        fi
        args+=("$n=$pk")
    done
    if [[ ${#args[@]} -eq 0 ]]; then
        echo -e "${RED}no node pubkeys — start daemons first: ./bin/dev run -b${NC}"
        exit 1
    fi

    echo -e "${BLUE}Enrolling ${#args[@]} daemon(s) into the hub as ${SEED_EMAIL}…${NC}"
    ZIM_HUB_HOME="$HUB_HOME" ZIM_DEV_SEED_EMAIL="$SEED_EMAIL" "$devseed" "${args[@]}" || exit 1

    # Simulate the daemon side of `zim hub login` (which we skipped): drop a
    # hub-session.json into each node's home so it knows which hub it's paired
    # with, then `hub peers sync` so each node's address book picks up the
    # whole account roster off the hub — the same path a real login would use.
    local now
    now="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    for n in $(get_node_names); do
        local pk home
        pk="$(node_id "$n")" || continue
        [[ -z "$pk" ]] && continue
        home="$(get_data_dir "$n")"
        printf '{\n  "hub_url": "%s",\n  "enrolled_pubkey": "%s",\n  "enrolled_at": "%s"\n}\n' \
            "$(hub_url)" "$pk" "$now" > "$home/hub-session.json"
        seed_cli "$n" hub peers sync >/dev/null 2>&1 \
            && echo -e "  ${GREEN}$n${NC} synced peer book from hub roster" \
            || echo -e "  ${YELLOW}$n${NC} hub peers sync failed (is the hub up?)"
    done

    echo -e "${GREEN}Enrolled.${NC} Next: sign in at $(hub_url) as ${SEED_EMAIL}, mint your web key."
}

# The hub's did:web, read off its own /.well-known/did.json. Pair with
# `./bin/dev cli alice peers add hub \"\$(./bin/dev hub did)\"`.
hub_did() {
    local doc
    doc=$(curl -sf "$(hub_url)/.well-known/did.json" 2>/dev/null) || {
        echo -e "${RED}hub not reachable at $(hub_url) — ./bin/dev hub up${NC}"
        exit 1
    }
    echo "$doc" | sed -E 's/.*"id"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/'
}

hub_down() {
    if tmux kill-window -t "$TMUX_SESSION:$HUB_WINDOW" 2>/dev/null; then
        echo -e "${GREEN}hub window killed${NC}"
    else
        echo -e "${YELLOW}no hub window running${NC}"
    fi
}
