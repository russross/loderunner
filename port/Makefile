.PHONY: all fmt lint test check mostlyclean clean
all:
	cargo build -p lode-port
fmt:
	cargo fmt -p lode-port --check
lint:
	cargo clippy -p lode-port --all-targets -- -D warnings
test:
	cargo test -p lode-port
check:
	$(MAKE) fmt
	$(MAKE) lint
	$(MAKE) test
mostlyclean:
	rm -rf ../target/debug/incremental/lode_port-*
clean:
	cargo clean -p lode-port

.PHONY: test-equivalence
test-equivalence:
	cargo test -p lode-port --test equivalence

.PHONY: test-timing
test-timing:
	cargo test -p lode-port --test event_timing --test devices --test equivalence
