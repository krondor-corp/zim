.PHONY: help
help: ## Show this help message
	@echo 'Usage: make [target]'
	@echo ''
	@echo 'Available targets:'
	@awk 'BEGIN {FS = ":.*##"; printf "\n"} /^[a-zA-Z_-]+:.*?##/ { printf "  %-20s %s\n", $$1, $$2 }' $(MAKEFILE_LIST)

.PHONY: install
install: ## Install dependencies
	@bundle install

# Dev port convention: all zim dev services live in the 17xxx band
# (see bin/dev_/nodes.toml). The site owns 1720x, livereload 1721x —
# separate bands so parallel worktrees can take +N slots in each
# without cross-colliding.
SITE_PORT ?= 17200
LIVERELOAD_PORT ?= 17210

.PHONY: dev
dev: ## Serve locally with live reload
	@bundle exec jekyll serve --port $(SITE_PORT) --livereload --livereload-port $(LIVERELOAD_PORT)

.PHONY: build
build: ## Build the site
	@bundle exec jekyll build

.PHONY: clean
clean: ## Remove build artifacts
	@bundle exec jekyll clean
