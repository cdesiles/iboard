.DEFAULT_GOAL := help
.PHONY: help hooks run iboard idesign list check test third-party lint fmt tokens toolchain build release install app install-app bump clean

# `make run` launches iboard; `make run idesign` switches to the design tool.
BIN := $(if $(filter idesign,$(MAKECMDGOALS)),idesign,iboard)

SWIFT_FORMAT := $(shell xcrun --find swift-format 2>/dev/null)
OPENCODE := $(shell command -v opencode 2>/dev/null)
PINNED_RUST := $(shell sed -n 's/^channel = "\(.*\)"/\1/p' rust-toolchain.toml)
VERSION := $(shell sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml)
NEXT_VERSION = $(shell echo $(VERSION) | awk -F. -v to='$(TO)' '\
	to == "major" { printf "%d.0.0", $$1 + 1 } \
	to == "minor" { printf "%d.%d.0", $$1, $$2 + 1 } \
	to == "patch" { printf "%d.%d.%d", $$1, $$2, $$3 + 1 } \
	to ~ /^[0-9]+\.[0-9]+\.[0-9]+$$/ { print to }')
APP_DIR ?= $(HOME)/Applications

help: ## Show this help
	@awk 'BEGIN {FS = ":.*## "; print "\nUsage:\n  make \033[36m<target>\033[0m"} \
		/^[a-zA-Z_-]+:.*## / {printf "  \033[36m%-10s\033[0m %s\n", $$1, $$2} \
		/^##@ / {printf "\n\033[1m%s\033[0m\n", substr($$0, 5)}' $(MAKEFILE_LIST)
	@echo

##@ Setup

hooks: ## Lint the staged snapshot before each commit (installs .githooks)
	git config core.hooksPath .githooks

##@ Run

run: ## Launch a binary, iboard by default: make run, make run iboard, make run idesign
	cargo run --bin $(BIN) -- $(ARGS)

iboard: ## Launch the board directly, e.g. make iboard ARGS="~/code/moon-cull"
	@if [ -z "$(filter run,$(MAKECMDGOALS))" ]; then \
		cargo run --bin iboard -- $(ARGS); \
	fi

idesign: ## Launch the design tool directly, e.g. make idesign ARGS="show"
	@if [ -z "$(filter run,$(MAKECMDGOALS))" ]; then \
		cargo run --bin idesign -- $(ARGS); \
	fi

list: ## Print the board to stdout, e.g. make list ARGS="~/code/moon-cull"
	cargo run --bin iboard -- --list $(ARGS)

##@ Quality

check: toolchain lint test ## Everything a ticket needs before it is done: formatting, lints and tests

test: ## Run every test that needs no third-party service
	IBOARD_OPENCODE= cargo test

third-party: ## Run the tests that need the real opencode service (before a release)
	@test -n "$(OPENCODE)" || { echo 'opencode is not installed' >&2; exit 2; }
	IBOARD_OPENCODE="$(OPENCODE)" cargo test --lib third_party -- --ignored

lint: ## Check formatting, clippy, an egui-free core and swift-format (fails on warnings)
	cargo fmt --all --check
	cargo clippy --all-targets -- -D warnings
	cargo check --lib --no-default-features
ifneq ($(SWIFT_FORMAT),)
	$(SWIFT_FORMAT) lint --strict macos/*.swift
endif

tokens: ## Regenerate src/shell/theme/tokens.rs from design/tokens.json
	cargo run --bin idesign -- tokens --rust

fmt: ## Format code in place
	cargo fmt
ifneq ($(SWIFT_FORMAT),)
	$(SWIFT_FORMAT) format --in-place macos/*.swift
endif

toolchain:
	@case "$$(rustc --version)" in \
		"rustc $(PINNED_RUST)"*) ;; \
		*) echo "warning: $$(rustc --version) is not the pinned $(PINNED_RUST) (rust-toolchain.toml); new lints may appear" >&2 ;; \
	esac

##@ Build

build: ## Build the binaries (debug)
	cargo build

release: ## Build the binaries (optimized)
	cargo build --release

install: install-app ## Install the CLI commands and the iboard.app bundle
	cargo install --path .

clean: ## Remove build artifacts
	cargo clean

##@ App

app: ## Build iboard.app into target/iboard.app
	cargo build --release --bin iboard
	sh macos/bundle.sh target/release/iboard design/components/Logo/files/iboard-1024.png $(VERSION) target/iboard.app

install-app: app ## Install target/iboard.app into ~/Applications (or $(APP_DIR))
	mkdir -p "$(APP_DIR)"
	rm -rf "$(APP_DIR)/iboard.app"
	cp -R target/iboard.app "$(APP_DIR)/iboard.app"
	@echo "Installed $(APP_DIR)/iboard.app"

##@ Version

bump: ## Set the version, commit and tag it: make bump TO=patch, minor, major or x.y.z
	@test -n "$(NEXT_VERSION)" || { echo 'usage: make bump TO=patch|minor|major|x.y.z' >&2; exit 2; }
	@git diff --quiet HEAD -- Cargo.toml Cargo.lock || { echo 'Cargo.toml or Cargo.lock has uncommitted changes' >&2; exit 1; }
	@if git rev-parse --quiet --verify refs/tags/v$(NEXT_VERSION) >/dev/null; then echo 'v$(NEXT_VERSION) is already tagged' >&2; exit 1; fi
	sed -i.bak 's/^version = ".*"/version = "$(NEXT_VERSION)"/' Cargo.toml && rm Cargo.toml.bak
	cargo update --workspace
	git commit -m 'Release $(NEXT_VERSION)' -- Cargo.toml Cargo.lock
	git tag -a v$(NEXT_VERSION) -m 'iboard $(NEXT_VERSION)'
	@echo 'Push it with: git push --follow-tags'
