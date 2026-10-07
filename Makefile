.PHONY: build release test lint fmt run iboard idesign list install clean help

# `make run` launches iboard; `make run idesign` switches to the design tool.
BIN := $(if $(filter idesign,$(MAKECMDGOALS)),idesign,iboard)

## Build the binary (debug)
build:
	cargo build

## Build the binary (optimized)
release:
	cargo build --release

## Run all tests
test:
	cargo test

## Check formatting and run clippy (fails on warnings)
lint:
	cargo fmt --all --check
	cargo clippy --all-targets -- -D warnings

## Format code in place
fmt:
	cargo fmt

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
