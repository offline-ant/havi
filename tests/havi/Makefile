SHELL := /bin/bash

TESTS := $(sort $(wildcard *-test.sh))
LOG_DIR := $(shell mktemp -d /tmp/havi-tests.XXXXXX)
PASS_DIR := $(LOG_DIR)/.pass
FAIL_DIR := $(LOG_DIR)/.fail

# Pre-warm uv cache before parallel test runs
HAVI_ROOT := $(abspath ../..)
_ := $(shell $(HAVI_ROOT)/havi-webview-remote-cli --help >/dev/null 2>&1 || true)

.PHONY: test $(TESTS) summary

test: $(TESTS)
	@echo ""
	@echo "--- Results (logs: $(LOG_DIR)) ---"
	@pass=$$(ls $(PASS_DIR) 2>/dev/null | wc -l); \
	 fail=$$(ls $(FAIL_DIR) 2>/dev/null | wc -l); \
	 for f in $(PASS_DIR)/*; do [ -e "$$f" ] && echo "  PASS  $$(basename $$f)" || true; done; \
	 for f in $(FAIL_DIR)/*; do [ -e "$$f" ] && echo "  FAIL  $$(basename $$f)  (log: $(LOG_DIR)/$$(basename $$f).log)" || true; done; \
	 echo ""; \
	 if [ "$$fail" -gt 0 ]; then echo "$$fail/$$((pass+fail)) tests FAILED"; exit 1; \
	 else echo "All $$pass tests passed"; fi

$(TESTS): | $(PASS_DIR) $(FAIL_DIR)
	@echo "  START $@"
	@if ./$@ > $(LOG_DIR)/$@.log 2>&1; then \
		echo "  PASS  $@"; \
		touch $(PASS_DIR)/$@; \
	else \
		echo "  FAIL  $@  (log: $(LOG_DIR)/$@.log)"; \
		touch $(FAIL_DIR)/$@; \
	fi

$(PASS_DIR) $(FAIL_DIR):
	@mkdir -p $@
