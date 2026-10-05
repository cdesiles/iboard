.PHONY: build release test lint fmt run list install clean help

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

## Launch the board as a GUI, e.g. make run ARGS="~/code/moon-cull"
run:
	cargo run -- $(ARGS)

## Print the board to stdout, e.g. make list ARGS="~/code/moon-cull"
list:
	cargo run -- --list $(ARGS)

## Install the `iboard` binary into the Cargo bin directory
install:
	cargo install --path .

## Remove build artifacts
clean:
	cargo clean

## Show this help
help:
	@awk '/^## /{d=substr($$0,4);next} /^[a-zA-Z_-]+:/&&d{split($$0,t,":");printf "  %-10s %s\n",t[1],d} {d=""}' $(MAKEFILE_LIST)
