# walkie — every day-to-day command in one place. `make help` lists them.

.DEFAULT_GOAL := help
.PHONY: help dev debug install build package run ui ui-deps \
        fmt fmt-check lint test check test-stt test-e2e test-app test-all \
        model icons cert hooks tauri-cli release clean

APP_DIR := crates/walkie-app
UI_DIR := $(APP_DIR)/ui
SWIFT := $(APP_DIR)/swift

help: ## List targets
	@grep -E '^[a-z0-9-]+:.*## ' $(MAKEFILE_LIST) | awk -F':.*## ' '{printf "  \033[36m%-10s\033[0m %s\n", $$1, $$2}'

# --- Run ---------------------------------------------------------------------

dev: ## Build, install to /Applications, launch, follow the log
	scripts/dev.sh

debug: ## Same as dev, with key-event logging (WALKIE_DEBUG_EVENTS=1)
	scripts/dev.sh --debug

install: ## Build and install to /Applications without launching
	scripts/dev.sh --install-only

run: ## Run with hot reload (cargo tauri dev)
	cd $(APP_DIR) && cargo tauri dev

# --- Build -------------------------------------------------------------------

ui-deps:
	cd $(UI_DIR) && bun install --frozen-lockfile

# The Rust app embeds the built UI at compile time (tauri::generate_context!),
# so anything that compiles walkie-app needs it built first.
ui: ui-deps ## Build the Svelte UI into crates/walkie-app/dist
	cd $(UI_DIR) && bun run build

build: ## Bundle Walkie.app, signed with the local identity
	cd $(APP_DIR) && cargo tauri build

package: ## Ad-hoc-signed release zip (what CI ships)
	scripts/package.sh

# --- Quality -----------------------------------------------------------------

fmt: ui-deps ## Format all code (Rust + Swift + UI)
	cargo fmt --all
	xcrun swift-format format --in-place --recursive $(SWIFT)
	cd $(UI_DIR) && bun run fmt

fmt-check: ui-deps ## Fail if code isn't formatted
	cargo fmt --all --check
	for f in $(SWIFT)/*.swift; do xcrun swift-format format "$$f" | diff -u "$$f" - || exit 1; done
	cd $(UI_DIR) && bun run biome format .

lint: ui ## Clippy + swift-format + oxlint + Biome + svelte-check, warnings are errors
	cargo clippy --workspace --all-targets --all-features -- -D warnings
	xcrun swift-format lint --strict --recursive $(SWIFT)
	cd $(UI_DIR) && bun run lint
	cd $(UI_DIR) && bun run check

test: ui ## Fast unit tests (Rust + UI)
	cargo test
	cd $(UI_DIR) && bun run test

check: fmt-check lint test ## What CI runs: fmt-check + lint + test

# --- Slow tests --------------------------------------------------------------

test-stt: model ## Real Whisper on the en/es fixtures
	cargo test -p walkie-core --features stt-tests

test-e2e: model ## Keystrokes → engine → session → Whisper, in-process
	cargo test -p walkie-e2e

test-app: ## Drive the installed app through macOS (~30s, don't type)
	e2e/run-app-tests.sh

test-all: check test-stt test-e2e test-app ## Everything

# --- Setup -------------------------------------------------------------------

model: ## Download the base Whisper model (skipped if present)
	cargo run -q -p walkie-core --example fetch_model -- base

icons: ## Regenerate app icons
	cargo run -p walkie-app --example gen_icons
	cd crates/walkie-app && cargo tauri icon icons/icon.png

cert: ## Create the local signing identity (once per machine)
	scripts/create-signing-cert.sh

hooks: ## Enable the pre-commit hook (format + lint)
	git config core.hooksPath .githooks

tauri-cli: ## Install the Tauri CLI
	cargo install tauri-cli --locked

# --- Release -----------------------------------------------------------------

release: ## Bump, check, commit, tag and push: make release VERSION=x.y.z
	scripts/release.sh $(VERSION)

clean: ## Remove build output
	cargo clean
