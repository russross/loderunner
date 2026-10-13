.PHONY: all types check probe mostlyclean clean
MAME ?= /usr/games/mame
WORK ?= $(abspath ../../artifacts/mame-poc)
ROMPATH ?=
all: check
types:
	uv run ty check run.py
check:
	$(MAKE) types
	./run.py --mame "$(MAME)" --work "$(WORK)" $(if $(ROMPATH),--rompath "$(ROMPATH)",)
probe:
	./run.py --mame "$(MAME)" --work "$(WORK)" $(if $(ROMPATH),--rompath "$(ROMPATH)",)
mostlyclean:
	rm -rf __pycache__
clean: mostlyclean
	rm -rf .venv
	rm -f "$(WORK)/level-1.bin"
