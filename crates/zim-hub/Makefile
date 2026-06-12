HUB_PORT ?= 8080

.PHONY: dev
dev: ## Run zim-hub dev server with hot reload (OAuth creds via confit, blobs via minio)
	@command -v cargo-watch >/dev/null 2>&1 || { \
		echo "cargo-watch not installed. Run: cargo install cargo-watch"; exit 1; \
	}
	@command -v confit >/dev/null 2>&1 || { \
		echo "confit not installed. Run:"; \
		echo "  curl -fsSL https://raw.githubusercontent.com/krondor-corp/confit/main/install.sh | bash"; \
		exit 1; \
	}
	@# Dev blob store: the minio container from bin/minio. Idempotent —
	@# `up` no-ops when it's already running, creates the zim-blobs
	@# bucket either way.
	@../../bin/minio up >/dev/null || { echo "minio failed to start — is docker running?"; exit 1; }
	@echo "Starting zim-hub on http://localhost:$(HUB_PORT) (blobs → minio :9000)"
	@# `confit run credentials.app --upper` injects GOOGLE_O_AUTH_CLIENT_ID
	@# and GOOGLE_O_AUTH_CLIENT_SECRET from 1Password into the child env,
	@# then we layer the zim-specific knobs on top before exec'ing
	@# cargo-watch.
	@confit run credentials.app --upper -- \
	 env RUST_LOG="$${RUST_LOG:-info,zim_hub=debug}" \
	     ZIM_HUB_LISTEN="$${ZIM_HUB_LISTEN:-127.0.0.1:$(HUB_PORT)}" \
	     ZIM_HUB_HOME="$${ZIM_HUB_HOME:-../../data/zim-hub}" \
	     ZIM_HUB_ADMIN_EMAILS="$${ZIM_HUB_ADMIN_EMAILS:-al@krondor.org}" \
	     ZIM_HUB_S3_ENDPOINT="$${ZIM_HUB_S3_ENDPOINT:-http://localhost:9000}" \
	     ZIM_HUB_S3_ACCESS_KEY="$${ZIM_HUB_S3_ACCESS_KEY:-minioadmin}" \
	     ZIM_HUB_S3_SECRET_KEY="$${ZIM_HUB_S3_SECRET_KEY:-minioadmin}" \
	     ZIM_HUB_S3_BUCKET="$${ZIM_HUB_S3_BUCKET:-zim-blobs}" \
	cargo watch \
		-w src \
		-w templates \
		-w static \
		-w Cargo.toml \
		-x "run -p zim-hub"
