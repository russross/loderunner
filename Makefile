.DEFAULT_GOAL := all
.NOTPARALLEL:
.DELETE_ON_ERROR:
CARGO ?= cargo
ARGS ?=
MAME ?= mame
ROMPATH ?=
TEST_ARGS ?=
SYMBOLS = sources/analysis/apple2-symbols.json
MANIFESTS = Cargo.toml src/trace/Cargo.toml src/mame/Cargo.toml ../crt/Cargo.toml
.PHONY: all release run format fmt lint test check validate mame-validation mame-poc test-render-sdl mostlyclean clean

$(SYMBOLS): sources/analysis/generate_symbols.py sources/lode_runner_reveng/main.bin $(wildcard sources/lode_runner_reveng/*.asm)
	./sources/analysis/generate_symbols.py $@

Cargo.lock: $(MANIFESTS)
	$(CARGO) update --workspace
	touch $@

all release: $(SYMBOLS) Cargo.lock
	$(CARGO) build --locked --release -p loderunner
run: all
	target/release/loderunner $(ARGS)
format:
	$(CARGO) fmt --all
fmt:
	$(CARGO) fmt --all --check
lint: fmt $(SYMBOLS) Cargo.lock
	$(CARGO) clippy --locked --workspace --all-targets -- -D warnings
test: $(SYMBOLS) Cargo.lock
	$(CARGO) test --locked --workspace $(TEST_ARGS)
check: lint test
validate: all
	@test -n "$(TRACE)" || { echo 'Usage: make validate TRACE=play.trace' >&2; exit 1; }
	target/release/loderunner --verify "$(TRACE)" $(ARGS)
mame-validation: $(SYMBOLS) Cargo.lock
	MAME="$(MAME)" ROMPATH="$(ROMPATH)" $(CARGO) test --locked -p loderunner --test mame -- --include-ignored --test-threads=1
	MAME="$(MAME)" ROMPATH="$(ROMPATH)" $(CARGO) test --locked -p loderunner --test cli machine_record_trace -- --include-ignored --test-threads=1
mame-poc:
	$(MAKE) -C sources/mame-poc check
test-render-sdl:
	SDL_VIDEODRIVER=dummy $(CARGO) test --locked -p loderunner --test render --test presentation sdl_ -- --include-ignored --test-threads=1

.PHONY: test-presentation
test-presentation: $(SYMBOLS) Cargo.lock
	$(CARGO) test --locked -p loderunner --bin loderunner --test cli --test render --test presentation

# Preserve the deployed release executable and all user recordings and scores.
mostlyclean:
	rm -rf target/debug target/release/deps target/release/build target/release/incremental target/release/.fingerprint target/release/examples
clean: mostlyclean
	$(CARGO) clean

.PHONY: timing-fit timing-types
TIMING_MEASUREMENTS ?= artifacts/event-timing-final/measurements.json
timing-fit:
	./sources/analysis/fit_port_model.py $(TIMING_MEASUREMENTS) --model src/model.json --report sources/analysis/port-model-report.json
timing-types:
	uv run --project sources/mame-poc ty check --extra-search-path sources/analysis sources/analysis/fit_port_model.py
