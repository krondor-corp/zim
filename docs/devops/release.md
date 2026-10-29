# Release Process

This document describes how to release new versions of the Zim crates using `cargo-smart-release`.

## Overview

We use [cargo-smart-release](https://crates.io/crates/cargo-smart-release) to:
- Automatically bump crate versions based on conventional commits
- Generate and update CHANGELOGs
- Create git tags per crate
- Keep workspace dependencies in sync

Publishing to crates.io is handled automatically by GitHub Actions when tags are pushed.

## Prerequisites

1. **Install cargo-smart-release** (if not already installed):
   ```bash
   cargo install cargo-smart-release
   ```

2. **Use conventional commits** for your changes:
   - `feat:` - New feature (minor version bump, e.g., 0.1.0 → 0.2.0)
   - `fix:` - Bug fix (patch version bump, e.g., 0.1.0 → 0.1.1)
   - `feat!:` or `BREAKING CHANGE:` - Breaking change (major version bump, e.g., 0.1.0 → 1.0.0)
   - `docs:`, `chore:`, `refactor:`, etc. - No version bump, but included in changelog

## When to Release

### Automated via Release PR (Recommended)

This is the recommended approach:

1. **Merge PRs to main** with conventional commit messages
   - The `release-pr.yml` workflow automatically runs after each merge
   - It creates/updates a PR on the `release-automation` branch
   - The PR accumulates all version bumps and changelog updates

2. **Review the Release PR**
   - Check the version bumps in `Cargo.toml` files
   - Review the `CHANGELOG.md` updates
   - Verify conventional commits are categorized correctly

3. **Merge the Release PR**
   - This pushes the git tags to the repository
   - GitHub Actions automatically publishes to crates.io via `publish-crate.yml`

4. **Manual trigger** (if needed):
   - Go to Actions → "Create Release PR" → "Run workflow"
   - Useful if the automatic workflow didn't trigger

### Manual Release (Alternative)

If you prefer to release manually from the command line:

1. **Make sure all changes are committed and pushed**
   ```bash
   git status  # Should be clean
   ```

2. **Preview what would happen** (dry-run):
   ```bash
   cargo smart-release zim-peer -v
   ```

   This shows:
   - Which versions will be bumped
   - What changelog entries will be generated
   - What git tags will be created

3. **Execute the release**:
   ```bash
   cargo smart-release zim-peer --execute --no-publish
   ```

   This will:
   - Update version numbers in all Cargo.toml files
   - Update CHANGELOGs based on commits since last release
   - Create git tags (e.g., `zim-core-v0.1.1`, `zim-core-v0.1.1`, `zim-v0.1.1`)
   - Commit the changes with message like "release"
   - Push tags and commits to GitHub
   - **Note**: It does NOT publish to crates.io (that's handled by GitHub Actions)

4. **GitHub Actions will automatically**:
   - Trigger on the new tags
   - Run tests
   - Publish updated crates to crates.io

## Flags Explained

- `--execute` - Actually perform the release (without this, it's a dry-run)
- `--no-publish` - Don't run `cargo publish` (we let GitHub Actions do that)
- `-v` - Verbose output showing what would happen
- `--allow-dirty` - Allow releasing with uncommitted changes (not recommended)
- `--update-crates-index` - Update local crates.io index first (good for accuracy)

## Releasing Individual Crates

If you only want to bump a specific crate (not the whole workspace):

```bash
# Release only zim-core
cargo smart-release zim-core --execute --no-publish

# Release only zim-core (will also bump deps if unreleased changes)
cargo smart-release zim-core --execute --no-publish
```

The tool will automatically determine which dependencies need to be updated.

## Manual Version Bumps

If you want to force a specific version bump instead of auto-detection:

```bash
# Force a minor version bump
cargo smart-release zim-peer --execute --no-publish --bump minor

# Force a patch version bump
cargo smart-release zim-peer --execute --no-publish --bump patch

# Force a major version bump
cargo smart-release zim-peer --execute --no-publish --bump major
```

## Editing Changelogs Manually

If the auto-generated changelog is empty or needs tweaking:

1. Run the release command (it will stop if changelog is empty)
2. Manually edit the CHANGELOG.md files in each crate directory
3. Re-run the release command

Or use:
```bash
cargo changelog --write zim-peer
```

## How the Automation Works

The automated release process is implemented in `.github/workflows/release-pr.yml`:

1. **Triggered on push to main** (after PR merges) or manual workflow dispatch
2. **Checks if release is needed** by running `cargo smart-release` dry-run
3. **Creates/updates release branch** (`release-automation`)
4. **Runs cargo-smart-release** with `--execute --no-publish --allow-dirty`
   - Bumps versions based on conventional commits
   - Updates CHANGELOGs
   - Creates git tags locally
5. **Opens or updates a PR** showing all changes for review
6. **On merge**: Tags are pushed, triggering `publish-crate.yml` to publish to crates.io

### Benefits of Automated Approach
- No need to manually run `cargo smart-release`
- Changes accumulate automatically after each merge
- Full audit trail via PR review
- One merge triggers the entire release process
- Can still manually trigger via GitHub Actions UI

## Troubleshooting

### "Changelog is empty"
Add more descriptive commit messages using conventional commits format, or manually edit the CHANGELOG.md files.

### "Working tree has changes"
Commit or stash your changes first. Use `--allow-dirty` only if absolutely necessary.

### "Crate already published"
The tool detects if a version is already on crates.io. You may need to bump to a higher version.

### Tags not pushing
Make sure you have push permissions to the repository. Check `git remote -v` to confirm the remote URL.

## Files

- `release.toml` - Configuration for cargo-smart-release
- `crates/*/CHANGELOG.md` - Per-crate changelogs
- `.github/workflows/release-pr.yml` - Creates automated release PRs
- `.github/workflows/publish-crate.yml` - Publishes to crates.io when tags are pushed

## Examples

### Typical Release Flow (Automated)

```bash
# 1. Make changes and use conventional commits
git commit -m "feat: add new file upload feature"

# 2. Merge to main (via PR or direct push)
git push origin main

# 3. GitHub Actions automatically creates/updates a Release PR
# Check: https://github.com/YOUR_ORG/Zim/pulls

# 4. Review the Release PR:
#    - Verify version bumps are correct
#    - Check CHANGELOG updates
#    - Ensure tags are created

# 5. Merge the Release PR
#    → Tags pushed → crates.io publish triggered automatically
```

### Manual Release (Alternative)

```bash
# 1. Check what's changed
git log --oneline

# 2. Dry-run to preview
cargo smart-release zim-peer -v

# 3. Execute if everything looks good
cargo smart-release zim-peer --execute --no-publish

# 4. Wait for GitHub Actions to publish to crates.io
# Check: https://github.com/zim/zim/actions
```

### Emergency Hotfix

```bash
# Make your fix with a conventional commit
git commit -m "fix: critical security issue in authentication"
git push origin main

# Option 1: Let automation handle it
# → Release PR will be created/updated automatically

# Option 2: Manually trigger via GitHub Actions
# Go to Actions → "Create Release PR" → "Run workflow"

# Option 3: Manual release (fastest)
cargo smart-release zim-peer --execute --no-publish --bump patch
```

## Desktop app

The desktop app (`crates/zim-desktop`) is released in lockstep with the `zim` CLI, not on its own train. It bundles the `zim` daemon as its sidecar, so a `zim-v*` tag is a desktop release too.

- **Tag → release.** `release-cli.yml` builds the CLI binaries and creates the GitHub Release for the tag. `release-desktop.yml` runs on the same tag, builds the bundles, then waits for that release to exist and uploads to it (`gh release upload --clobber`). It never creates the release, so the two workflows can't race. The release body's desktop table is written by `release-cli.yml`.
- **Version.** Stamped from the tag at build time (`cargo tauri build --config '{"version":"<tag version>"}'`). `crates/zim-desktop/tauri.conf.json` and its `Cargo.toml` carry placeholder versions; there is nothing to bump and release-plz needs no changes. The crate is outside the workspace, so release-plz doesn't see it.
- **Artifacts.** macOS Apple Silicon `.dmg` (and a `_fuse` `.dmg` whose sidecar is built with `--features hub,fuse`; needs macFUSE), Linux x86_64 `.deb` and `.AppImage` (built on Ubuntu 22.04 for glibc compatibility).
- **Per build.** `make desktop-sidecar PROFILE=release FEATURES=<features>` builds `zim` into `crates/zim-desktop/binaries/zim-<triple>`, then `cargo tauri build` runs `trunk build` for the frontend and bundles. `tauri-cli` is installed from crates.io (pinned, cached).
- **Not signed.** No code signing or notarization yet: macOS users right-click → Open the first time (Gatekeeper).
- **Manual run.** Actions → "Release Desktop" → "Run workflow" with a version, for a `zim-v<version>` release that already exists.
- **CI.** `ci-tauri.yml` (called from `ci.yml`, part of CI Success) runs fmt, clippy and tests for the crate on Ubuntu. It doesn't bundle.
