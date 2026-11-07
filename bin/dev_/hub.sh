#!/bin/bash
# Optional zim-hub node for the dev environment.
#
# Boots `zim-hub` in a tmux window alongside the daemons, with dev
# defaults. Login providers come from ZIM_HUB_AUTH (default
# `dev-trust`, built with `--features dev-login`): open the hub, type
# any email, you're an admin — no Google account or OAuth client
# needed. Add real Google OAuth alongside (or instead):
#
#   ZIM_HUB_AUTH=dev-trust,google GOOGLE_O_AUTH_CLIENT_ID=… GOOGLE_O_AUTH_CLIENT_SECRET=… ./bin/dev hub up
#
# `hub enroll` stands in for `zim login`: it writes the user + daemon
# device rows directly into the hub DB (via the `zim-hub-devseed` bin),
# so the dev daemons are enrolled without the device-code browser
# dance. The web user is seeded to $ZIM_DEV_SEED_EMAIL (default
# dev@example.com) — type that email at the dev login to land on the
# same account. You still mint the browser web-key manually in the
# onboarding UI.
#
# Needs a container runtime — docker or podman — for the S3 blob store
# (via bin/minio, which picks whichever is running).
#
#   ./bin/dev hub up       Start minio + the hub (tmux window 'hub')
#   ./bin/dev hub enroll   Seed the hub user + enroll all dev daemons
#   ./bin/dev hub did      Print the hub's did:web (for `peers add`)
#   ./bin/dev hub down     Kill the hub window

# HUB_PORT and HUB_BUCKET come from ports.sh (per worktree).
HUB_HOME="$DATA_DIR/zim-hub"
HUB_BIN="$PROJECT_ROOT/target/debug/zim-hub"
# The hub runs under `cargo watch` too, watching the shared sync crates — so an
# edit to e.g. `zim-peer` (the wire protocol) rebuilds the hub *and* the
# daemons together, never leaving them on skewed protocol versions. (The SPA
# bundle is separate: `make build-web` / `trunk watch`.)
HUB_WATCH_DIRS="-w crates/zim-hub/src -w crates/zim-peer/src -w crates/zim-core/src -w crates/zim-crypto/src -w crates/zim-did/src"
HUB_WINDOW="hub"
# Web user the hub seeds + enrolls daemons under. Override with the env var.
SEED_EMAIL="${ZIM_DEV_SEED_EMAIL:-dev@example.com}"

hub_url() { echo "http://127.0.0.1:$HUB_PORT"; }
# The same hub as other devices on the Wi-Fi see it (the phone, for
# `make ios-ipa-dev`): this Mac's Bonjour name. Only reachable when the
# hub was started with `hub up --lan`.
# Lowercased: browsers lowercase hosts, and the hub's did:web must match
# the DID the app derives from its address bar.
# Mark this dev env as LAN (data/.lan) and move the hub to LAN_HUB_PORT.
# Refused once the env already has a hub: its identity is 127.0.0.1.
enable_lan() {
    [[ -e "$LAN_MARKER" ]] && return 0
    if [[ -d "$HUB_HOME" ]]; then
        echo -e "${RED}this dev env already has a hub that is not on the LAN port (its did:web would change); run \`make dev-clean\` first${NC}"
        return 1
    fi
    mkdir -p "$DATA_DIR" && touch "$LAN_MARKER"
    HUB_PORT="$LAN_HUB_PORT"
}

lan_host() { echo "$(scutil --get LocalHostName | tr '[:upper:]' '[:lower:]').local"; }
lan_hub_url() { echo "http://$(lan_host):$HUB_PORT"; }

cmd_hub() {
    case "${1:-up}" in
        up)     shift; hub_up "$@" ;;
        enroll) hub_enroll ;;
        did)    hub_did ;;
        down)   hub_down ;;
        *)      echo -e "${RED}usage: ./bin/dev hub [up [--lan]|enroll|did|down]${NC}"; exit 1 ;;
    esac
}

hub_up() {
    # --lan: listen on every interface so a phone on the same Wi-Fi can
    # reach the hub, and answer as this Mac's Bonjour name — both the
    # public URL (page loads on other hosts redirect there,
    # http/canonical_host.rs) and the did:web (the app derives the hub's
    # DID from the address it loaded, so they must agree). Users, keys and
    # shares embed that DID, so a dev env is LAN or not for its lifetime
    # (see enable_lan): later `hub up`s stay LAN, and switching needs
    # `make dev-clean`. It serves on the fixed LAN_HUB_PORT, so one dev
    # IPA reaches whichever worktree is serving — one at a time. Anyone on
    # the network can reach the dev-trust login while it's on.
    case "${1:-}" in
        "") ;;
        --lan) enable_lan || exit 1 ;;
        *) echo -e "${RED}usage: ./bin/dev hub up [--lan]${NC}"; exit 1 ;;
    esac
    local listen_host="127.0.0.1" did_host="127.0.0.1" lan=""
    if [[ -e "$LAN_MARKER" ]]; then
        listen_host="0.0.0.0"; did_host="$(lan_host)"; lan=1
    fi
    command -v tmux >/dev/null 2>&1 || { echo -e "${RED}tmux not installed${NC}"; exit 1; }

    echo -e "${BLUE}Starting minio (blob store)…${NC}"
    "$PROJECT_ROOT/bin/minio" up >/dev/null || {
        echo -e "${RED}blob store failed to start — is docker or podman running? (./bin/minio status)${NC}"
        exit 1
    }
    # One shared container, one bucket per worktree: each hub has its own
    # blob index and may delete objects, so hubs must never share a bucket.
    "$PROJECT_ROOT/bin/minio" bucket "$HUB_BUCKET" >/dev/null || {
        echo -e "${RED}could not create bucket $HUB_BUCKET (./bin/minio bucket $HUB_BUCKET)${NC}"
        exit 1
    }

    echo -e "${BLUE}Building web SPA…${NC}"
    # `ARGS=` neutralizes make's command-line-variable propagation: a
    # caller's `make … ARGS=…` would otherwise reach the sub-make and be
    # handed to trunk.
    make -C "$PROJECT_ROOT" build-web ARGS= || {
        echo -e "${RED}web SPA build failed${NC}"
        exit 1
    }

    echo -e "${BLUE}Building zim-hub…${NC}"
    (cd "$PROJECT_ROOT" && cargo build --bin zim-hub --features dev-login --quiet) || {
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
    if [[ -n "$lan" ]] && lsof -ti tcp:"$HUB_PORT" -sTCP:LISTEN >/dev/null 2>&1; then
        echo -e "${RED}LAN port $HUB_PORT is taken — another worktree's LAN hub? Stop it there (./bin/dev hub down) first.${NC}"
        exit 1
    fi

    # Env the hub reads. `ZIM_HUB_HOST` is percent-encoded so did:web
    # resolves to the URL. The SPA timings are dev-fast (production: 30s
    # autosave, 3s follow) so an edit reaches another tab in ~2s; the hub
    # injects them into index.html. Override either by exporting it.
    local env_prefix="\
ZIM_HUB_LISTEN='$listen_host:$HUB_PORT' \
ZIM_HUB_AUTOSAVE_MS='${ZIM_HUB_AUTOSAVE_MS:-1500}' \
ZIM_HUB_FOLLOW_MS='${ZIM_HUB_FOLLOW_MS:-1000}' \
ZIM_HUB_HOME='$HUB_HOME' \
ZIM_HUB_HOST='$did_host%3A$HUB_PORT' \
ZIM_HUB_S3_ENDPOINT='http://localhost:$MINIO_API_PORT' \
ZIM_HUB_S3_ACCESS_KEY='minioadmin' \
ZIM_HUB_S3_SECRET_KEY='minioadmin' \
ZIM_HUB_S3_BUCKET='$HUB_BUCKET'"
    # Google sign-in settings, forwarded only when the caller set them:
    # the tmux pane gets the tmux server's env, not ours.
    if [[ -n "$lan" ]]; then
        env_prefix="$env_prefix HOST_NAME='$(lan_hub_url)'"
        echo -e "${YELLOW}LAN mode: hub reachable at $(lan_hub_url) from this network${NC}"
    fi
    local var
    for var in GOOGLE_O_AUTH_CLIENT_ID GOOGLE_O_AUTH_CLIENT_SECRET ZIM_HUB_ADMIN_EMAILS; do
        [[ -n "${!var:-}" ]] && env_prefix="$env_prefix $var='${!var}'"
    done

    # Dev-trust login by default; set ZIM_HUB_AUTH (e.g.
    # `dev-trust,google`) to change the provider list. Run under
    # `cargo watch` (not the prebuilt binary) so server/sync edits
    # rebuild + restart the hub live, in lockstep with the daemons.
    #
    # No `eval`: the command is parsed once by the tmux pane shell, which
    # interprets both the `$env_prefix` value quotes *and* the `-x 'run …'`
    # quoting. An `eval` here would double-parse and strip the inner quotes,
    # leaving cargo-watch to fall back to a bare `cargo run`.
    # `--bin zim-hub`: the package also ships `zim-hub-devseed`, so the bin
    # must be named explicitly.
    local run_hub="cargo watch $HUB_WATCH_DIRS -x 'run --bin zim-hub --features dev-login'"
    local auth="${ZIM_HUB_AUTH:-dev-trust}"
    echo -e "${BLUE}Login providers: $auth${NC}"
    local launch="$env_prefix ZIM_HUB_AUTH='$auth' $run_hub"

    tmux new-window -t "$TMUX_SESSION" -n "$HUB_WINDOW" \
        "cd $PROJECT_ROOT && printf '%b\n' '${GREEN}=== zim-hub on :$HUB_PORT ===${NC}' && $launch"

    echo -e "${GREEN}hub starting on $(hub_url)${NC} (tmux window '$HUB_WINDOW', bucket $HUB_BUCKET)"
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
    (cd "$PROJECT_ROOT" && cargo build --bin zim-hub-devseed --features dev-login --quiet) || {
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
        # The hub itself must be a contact — its pushes (browser-side
        # writes fanned out over iroh) are gated by ContactsAcceptPolicy,
        # and `hub peers sync` only adds roster *devices*. A real
        # `zim hub login` does this; the dev stand-in must too.
        seed_cli "$n" peers add hub "$(hub_did)" >/dev/null 2>&1 || true
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
    # `cargo watch` can leave the hub it spawned running after its window
    # dies, still holding the port; the next `hub up` would then talk to
    # the old hub (or fail to bind). Listener only, this worktree's port.
    local pid
    pid=$(lsof -ti tcp:"$HUB_PORT" -sTCP:LISTEN 2>/dev/null) || true
    if [[ -n "$pid" ]]; then
        kill $pid 2>/dev/null && echo -e "${GREEN}stopped hub process $pid on :$HUB_PORT${NC}"
    fi
}
