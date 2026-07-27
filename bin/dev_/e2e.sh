#!/bin/bash
# One-shot end-to-end test run over the real dev harness.
#
# `./bin/dev e2e` (or `make e2e`): clean start → daemons → fixtures →
# cross-node sync verification → PASS/FAIL + exit code. Deterministic
# in outcome: peers are cross-introduced directly (no DHT in the local
# dial path) and every sync assertion polls until converged or a
# deadline — no bare sleeps. FUSE fixtures run when available, else
# skip (not a failure). The seeded environment is left running.

# Poll `cmd...` until it succeeds or $E2E_DEADLINE seconds pass.
E2E_DEADLINE="${E2E_DEADLINE:-60}"
e2e_until() {
    local label="$1"
    shift
    local start elapsed
    start=$(date +%s)
    while true; do
        if "$@" >/dev/null 2>&1; then
            elapsed=$(( $(date +%s) - start ))
            echo -e "  ${GREEN}✓${NC} $label (${elapsed}s)"
            return 0
        fi
        elapsed=$(( $(date +%s) - start ))
        if (( elapsed >= E2E_DEADLINE )); then
            echo -e "  ${RED}✗${NC} $label (deadline ${E2E_DEADLINE}s)"
            return 1
        fi
        sleep 1
    done
}

# Helper predicates for e2e_until (must be argv-invocable).
e2e_bob_sees_demo() { cmd_cli "$1" vault list 2>/dev/null | grep -qw demo; }
e2e_cat_matches() { # nick vault path expected
    [[ "$(cmd_cli "$1" vault cat "$2" "$3" 2>/dev/null)" == "$4" ]]
}

cmd_e2e() {
    local errors=0

    echo -e "${BLUE}=== zim e2e ===${NC}"

    # FUSE on when the platform lib is present (daemon feature + lib
    # are both required at fixture time; fuse-check reports the truth).
    if fuse_lib_present; then
        export ZIM_DEV_FUSE=1
    fi

    echo -e "${BLUE}[1/4] clean start${NC}"
    cmd_kill --force >/dev/null 2>&1 || true
    cmd_clean >/dev/null 2>&1 || true

    echo -e "${BLUE}[2/4] daemons up${NC}"
    cmd_run -b >/dev/null
    local nodes=($(get_node_names))
    for n in "${nodes[@]}"; do
        e2e_until "daemon $n healthy" \
            curl -sf "http://127.0.0.1:$(get_api_port "$n")/_status/livez" \
            || { echo -e "${RED}e2e aborted: $n never came up${NC}"; return 1; }
    done

    echo -e "${BLUE}[3/4] fixtures${NC}"
    if ! cmd_seed; then
        echo -e "${RED}e2e FAILED: fixture apply failed${NC}"
        return 1
    fi

    echo -e "${BLUE}[4/4] cross-node sync${NC}"
    local a="${nodes[0]}" b="${nodes[1]:-${nodes[0]}}"
    e2e_until "$b sees demo" e2e_bob_sees_demo "$b" || errors=$((errors + 1))
    e2e_until "$b reads $a's /readme.md" \
        e2e_cat_matches "$b" demo /readme.md "hello from alice" || errors=$((errors + 1))
    e2e_until "$b reads moved /guide.md" \
        cmd_cli "$b" vault cat demo /guide.md || errors=$((errors + 1))
    # Round-trip the other way.
    echo "hi from $b" | cmd_cli "$b" vault add demo /b.md >/dev/null 2>&1
    e2e_until "$a reads $b's /b.md" \
        e2e_cat_matches "$a" demo /b.md "hi from $b" || errors=$((errors + 1))

    echo ""
    if [[ $errors -eq 0 ]]; then
        echo -e "${GREEN}e2e PASS${NC} — environment left running (./bin/dev status)"
        return 0
    fi
    echo -e "${RED}e2e FAIL — $errors check(s) failed${NC} (env left running for inspection)"
    return 1
}
