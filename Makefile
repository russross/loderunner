.PHONY: all fmt lint test check mostlyclean clean
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
mostlyclean:
	rm -rf target/debug/incremental
clean:
	cargo clean

