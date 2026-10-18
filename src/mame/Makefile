.PHONY: all fmt lint test check mostlyclean clean
all:
	$(MAKE) -C ../.. all
fmt:
	cargo fmt -p lode-mame-validation --check
lint: fmt
	cargo clippy -p lode-mame-validation --all-targets -- -D warnings
test:
	$(MAKE) -C ../.. mame-validation
check: lint test
mostlyclean:
	@:
clean:
	cargo clean -p lode-mame-validation
