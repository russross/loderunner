.PHONY: all check test mostlyclean clean
TEST_FILTER ?=
all:
	$(MAKE) -C .. all
check:
	$(MAKE) -C .. fmt
	$(MAKE) -C .. lint
	$(MAKE) -C ../sources/mame-poc symbols-types
	$(MAKE) test
test:
	cargo test -p lode-mame-validation $(TEST_FILTER) -- --include-ignored --test-threads=1
mostlyclean:
	@:
clean:
	cargo clean -p lode-mame-validation

.PHONY: timing-measure
TIMING_WORK ?= ../artifacts/event-timing-final
# Measurements preserve both traces and independent machine observations.
timing-measure:
	cargo run --example timing -p lode-mame-validation -- $(TIMING_WORK)

.PHONY: timing-fit
timing-fit:
	../sources/analysis/fit_event_model.py $(TIMING_WORK)

.PHONY: timing-refresh
timing-refresh:
	cargo run --example timing -p lode-mame-validation -- $(TIMING_WORK) --refresh-native

.PHONY: timing-verify timing-types
timing-verify:
	cargo run --example timing -p lode-mame-validation -- $(TIMING_WORK) --verify-model
timing-types:
	uv run --project ../sources/mame-poc ty check ../sources/analysis/fit_event_model.py

.PHONY: timing-port-test
timing-port-test:
	cargo test -p lode-original --test event_timing --test cli
