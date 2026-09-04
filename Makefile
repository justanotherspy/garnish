.PHONY: all build release check lint fmt test doc docs bench install ci clean

all: check

build:
	cargo build

release:
	cargo build --release

check: lint test

lint:
	cargo fmt --check
	cargo clippy --all-targets --all-features -- -D warnings

fmt:
	cargo fmt

test:
	cargo nextest run
	cargo test --doc

doc:
	cargo doc --no-deps

docs: release
	./target/release/garnish docs --out docs

bench: release
	./bench/run.sh

install:
	cargo install --path . --locked

ci:
	./scripts/ci.sh

clean:
	cargo clean
