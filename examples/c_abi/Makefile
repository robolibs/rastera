SHELL := /bin/bash

ROOT := ../..
CARGO ?= cargo
C_DEMO_BIN := $(ROOT)/target/debug/rastera_c_api_demo

.PHONY: all build run clean

all: run

build:
	@cd $(ROOT) && $(CARGO) build
	@cc -I$(ROOT)/include demo.c -L$(ROOT)/target/debug -lrastera -Wl,-rpath,$$(cd $(ROOT) && pwd)/target/debug -lm -o $(C_DEMO_BIN)

run: build
	@$(C_DEMO_BIN)

clean:
	@rm -f $(C_DEMO_BIN)
