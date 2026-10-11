.PHONY: help build web test lint fmt dev install uninstall release clean

BINDIR ?= $(HOME)/bin

help: ## Show targets
	@grep -E '^[a-zA-Z_-]+:.*## ' $(MAKEFILE_LIST) | awk -F':.*## ' '{printf "  %-10s %s\n", $$1, $$2}'

web: ## Build the frontend into web/dist (embedded by the binary)
	cd web && npm ci && npm run build

build: web ## Release build with embedded frontend
	@touch src/assets.rs  # force re-embedding web/dist (hashed asset names)
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

install: build ## Install the binary into $(BINDIR) (default ~/bin) and restart the daemon
	install -d $(BINDIR)
	install -m 0755 target/release/annotator $(BINDIR)/annotator
	@# A running daemon keeps the old binary; stop it (waiting clients respawn it transparently).
	-@$(BINDIR)/annotator stop >/dev/null 2>&1 || true
	@echo "installed $$($(BINDIR)/annotator --version) to $(BINDIR)/annotator"

uninstall: ## Remove the binary from $(BINDIR)
	-@$(BINDIR)/annotator stop >/dev/null 2>&1 || true
	rm -f $(BINDIR)/annotator

release: ## Cut a release: make release VERSION=X.Y.Z (see RELEASING.md)
	@test -n "$(VERSION)" || { echo "usage: make release VERSION=X.Y.Z"; exit 1; }
	scripts/release.sh $(VERSION)

clean: ## Remove build outputs
	cargo clean
	rm -rf web/dist/* web/node_modules
