.PHONY: build run test clean gen-header lint dev dev-run

CARGO_TARGET_DIR = $(CURDIR)/target

build:
	cargo build --release
	cd app && swift build -c release \
		-Xlinker -L$(CARGO_TARGET_DIR)/release \
		-Xlinker -lwolpa_bridge

run: build
	open app/.build/release/WolpaApp

dev:
	cargo build
	cd app && swift build \
		-Xlinker -L$(CARGO_TARGET_DIR)/debug \
		-Xlinker -lwolpa_bridge

dev-run: dev
	open app/.build/debug/WolpaApp

test:
	cargo test
	cd app && swift test

lint:
	cargo clippy -- -D warnings
	cargo fmt --check

integration:
	cargo test -- --ignored

gen-header:
	cbindgen --config crates/wolpa-bridge/cbindgen.toml crates/wolpa-bridge -o crates/wolpa-bridge/wolpa.h

clean:
	cargo clean
	rm -rf app/.build
