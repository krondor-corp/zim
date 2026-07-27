# Vault POSIX commands should mimic real POSIX UX

**Stage:** Planned
**Priority:** Low

## Objective

Make `zim vault <target> {ls,cat,rm,mv,mkdir,add,...}` present output like the
real POSIX commands they mirror, instead of inventing bespoke success/failure
decorations and leaking daemon internals.

## Background

The vault filesystem subcommands mirror standard Unix tools by name, so users
expect them to behave like the originals: `rm`/`mv`/`mkdir` are silent on
success, `ls` output looks like `ls`, errors go to stderr in POSIX style, and
nothing prints internal state (log height, manifest heads, inodes).

Today they don't. The clearest offender:

```
$ zim vault demo rm /aaa.txt
✗ removed /aaa.txt → height 8
```

A **successful** removal is rendered with the failure glyph (`ui::failure`) and
tacks on `→ height 8` (internal append-only-log height). Real `rm` prints
nothing on success. See `crates/zim/src/cli/ops/vault/rm.rs` — the `Display` impl
calls `ui::failure("removed", …)`.

This is a UX/consistency rule, not a one-line glyph swap: the whole family should
be aligned.

## Guiding rule

Each vault POSIX subcommand should match the UX of the real command it mirrors:

- `rm`, `mv`, `mkdir`, `add`(≈`cp`/write) — **silent on success**, non-zero exit
  + stderr message on failure.
- `ls` — columns/format close to real `ls`; `ls -l`-ish detail only behind a
  flag.
- `cat`, `head` — raw content to stdout, nothing else.
- No leaking of internals (`height`, manifest hash, inode numbers) in the
  human-facing path. Machine output belongs on the HTTP API, not a `--json` flag
  (per CLAUDE.md "Do Not").

Keep the Op pattern: Ops still return typed data (`RmOutput { path, height }`);
only the `Display` impls change. Internal fields can stay on the struct for the
API/tests even if the Display stops printing them.

## Files to Modify

- `crates/zim/src/cli/ops/vault/rm.rs` — `Display`: silent on success (empty
  output); stop using `ui::failure` and drop `→ height`.
- `crates/zim/src/cli/ops/vault/mv.rs` — silent on success.
- `crates/zim/src/cli/ops/vault/mkdir.rs` — silent on success.
- `crates/zim/src/cli/ops/vault/add.rs` — silent on success (or minimal, like
  `cp`).
- `crates/zim/src/cli/ops/vault/ls.rs` — confirm formatting mirrors `ls`.
- `crates/zim/src/cli/ops/vault/{cat,head}.rs` — confirm raw passthrough.
- `crates/zim/src/cli/ui.rs` — check `success`/`failure`/`num` helpers aren't
  forcing decoration where POSIX wants none.

## Intended outcomes

- `zim vault <t> rm <path>` prints nothing on success, exits 0.
- `mv`, `mkdir`, `add` are silent on success.
- No vault POSIX command prints `height`, manifest hash, or inodes on the
      human path.
- Failures print a POSIX-style message to stderr with a non-zero exit.
- `ls`/`cat`/`head` output matches their real counterparts closely enough to
      pipe into standard tools.
- `cargo test && cargo clippy -- -D warnings && cargo fmt -- --check` pass.

## Verification

```bash
./bin/dev cli alice vault demo rm /x.md   # no output, echo $? == 0
./bin/dev cli alice vault demo ls /        # looks like ls
./bin/dev cli alice vault demo cat /readme.md | wc -c   # raw bytes only
```

## Notes

Surfaced 2026-06-30 while cleaning up FUSE test files. Related to the broader
"make CLI errors/output better" ask.
