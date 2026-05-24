.PHONY: help
help: ## Show this help message
	@echo 'Usage: make [target]'
	@echo ''
	@echo 'Available targets:'
	@awk 'BEGIN {FS = ":.*##"; printf "\n"} /^[a-zA-Z_-]+:.*?##/ { printf "  %-20s %s\n", $$1, $$2 }' $(MAKEFILE_LIST)

.PHONY: install
install: ## Install dependencies
	@bundle install

.PHONY: dev
dev: ## Serve locally with live reload
	@bundle exec jekyll serve --livereload

.PHONY: build
build: ## Build the site
	@bundle exec jekyll build

.PHONY: clean
clean: ## Remove build artifacts
	@bundle exec jekyll clean
