.PHONY: build release check test lint fmt toolchain hooks run iboard idesign list install clean help

# `make run` launches iboard; `make run idesign` switches to the design tool.
BIN := $(if $(filter idesign,$(MAKECMDGOALS)),idesign,iboard)

SWIFT_FORMAT := $(shell xcrun --find swift-format 2>/dev/null)
PINNED_RUST := $(shell sed -n 's/^channel = "\(.*\)"/\1/p' rust-toolchain.toml)

## Build the binary (debug)
build:
	cargo build

## Build the binary (optimized)
release:
	cargo build --release

## Everything a ticket needs before it is done: formatting, lints and tests
check: toolchain lint test

## Run all tests
test:
	cargo test

## Check formatting, clippy, an egui-free core and swift-format (fails on warnings)
lint:
	cargo fmt --all --check
	cargo clippy --all-targets -- -D warnings
	cargo check --lib --no-default-features
ifneq ($(SWIFT_FORMAT),)
	$(SWIFT_FORMAT) lint --strict macos/*.swift
endif

## Format code in place
fmt:
	cargo fmt
ifneq ($(SWIFT_FORMAT),)
	$(SWIFT_FORMAT) format --in-place macos/*.swift
endif

toolchain:
	@case "$$(rustc --version)" in \
		"rustc $(PINNED_RUST)"*) ;; \
		*) echo "warning: $$(rustc --version) is not the pinned $(PINNED_RUST) (rust-toolchain.toml); new lints may appear" >&2 ;; \
	esac

## Lint the staged snapshot before each commit (installs .githooks)
hooks:
	git config core.hooksPath .githooks

## Launch a binary, iboard by default: make run, make run iboard, make run idesign
run:
	cargo run --bin $(BIN) -- $(ARGS)

## Launch the board directly, e.g. make iboard ARGS="~/code/moon-cull"
iboard:
	@if [ -z "$(filter run,$(MAKECMDGOALS))" ]; then \
		cargo run --bin iboard -- $(ARGS); \
	fi

## Launch the design tool directly, e.g. make idesign ARGS="show"
idesign:
	@if [ -z "$(filter run,$(MAKECMDGOALS))" ]; then \
		cargo run --bin idesign -- $(ARGS); \
	fi

## Print the board to stdout, e.g. make list ARGS="~/code/moon-cull"
list:
	cargo run --bin iboard -- --list $(ARGS)

## Install the binaries into the Cargo bin directory
install:
	cargo install --path .

## Remove build artifacts
clean:
	cargo clean

## Show this help
help:
	@awk '/^## /{d=substr($$0,4);next} /^[a-zA-Z_-]+:/&&d{split($$0,t,":");printf "  %-10s %s\n",t[1],d} {d=""}' $(MAKEFILE_LIST)
