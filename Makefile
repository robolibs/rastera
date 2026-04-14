SHELL := /bin/bash

PROJECT_NAME := $(shell sed -n '/^[[:space:]]*[^#\[[:space:]]/p' PROJECT | head -1 | tr -d '[:space:]')
PROJECT_VERSION := $(shell sed -n '/^[[:space:]]*[^#\[[:space:]]/p' PROJECT | sed -n '2p' | tr -d '[:space:]')
ifeq ($(PROJECT_NAME),)
    $(error Error: PROJECT file not found or invalid)
endif

TOP_DIR := $(CURDIR)
CARGO ?= cargo
EXAMPLE ?= main

$(info ------------------------------------------)
$(info Project: $(PROJECT_NAME) v$(PROJECT_VERSION))
$(info ------------------------------------------)

.PHONY: build b compile c run r test t check fmt clean help h test-python c-demo python-basic python-rgba

build:
	@$(CARGO) build --lib --examples

b: build

compile:
	@$(CARGO) clean
	@$(MAKE) build

c: compile

run:
	@$(CARGO) run --example $(EXAMPLE)

r: run

test:
	@$(CARGO) test --all-targets

t: test

test-python:
	@$(CARGO) check --features python

c-demo:
	@$(MAKE) -C examples/c_abi run CARGO="$(CARGO)"

python-basic:
	@$(MAKE) -C examples/python_binding basic CARGO="$(CARGO)"

python-rgba:
	@$(MAKE) -C examples/python_binding rgba CARGO="$(CARGO)"

check:
	@$(CARGO) check --all-targets

fmt:
	@$(CARGO) fmt --all

clean:
	@$(CARGO) clean
	@$(MAKE) -C examples/python_binding clean 2>/dev/null || true

help:
	@echo
	@echo "Usage: make [target]"
	@echo
	@echo "Available targets:"
	@echo "  build          Build library and Rust examples"
	@echo "  compile        Clean and rebuild"
	@echo "  run            Run a Rust example (EXAMPLE=main by default)"
	@echo "  test           Run all tests"
	@echo "  test-python    Type-check with the python feature enabled"
	@echo "  c-demo         Build and run the C ABI demo (examples/c_abi/demo.c)"
	@echo "  python-basic   Build wheel via maturin and run examples/python_binding/basic.py"
	@echo "  python-rgba    Build wheel via maturin and run examples/python_binding/rgba.py"
	@echo "  check          Run cargo check on all targets"
	@echo "  fmt            Format the workspace"
	@echo "  clean          Remove Cargo build artifacts + Python venv"
	@echo
	@echo "Examples:"
	@echo "  make run"
	@echo "  make run EXAMPLE=rgba_image"
	@echo "  make c-demo"
	@echo "  make python-basic"
	@echo

h: help
