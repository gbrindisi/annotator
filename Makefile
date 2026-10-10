.PHONY: help build web test lint fmt dev install clean

help: ## Show targets
	@grep -E '^[a-zA-Z_-]+:.*## ' $(MAKEFILE_LIST) | awk -F':.*## ' '{printf "  %-10s %s\n", $$1, $$2}'

web: ## Build the frontend into web/dist (embedded by the binary)
	cd web && npm ci && npm run build

build: web ## Release build with embedded frontend
	cargo build --release

test: ## Rust tests (+ frontend typecheck/tests if web deps are installed)
	cargo test
	@if [ -d web/node_modules ]; then cd web && npm test --if-present && npm run typecheck --if-present; fi

lint: ## clippy + fmt check
	cargo fmt --check
	cargo clippy --all-targets -- -D warnings

fmt: ## Format Rust code
	cargo fmt

dev: ## Run the daemon in the foreground (debug build reads web/dist from disk)
	cargo run -- serve --foreground

install: build ## Install the binary into ~/.cargo/bin
	cargo install --path .

clean: ## Remove build outputs
	cargo clean
	rm -rf web/dist/* web/node_modules
