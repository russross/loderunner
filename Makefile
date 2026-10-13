.PHONY: all fmt lint test check mame-poc mostlyclean clean
all:
	cargo build --workspace
fmt:
	cargo fmt --all --check
lint:
	cargo clippy --workspace --all-targets -- -D warnings
test:
	cargo test --workspace
check:
	$(MAKE) fmt
	$(MAKE) lint
	$(MAKE) test
mame-poc:
	$(MAKE) -C sources/mame-poc check
mostlyclean:
	rm -rf target/debug/incremental
clean:
	cargo clean

