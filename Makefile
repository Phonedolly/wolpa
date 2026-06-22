.PHONY: build run test clean gen-header lint

build:
	cargo build --release
	cd app && swift build -c release

run: build
	open app/.build/release/WolpaApp

test:
	cargo test
	cd app && swift test

lint:
	cargo clippy -- -D warnings
	cargo fmt --check

integration:
	cargo test -- --ignored

gen-header:
	cbindgen crates/wolpa-bridge -o crates/wolpa-bridge/wolpa.h

clean:
	cargo clean
	rm -rf app/.build
