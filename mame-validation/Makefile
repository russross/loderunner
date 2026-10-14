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
