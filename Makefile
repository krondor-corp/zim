ARGS ?=

.PHONY: help
help: ## Show this help message
	@echo 'Usage: make [target]'
	@echo ''
	@echo 'Available targets:'
	@echo '  hub: Run zim-hub dev server with hot reload (http://localhost:$(HUB_PORT))'
	@echo '  dev: Start development environment with two nodes in tmux'
	@echo '  check: Check all Rust code'
	@echo '  install: Install dependencies'
	@echo '  build: Build all Rust packages'
	@echo '  test: Run all tests'
	@echo '  lint: Run clippy linter'
	@echo '  fmt: Format code'
	@echo '  fmt-check: Check code formatting'
	@echo '  clean: Clean build artifacts'

HUB_PORT ?= 8080

.PHONY: hub
hub: ## Run zim-hub dev server with hot reload
	@command -v cargo-watch >/dev/null 2>&1 || { \
		echo "cargo-watch not installed. Run: cargo install cargo-watch"; exit 1; \
	}
	@bash -c '\
		export RUST_LOG="$${RUST_LOG:-info,zim_hub=debug}" \
		       ZIM_HUB_LISTEN="$${ZIM_HUB_LISTEN:-127.0.0.1:$(HUB_PORT)}" \
		       ZIM_HUB_PEER="$${ZIM_HUB_PEER:-http://127.0.0.1:3001}"; \
		echo "Starting zim-hub on http://localhost:$(HUB_PORT) (peer: $$ZIM_HUB_PEER)" && \
		cargo watch \
			-w crates/zim-hub/src \
			-w crates/zim-hub/templates \
			-w crates/zim-hub/static \
			-w crates/zim-hub/Cargo.toml \
			-x "run -p zim-hub" \
	'

.PHONY: dev
dev: ## Start development environment with two nodes in tmux
	./bin/dev

.PHONY: check
check: ## Check all Rust code
	cargo check --all

.PHONY: install
install: ## Install dependencies
	cargo fetch

.PHONY: build
build: ## Build all Rust packages
	cargo build --all

.PHONY: test
test: ## Run all tests
	cargo test --all

.PHONY: lint
lint: ## Run clippy linter
	cargo clippy --all -- -D warnings

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