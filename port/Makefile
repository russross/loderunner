.PHONY: all fmt lint test check mostlyclean clean
all:
	cargo build -p lode-port
fmt:
	cargo fmt -p lode-port --check
lint:
	cargo clippy -p lode-port --all-targets -- -D warnings
test:
	cargo test -p lode-port

.PHONY: test-validation
test-validation:
	cargo test -p lode-port --test cli live_validation_drains -- --include-ignored --test-threads=1
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

.PHONY: test-collision
test-collision:
	cargo test -p lode-port --test collision -- --nocapture

.PHONY: test-render-sdl
test-render-sdl:
	SDL_VIDEODRIVER=dummy cargo test -p lode-port --test render sdl_partial_texture_updates_match_the_rgb_cache -- --include-ignored --test-threads=1
