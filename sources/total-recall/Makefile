.DEFAULT_GOAL := capture
SOURCE ?= $(HOME)/LodeRunner_TotalRecall
LEVEL ?= 12
OUTPUT ?= ../../target/total-recall/$(LEVEL).json
MODE ?= ticks
KEY_ROUNDS ?=
INDEX ?=

.PHONY: capture compare check mostlyclean clean
capture:
	./capture.py --source "$(SOURCE)" --level "$(LEVEL)" --output "$(OUTPUT)" $(if $(INDEX),--demo-index "$(INDEX)",)
compare:
	$(MAKE) -C ../.. recall-compare CAPTURE="target/total-recall/$(LEVEL).json" RECORDING="target/total-recall/$(LEVEL)-$(MODE).json" RECALL_ARGS="$(foreach timing,$(KEY_ROUNDS),--key-round $(timing))"
check:
	uv run ty check capture.py
	node --check observer.js
	$(MAKE) -C ../.. recall-check

# Captures are diagnostic evidence and remain available across cleanup.
mostlyclean:
	rm -rf __pycache__
clean: mostlyclean
