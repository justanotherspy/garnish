.PHONY: all build release check lint fmt test doc docs bench install ci clean watch

all: check

# Re-run lint + tests whenever a source or test file changes. Output goes to
# the terminal and to target/watch.log so the Monitor tool can follow it.
watch:
	watchexec -c clear --debounce 2s -w src -w tests -w Cargo.toml -w benches \
		--shell=bash -- 'cargo clippy --all-targets --all-features -- -D warnings 2>&1 | tail -30 && cargo nextest run 2>&1 | grep -E "FAIL|Summary|panicked"; echo "== watch run done $$(date +%T)"' \
		2>&1 | tee target/watch.log

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
