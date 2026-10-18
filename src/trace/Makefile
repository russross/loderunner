.PHONY: all fmt lint test check mostlyclean clean
all:
	cargo build -p lode-trace
fmt:
	cargo fmt -p lode-trace --check
lint: fmt
	cargo clippy -p lode-trace --all-targets -- -D warnings
test:
	cargo test -p lode-trace
check: lint test
mostlyclean:
	@:
clean:
	cargo clean -p lode-trace
