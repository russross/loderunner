.PHONY: all fmt lint test check mame-poc mame-validation mostlyclean clean
SYMBOLS = sources/analysis/apple2-symbols.json
$(SYMBOLS): sources/analysis/generate_symbols.py sources/lode_runner_reveng/main.bin $(wildcard sources/lode_runner_reveng/*.asm)
	./sources/analysis/generate_symbols.py $@
.DEFAULT_GOAL := all
all: $(SYMBOLS)
	cargo build --workspace
fmt:
	cargo fmt --all --check
lint: $(SYMBOLS)
	cargo clippy --workspace --all-targets -- -D warnings
test: $(SYMBOLS)
	cargo test --workspace
check:
	$(MAKE) fmt
	$(MAKE) lint
	$(MAKE) test
mame-poc:
	$(MAKE) -C sources/mame-poc check
mame-validation:
	$(MAKE) -C mame-validation check
mostlyclean:
	rm -rf target/debug/incremental
clean:
	cargo clean

