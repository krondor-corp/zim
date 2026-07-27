#!/bin/bash
# Fixtures — declarative initial data for the dev environment.
#
# Reads `fixtures.toml` and drives every operation through the real
# `zim` CLI against the harness daemons (same path a user takes), so
# applying fixtures doubles as an end-to-end exercise of the stack.
# FUSE fixtures auto-skip when FUSE isn't available (see fuse-check).

FIXTURES_FILE="$DEV_DIR/fixtures.toml"

# --- fixture plumbing -------------------------------------------------------

# Run zim against a node by nick.
fix_cli() {
    local nick="$1"
    shift
    ZIM_HOME="$(get_data_dir "$nick")" "$ZIM_BIN" "$@"
}

fix_default_node() {
    get_node_names | head -1
}

# Resolve a mount_point: absolute stays as-is, relative lands in data/.
fix_mount_point() {
    local mp="${1/#\~/$HOME}"
    [[ "$mp" == /* ]] && echo "$mp" || echo "$DATA_DIR/$mp"
}

# --- TOML parsing -----------------------------------------------------------

# Emits one line per [[fixture]]:
#   "type|vault|name|path|content|source|node|peer|from|to|mount_point"
# Multiline ("""…""") content is emitted with \n escapes (unpacked by
# consumers via `echo -e` / `printf '%b'`).
parse_fixtures() {
    local in_fixture=false in_multiline=false
    local type="" vault="" name="" path="" content="" source="" node="" peer="" from="" to="" mount_point=""
    local multiline_content=""

    # Always returns 0 — a bare nonzero here would trip bin/dev's
    # `set -e` on the first [[fixture]] header (nothing to emit yet).
    flush() {
        if $in_fixture && [[ -n "$type" ]]; then
            echo "$type|$vault|$name|$path|$content|$source|$node|$peer|$from|$to|$mount_point"
        fi
        return 0
    }

    while IFS= read -r line; do
        if [[ "$line" =~ ^\[\[fixture\]\]$ ]]; then
            flush
            in_fixture=true in_multiline=false
            type="" vault="" name="" path="" content="" source="" node="" peer="" from="" to="" mount_point=""
            multiline_content=""
            continue
        fi
        $in_fixture || continue

        if $in_multiline; then
            if [[ "$line" =~ ^\"\"\"$ ]]; then
                content="$multiline_content"
                in_multiline=false
            else
                if [[ -n "$multiline_content" ]]; then
                    multiline_content="$multiline_content\\n$line"
                else
                    multiline_content="$line"
                fi
            fi
            continue
        fi

        if [[ "$line" =~ ^([a-z_]+)[[:space:]]*=[[:space:]]*(.+)$ ]]; then
            local key="${BASH_REMATCH[1]}" value="${BASH_REMATCH[2]}"
            # Multiline opener; keep any text on the opener line
            # (`content = """# Heading`).
            if [[ "$value" =~ ^\"\"\"(.*)$ ]]; then
                in_multiline=true
                multiline_content="${BASH_REMATCH[1]}"
                continue
            fi
            value="${value%\"}"
            value="${value#\"}"
            case "$key" in
                type)        type="$value" ;;
                vault)       vault="$value" ;;
                name)        name="$value" ;;
                path)        path="$value" ;;
                content)     content="$value" ;;
                source)      source="$value" ;;
                node)        node="$value" ;;
                peer)        peer="$value" ;;
                from)        from="$value" ;;
                to)          to="$value" ;;
                mount_point) mount_point="$value" ;;
            esac
        fi
    done < "$FIXTURES_FILE"
    flush
}

# --- vault fixtures (through the CLI) ---------------------------------------

fixture_vault() { # name node
    echo -e "${BLUE}vault: create $1 (on $2)${NC}"
    if fix_cli "$2" vault create "$1" >/dev/null 2>&1; then
        echo -e "  ${GREEN}created${NC}"
    elif fix_cli "$2" vault list 2>/dev/null | grep -qw "$1"; then
        echo -e "  ${GREEN}exists${NC}"
    else
        echo -e "  ${RED}FAILED${NC}"
        return 1
    fi
}

fixture_file() { # vault path content source node
    local vault="$1" path="$2" content="$3" source="$4" node="$5"
    echo -e "${BLUE}file: $vault:$path (on $node)${NC}"
    if [[ -n "$source" ]]; then
        if [[ ! -f "$PROJECT_ROOT/$source" ]]; then
            echo -e "  ${RED}FAILED: source not found: $source${NC}"
            return 1
        fi
        fix_cli "$node" vault add "$vault" "$path" < "$PROJECT_ROOT/$source" >/dev/null 2>&1
    else
        printf '%b' "$content" | fix_cli "$node" vault add "$vault" "$path" >/dev/null 2>&1
    fi
    if [[ $? -eq 0 ]]; then
        echo -e "  ${GREEN}written${NC}"
    else
        echo -e "  ${RED}FAILED${NC}"
        return 1
    fi
}

fixture_dir() { # vault path node
    echo -e "${BLUE}dir: $1:$2 (on $3)${NC}"
    if fix_cli "$3" vault mkdir "$1" "$2" >/dev/null 2>&1; then
        echo -e "  ${GREEN}created${NC}"
    else
        echo -e "  ${RED}FAILED${NC}"
        return 1
    fi
}

fixture_share() { # vault peer node
    local vault="$1" peer="$2" node="$3"
    local peer_key
    peer_key="$(node_id "$peer")"
    if [[ -z "$peer_key" ]]; then
        echo -e "${RED}share: could not resolve pubkey for '$peer'${NC}"
        return 1
    fi
    echo -e "${BLUE}share: $vault → $peer (on $node)${NC}"
    # Give discovery a moment so the fire-and-forget OfferShare lands;
    # the share is recorded (and re-announced by the reconcile sweep)
    # either way.
    if wait_dialable "$node" "$peer"; then
        fix_cli "$node" vault shares add "$vault" "$peer_key" >/dev/null 2>&1 \
            && echo -e "  ${GREEN}shared (reachable)${NC}" && return 0
    else
        fix_cli "$node" vault shares add "$vault" "$peer_key" >/dev/null 2>&1 \
            && echo -e "  ${YELLOW}shared (unreachable — syncs when discovery converges)${NC}" && return 0
    fi
    echo -e "  ${RED}FAILED${NC}"
    return 1
}

fixture_mv() { # vault from to node
    echo -e "${BLUE}mv: $1:$2 → $3 (on $4)${NC}"
    if fix_cli "$4" vault mv "$1" "$2" "$3" >/dev/null 2>&1; then
        echo -e "  ${GREEN}moved${NC}"
    else
        echo -e "  ${RED}FAILED${NC}"
        return 1
    fi
}

fixture_vault_read() { # vault path content node
    local vault="$1" path="$2" expected="$3" node="$4"
    echo -e "${BLUE}vault_read: $vault:$path (on $node)${NC}"
    local actual
    if ! actual="$(fix_cli "$node" vault cat "$vault" "$path" 2>/dev/null)"; then
        echo -e "  ${RED}FAILED: could not read${NC}"
        return 1
    fi
    if [[ -n "$expected" ]]; then
        if [[ "$actual" == "$(printf '%b' "$expected")" ]]; then
            echo -e "  ${GREEN}OK (content verified)${NC}"
        else
            echo -e "  ${RED}FAILED: content mismatch${NC}"
            echo -e "  ${RED}  expected: $(printf '%b' "$expected" | head -1)${NC}"
            echo -e "  ${RED}  actual:   $(head -1 <<<"$actual")${NC}"
            return 1
        fi
    else
        echo -e "  ${GREEN}OK${NC}"
    fi
}

# --- FUSE fixtures ----------------------------------------------------------

fixture_mount() { # vault mount_point node
    local vault="$1" mp node="$3"
    mp="$(fix_mount_point "$2")"
    echo -e "${BLUE}mount: $vault at $mp (on $node)${NC}"
    mkdir -p "$mp" 2>/dev/null || true
    if fix_cli "$node" mount add "$vault" "$mp" >/dev/null 2>&1; then
        sleep 1
        echo -e "  ${GREEN}mounted${NC}"
    else
        echo -e "  ${RED}FAILED${NC}"
        return 1
    fi
}

fixture_mount_verify() { # mount_point
    local mp
    mp="$(fix_mount_point "$1")"
    echo -e "${BLUE}mount_verify: $mp${NC}"
    sleep 1
    if mount | grep -q " $mp " && ls "$mp" >/dev/null 2>&1; then
        echo -e "  ${GREEN}accessible${NC}"
    else
        echo -e "  ${RED}FAILED: not mounted or unreadable${NC}"
        return 1
    fi
}

fixture_unmount() { # vault node
    echo -e "${BLUE}unmount: $1 (on $2)${NC}"
    if fix_cli "$2" mount stop "$1" >/dev/null 2>&1; then
        echo -e "  ${GREEN}unmounted${NC}"
    else
        echo -e "  ${YELLOW}unmount reported an error (may not have been mounted)${NC}"
    fi
}

fixture_fuse_ls() { # mount_point path
    local mp
    mp="$(fix_mount_point "$1")"
    echo -e "${BLUE}fuse_ls: $2${NC}"
    if ls "$mp/$2" >/dev/null 2>&1; then
        echo -e "  ${GREEN}OK${NC}"
    else
        echo -e "  ${RED}FAILED: could not list $mp/$2${NC}"
        return 1
    fi
}

fixture_fuse_read() { # mount_point path content
    local mp full actual
    mp="$(fix_mount_point "$1")"
    full="$mp/$2"
    echo -e "${BLUE}fuse_read: $2${NC}"
    if [[ ! -f "$full" ]]; then
        echo -e "  ${RED}FAILED: file not found $full${NC}"
        return 1
    fi
    if ! actual="$(cat "$full" 2>/dev/null)"; then
        echo -e "  ${RED}FAILED: could not read $full${NC}"
        return 1
    fi
    if [[ -n "$3" ]]; then
        if [[ "$actual" == "$(printf '%b' "$3")" ]]; then
            echo -e "  ${GREEN}OK (content verified)${NC}"
        else
            echo -e "  ${RED}FAILED: content mismatch${NC}"
            echo -e "  ${RED}  expected: $(printf '%b' "$3" | head -1)${NC}"
            echo -e "  ${RED}  actual:   $(head -1 <<<"$actual")${NC}"
            return 1
        fi
    else
        echo -e "  ${GREEN}OK${NC}"
    fi
}

fixture_fuse_write() { # mount_point path content
    local mp
    mp="$(fix_mount_point "$1")"
    echo -e "${BLUE}fuse_write: $2${NC}"
    if printf '%b' "$3" > "$mp/$2" 2>/dev/null; then
        sync
        echo -e "  ${GREEN}OK${NC}"
    else
        echo -e "  ${RED}FAILED: could not write $mp/$2${NC}"
        return 1
    fi
}

fixture_fuse_mv() { # mount_point from to
    local mp
    mp="$(fix_mount_point "$1")"
    echo -e "${BLUE}fuse_mv: $2 → $3${NC}"
    if mv "$mp/$2" "$mp/$3" 2>/dev/null; then
        echo -e "  ${GREEN}OK${NC}"
    else
        echo -e "  ${RED}FAILED: could not move $mp/$2 → $mp/$3${NC}"
        return 1
    fi
}

fixture_fuse_mv_in() { # mount_point path content
    local mp tmp
    mp="$(fix_mount_point "$1")"
    echo -e "${BLUE}fuse_mv_in: (tmpfile) → $2${NC}"
    tmp="$(mktemp)"
    printf '%b' "$3" > "$tmp"
    if mv "$tmp" "$mp/$2" 2>/dev/null; then
        echo -e "  ${GREEN}OK${NC}"
    else
        echo -e "  ${RED}FAILED: could not move into mount${NC}"
        rm -f "$tmp"
        return 1
    fi
}

fixture_fuse_mv_out() { # mount_point path
    local mp tmp
    mp="$(fix_mount_point "$1")"
    echo -e "${BLUE}fuse_mv_out: $2 → (tmpfile)${NC}"
    tmp="$(mktemp)"
    rm -f "$tmp" # free the name for mv
    if mv "$mp/$2" "$tmp" 2>/dev/null && [[ -f "$tmp" && ! -e "$mp/$2" ]]; then
        echo -e "  ${GREEN}OK${NC}"
        rm -f "$tmp"
    else
        echo -e "  ${RED}FAILED: could not move out of mount${NC}"
        rm -f "$tmp"
        return 1
    fi
}

fixture_fuse_rm() { # mount_point path
    local mp full
    mp="$(fix_mount_point "$1")"
    full="$mp/$2"
    echo -e "${BLUE}fuse_rm: $2${NC}"
    if rm -f "$full" 2>/dev/null && [[ ! -e "$full" ]]; then
        echo -e "  ${GREEN}OK${NC}"
    else
        echo -e "  ${RED}FAILED: could not delete $full${NC}"
        return 1
    fi
}

# --- FUSE availability ------------------------------------------------------

# FUSE needs (a) the platform library and (b) a daemon built with the
# `fuse` feature (checked via `_status/version` build_features).
check_fuse_available() {
    local node="${1:-$(fix_default_node)}"
    local fuse_device=false fuse_feature=false

    case "$(uname -s)" in
        Darwin) [[ -d /Library/Filesystems/macfuse.fs ]] && fuse_device=true ;;
        Linux)  [[ -e /dev/fuse ]] && fuse_device=true ;;
    esac

    local port version_info
    port="$(get_api_port "$node" 2>/dev/null)"
    if [[ -n "$port" ]]; then
        version_info="$(curl -sf "http://127.0.0.1:$port/_status/version" 2>/dev/null)"
        grep -q '"fuse"' <<<"$version_info" && fuse_feature=true
    fi

    if $fuse_device && $fuse_feature; then
        echo -e "${GREEN}FUSE available — FUSE fixtures will run${NC}"
        return 0
    fi
    if ! $fuse_device; then
        echo -e "${YELLOW}FUSE not available — no FUSE library/device on this platform${NC}"
    else
        echo -e "${YELLOW}FUSE not available — daemon '$node' not built with the fuse feature${NC}"
    fi
    echo -e "${YELLOW}FUSE fixtures will be skipped (not a failure)${NC}"
    return 1
}

cmd_fuse_check() {
    check_fuse_available "$@"
}

# --- commands ---------------------------------------------------------------

fixtures_list() {
    echo -e "${GREEN}Fixtures to apply:${NC} ($FIXTURES_FILE)"
    echo ""
    parse_fixtures | while IFS='|' read -r type vault name path content source node peer from to mount_point; do
        case "$type" in
            vault)        echo "  [vault]        name=$name node=$node" ;;
            file)         echo "  [file]         vault=$vault path=$path node=$node" ;;
            dir)          echo "  [dir]          vault=$vault path=$path node=$node" ;;
            share)        echo "  [share]        vault=$vault peer=$peer node=$node" ;;
            mv)           echo "  [mv]           vault=$vault from=$from to=$to node=$node" ;;
            vault_read)   echo "  [vault_read]   vault=$vault path=$path node=$node" ;;
            mount)        echo "  [mount]        vault=$vault mount_point=$mount_point node=$node" ;;
            mount_verify) echo "  [mount_verify] mount_point=$mount_point" ;;
            unmount)      echo "  [unmount]      vault=$vault node=$node" ;;
            fuse_*)       echo "  [$type]  mount_point=$mount_point path=$path$([[ -n $from ]] && echo " from=$from to=$to")" ;;
            *)            echo "  [?] unknown type: $type" ;;
        esac
    done
}

# Is this fixture type part of the FUSE block?
is_fuse_fixture() {
    case "$1" in
        mount|mount_verify|unmount|fuse_*) return 0 ;;
        *) return 1 ;;
    esac
}

fixtures_apply() {
    if [[ ! -f "$FIXTURES_FILE" ]]; then
        echo -e "${YELLOW}No fixtures file: $FIXTURES_FILE${NC}"
        return 0
    fi

    local fuse_ok=true
    check_fuse_available >/dev/null 2>&1 || fuse_ok=false

    echo -e "${BLUE}Applying fixtures…${NC} (FUSE: $($fuse_ok && echo enabled || echo skipped))"
    echo ""

    local errors=0 skipped=0
    while IFS='|' read -r type vault name path content source node peer from to mount_point; do
        node="${node:-$(fix_default_node)}"

        if is_fuse_fixture "$type" && ! $fuse_ok; then
            skipped=$((skipped + 1))
            continue
        fi

        case "$type" in
            vault)        fixture_vault "$name" "$node" || errors=$((errors + 1)) ;;
            file)         fixture_file "$vault" "$path" "$content" "$source" "$node" || errors=$((errors + 1)) ;;
            dir)          fixture_dir "$vault" "$path" "$node" || errors=$((errors + 1)) ;;
            share)        fixture_share "$vault" "$peer" "$node" || errors=$((errors + 1)) ;;
            mv)           fixture_mv "$vault" "$from" "$to" "$node" || errors=$((errors + 1)) ;;
            vault_read)   fixture_vault_read "$vault" "$path" "$content" "$node" || errors=$((errors + 1)) ;;
            mount)        fixture_mount "$vault" "$mount_point" "$node" || errors=$((errors + 1)) ;;
            mount_verify) fixture_mount_verify "$mount_point" || errors=$((errors + 1)) ;;
            unmount)      fixture_unmount "$vault" "$node" || errors=$((errors + 1)) ;;
            fuse_ls)      fixture_fuse_ls "$mount_point" "$path" || errors=$((errors + 1)) ;;
            fuse_read)    fixture_fuse_read "$mount_point" "$path" "$content" || errors=$((errors + 1)) ;;
            fuse_write)   fixture_fuse_write "$mount_point" "$path" "$content" || errors=$((errors + 1)) ;;
            fuse_mv)      fixture_fuse_mv "$mount_point" "$from" "$to" || errors=$((errors + 1)) ;;
            fuse_mv_in)   fixture_fuse_mv_in "$mount_point" "$path" "$content" || errors=$((errors + 1)) ;;
            fuse_mv_out)  fixture_fuse_mv_out "$mount_point" "$path" || errors=$((errors + 1)) ;;
            fuse_rm)      fixture_fuse_rm "$mount_point" "$path" || errors=$((errors + 1)) ;;
            *)            echo -e "${RED}unknown fixture type: $type${NC}"; errors=$((errors + 1)) ;;
        esac
    done < <(parse_fixtures)

    echo ""
    [[ $skipped -gt 0 ]] && echo -e "${YELLOW}$skipped FUSE fixture(s) skipped (FUSE unavailable)${NC}"
    if [[ $errors -eq 0 ]]; then
        echo -e "${GREEN}Fixtures applied successfully${NC}"
        return 0
    fi
    echo -e "${RED}$errors fixture(s) failed${NC}"
    return 1
}

fixtures_help() {
    cat <<EOF
Fixtures — declarative initial data for the dev environment

Applied by \`./bin/dev seed\` (after peer plumbing), or directly:

Usage: ./bin/dev fixtures [command]

Commands:
  apply    Apply all fixtures from fixtures.toml (default)
  list     List fixtures without applying
  help     Show this help

Config: $FIXTURES_FILE — see its header for the fixture-type reference
and the EXPECTED END STATE comment at the bottom for what e2e verifies.
EOF
}

cmd_fixtures() {
    case "${1:-apply}" in
        apply)          fixtures_apply ;;
        list)           fixtures_list ;;
        help|-h|--help) fixtures_help ;;
        *) echo -e "${RED}Unknown fixtures command: $1${NC}"; fixtures_help; return 1 ;;
    esac
}
