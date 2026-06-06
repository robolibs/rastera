SHELL := /bin/bash

ROOT := ../..
PYTHON ?= python3
CARGO ?= cargo
VENV := .venv
VENV_PYTHON := $(CURDIR)/$(VENV)/bin/python

.PHONY: all develop basic rgba clean

all: basic

$(VENV_PYTHON):
	@$(PYTHON) -m venv $(VENV)
	@$(VENV_PYTHON) -m pip install --quiet maturin

develop: $(VENV_PYTHON)
	@cd $(ROOT) && VIRTUAL_ENV=$(CURDIR)/$(VENV) PATH=$(CURDIR)/$(VENV)/bin:$$PATH PYO3_PYTHON=$(VENV_PYTHON) CARGO=$(CARGO) maturin develop --features python

basic: develop
	@$(VENV_PYTHON) basic.py

rgba: develop
	@$(VENV_PYTHON) rgba.py

clean:
	@rm -rf $(VENV)
