ARGS ?=

.PHONY: help
help: ## Show this help message
	@echo 'Usage: make [target] [ARGS=...]'
	@echo ''
	@echo 'Common:'
	@echo '  install        Install the `zim` binary into ~/.cargo/bin (release).'
	@echo '                 Pass ARGS="--dev" to symlink the debug build instead.'
	@echo '  uninstall      Remove ~/.cargo/bin/zim. ARGS="--purge" also deletes $$ZIM_HOME.'
	@echo '  dev            Spawn 2 daemons in tmux for local sync testing.'
	@echo ''
	@echo 'Build / test:'
	@echo '  build          cargo build --workspace'
	@echo '  test           cargo test --workspace'
	@echo '  check          cargo check --workspace'
	@echo '  lint           cargo clippy --workspace -- -D warnings'
	@echo '  fmt            cargo fmt --all'
	@echo '  fmt-check      cargo fmt --all -- --check'
	@echo ''
	@echo 'Misc:'
	@echo '  deps           cargo fetch (download dependencies)'
	@echo '  clean          cargo clean'

.PHONY: dev
dev: ## Start the local dev environment (2 daemons in tmux)
	./bin/dev $(ARGS)

.PHONY: hub
hub: ## Start zim-hub dev server with hot reload
	$(MAKE) -C crates/zim-hub dev

.PHONY: check
check: ## Check all Rust code
	cargo check --workspace

.PHONY: install
install: ## Install the `zim` binary into ~/.cargo/bin (ARGS=--dev for symlink)
	./bin/install $(ARGS)

.PHONY: uninstall
uninstall: ## Remove the `zim` binary (ARGS=--purge to delete $ZIM_HOME)
	./bin/uninstall $(ARGS)

.PHONY: deps
deps: ## Download dependencies into the cargo cache
	cargo fetch

.PHONY: build
build: ## Build all Rust packages
	cargo build --workspace

.PHONY: test
test: ## Run all tests
	cargo test --workspace

.PHONY: lint
lint: ## Run clippy linter
	cargo clippy --workspace -- -D warnings

.PHONY: fmt
fmt: ## Format code
	cargo fmt --all

.PHONY: fmt-check
fmt-check: ## Check code formatting
	cargo fmt --all -- --check

.PHONY: types
types: ## Run type checking (alias for check)
	@$(MAKE) check

.PHONY: clean
clean: ## Clean build artifacts
	cargo clean
