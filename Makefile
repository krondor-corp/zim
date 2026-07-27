ARGS ?=
# Feature selection for test/check/lint. CI overrides per matrix
# variant (--all-features vs --no-default-features).
CARGO_FEATURES ?= --all-features

.PHONY: help
help: ## Show this help message
	@echo 'Usage: make [target] [ARGS=...]'
	@echo ''
	@echo 'Common:'
	@echo '  install        Install the `zim` binary into ~/.cargo/bin (release).'
	@echo '  install-dev    Symlink ~/.cargo/bin/zim to the debug build: isolated'
	@echo '                 data dir (~/.config/zim/debug), debug logs, `zim clean`.'
	@echo '  uninstall      Remove ~/.cargo/bin/zim. uninstall-purge also deletes $$ZIM_HOME.'
	@echo '  up             Whole stack: daemons + hub + minio + fixtures + enroll.'
	@echo '                 (ZIM_DEV_FUSE=1 make up to include FUSE mounts.)'
	@echo '  dev            Spawn 2 daemons in tmux for local sync testing.'
	@echo '                 Subcommands pass through: `make dev hub up`,'
	@echo '                 `make dev status`; flags via ARGS="-b --fuse".'
	@echo '  e2e            Hermetic e2e verdict (zim-e2e crate): daemons on'
	@echo '                 1722x, fixtures, sync convergence. Exit code = result.'
	@echo ''
	@echo 'Build / test:'
	@echo '  build          cargo build --workspace'
	@echo '  build-web      Build the hub SPA (trunk) into crates/zim-hub/web/dist.'
	@echo '                 Pass ARGS="--release" for an optimized bundle.'
	@echo '  test           cargo test --workspace'
	@echo '  check          full quality gate: fmt-check + clippy + cargo check (CI)'
	@echo '  lint           cargo clippy --workspace -- -D warnings'
	@echo '  fmt            cargo fmt --all'
	@echo '  fmt-check      cargo fmt --all -- --check'
	@echo ''
	@echo 'Misc:'
	@echo '  deps           cargo fetch (download dependencies)'
	@echo '  clean          cargo clean'
	@echo '  cleanup        Free disk: cargo target + site build artifacts'
	@echo '  cleanup-all    cleanup + wipe dev data (./data) and minio state'

.PHONY: dev
dev: ## Start the local dev environment (2 daemons in tmux)
	./bin/dev $(DEV_ARGS) $(ARGS)

.PHONY: e2e
e2e: ## One-shot hermetic e2e run (zim-e2e crate; own ports, own data)
	cargo build -p zim-cli --features hub,fuse
	cargo run -q -p zim-e2e

.PHONY: up
up: ## The whole dev stack: daemons + hub + minio + fixtures + enroll
	./bin/dev --hub

.PHONY: hub
hub: ## Start the hub in the dev tmux session (minio + OAuth via confit)
	./bin/dev hub up

.PHONY: check
check: ## Run the quality gate: fmt-check, clippy, cargo check (what CI runs)
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets $(CARGO_FEATURES) -- -D warnings
	cargo check --workspace --all-targets $(CARGO_FEATURES)

# `--locked` matters: `cargo install` re-resolves deps without it and can
# land on broken pre-release versions (notably ed25519-dalek 3.0.0-pre.1
# vs ed25519 3.0.0-rc.4). `cargo build` already respects Cargo.lock.
.PHONY: install
install: ## Install the `zim` binary into ~/.cargo/bin (release)
	cargo install --path crates/zim --bin zim --force --locked
	@echo "Run 'zim --help' to see all commands."

# Debug installs are isolated: data dir ~/.config/zim/debug, DEBUG logs,
# `zim clean` to wipe. Iterate with `cargo build -p zim-cli`, re-run `zim`.
.PHONY: install-dev
install-dev: ## Symlink ~/.cargo/bin/zim to the debug build (fast iteration)
	cargo build -p zim-cli
	@bin="$${CARGO_HOME:-$$HOME/.cargo}/bin"; mkdir -p "$$bin"; \
	 rm -f "$$bin/zim"; ln -s "$(CURDIR)/target/debug/zim" "$$bin/zim"; \
	 echo "Installed (dev symlink): $$bin/zim -> target/debug/zim"

.PHONY: uninstall
uninstall: ## Remove the `zim` binary (dev symlink or release install)
	@bin="$${CARGO_HOME:-$$HOME/.cargo}/bin/zim"; \
	 if [ -L "$$bin" ]; then rm -f "$$bin"; echo "Removed dev symlink: $$bin"; \
	 elif [ -x "$$bin" ]; then cargo uninstall zim 2>/dev/null || rm -f "$$bin"; echo "Removed: $$bin"; \
	 else echo "Nothing at: $$bin"; fi

.PHONY: uninstall-purge
uninstall-purge: uninstall ## Also delete $ZIM_HOME (default ~/.zim)
	@home="$${ZIM_HOME:-$$HOME/.zim}"; \
	 if [ -d "$$home" ]; then rm -rf "$$home"; echo "Purged: $$home"; fi

.PHONY: deps
deps: ## Download dependencies into the cargo cache
	cargo fetch

.PHONY: build
build: ## Build all Rust packages
	cargo build --workspace

# The SPA build MUST run before launching the hub, every time — otherwise
# the hub serves whatever stale bundle is on disk. It can't live in the
# hub's build.rs: trunk shells out to cargo, and a nested cargo invocation
# deadlocks on the outer build's lock.
.PHONY: build-web
build-web: ## Build the hub's Yew SPA into crates/zim-hub/web/dist (ARGS=--release to optimize)
	@command -v trunk >/dev/null 2>&1 || { echo "trunk not installed. Run: cargo install trunk" >&2; exit 1; }
	@echo "Building web SPA (trunk)…" >&2
	@cd crates/zim-hub/web && trunk build $(ARGS) >&2
	@bundle=$$(grep -oE 'zim-hub-web-[a-f0-9]+\.js' crates/zim-hub/web/dist/index.html 2>/dev/null | head -1); \
	 if [ -n "$$bundle" ]; then echo "web SPA built: $$bundle" >&2; fi

.PHONY: test
test: ## Run all tests
	cargo test --workspace $(CARGO_FEATURES)

.PHONY: lint
lint: ## Run clippy linter
	cargo clippy --workspace --all-targets $(CARGO_FEATURES) -- -D warnings

.PHONY: fmt
fmt: ## Format code
	cargo fmt --all

.PHONY: fmt-check
fmt-check: ## Check code formatting
	cargo fmt --all -- --check

.PHONY: clean
clean: ## Clean build artifacts
	cargo clean

.PHONY: cleanup
cleanup: ## Free disk: cargo target + site build artifacts
	cargo clean
	rm -rf web/_site web/vendor web/.jekyll-cache
	rm -f web/Gemfile.lock

.PHONY: cleanup-all
cleanup-all: cleanup ## Also wipe dev data (./data) and minio local state
	rm -rf data .minio

# `make dev <words...>` — words after `dev` pass through to ./bin/dev
# (`make dev hub up`, `make dev status`). Lives at the BOTTOM of the
# file so the no-op goals declared here override any real targets the
# words collide with (clean, e2e, hub, help) — otherwise `make dev
# clean` would also run cargo clean. The override warning is expected.
# Flags can't ride this way (make parses leading dashes itself) — use
# ARGS for those: `make dev run ARGS="-b --fuse"`.
ifeq (dev,$(firstword $(MAKECMDGOALS)))
  DEV_ARGS := $(wordlist 2,$(words $(MAKECMDGOALS)),$(MAKECMDGOALS))
  $(eval $(DEV_ARGS):;@:)
endif
