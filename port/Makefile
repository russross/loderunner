.PHONY: all format fmt lint test check mostlyclean clean
TEST_ARGS ?=
all:
	cargo build -p lode-port
format:
	cargo fmt -p lode-port
fmt:
	cargo fmt -p lode-port --check
lint:
	cargo clippy -p lode-port --all-targets -- -D warnings
test:
	cargo test -p lode-port $(TEST_ARGS)

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

.PHONY: timing-fit timing-types
TIMING_MEASUREMENTS ?= ../artifacts/event-timing-final/measurements.json
timing-fit:
	../sources/analysis/fit_port_model.py $(TIMING_MEASUREMENTS) --model src/model.json --report ../sources/analysis/port-model-report.json
timing-types:
	uv run --project ../sources/mame-poc ty check --extra-search-path ../sources/analysis ../sources/analysis/fit_port_model.py
