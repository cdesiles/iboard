.DEFAULT_GOAL := help
.PHONY: help hooks run iboard idesign list check test lint fmt toolchain build release install clean

# `make run` launches iboard; `make run idesign` switches to the design tool.
BIN := $(if $(filter idesign,$(MAKECMDGOALS)),idesign,iboard)

SWIFT_FORMAT := $(shell xcrun --find swift-format 2>/dev/null)
PINNED_RUST := $(shell sed -n 's/^channel = "\(.*\)"/\1/p' rust-toolchain.toml)

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

test: ## Run all tests
	cargo test

lint: ## Check formatting, clippy, an egui-free core and swift-format (fails on warnings)
	cargo fmt --all --check
	cargo clippy --all-targets -- -D warnings
	cargo check --lib --no-default-features
ifneq ($(SWIFT_FORMAT),)
	$(SWIFT_FORMAT) lint --strict macos/*.swift
endif

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

install: ## Install the binaries into the Cargo bin directory
	cargo install --path .

clean: ## Remove build artifacts
	cargo clean
