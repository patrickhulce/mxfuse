.PHONY: all build lint typecheck test \
        build-rust build-python build-node \
        lint-rust lint-python lint-node \
        typecheck-python typecheck-node \
        test-rust test-python test-node

all: build lint typecheck test

build: build-rust build-python build-node

build-rust:
	cargo build --workspace --exclude mxfuse_python_bindings

build-python:
	cd src/python-mxfuse && uv sync && uv run maturin develop --uv

build-node:
	pnpm --dir src/node-mxfuse build

lint: lint-rust lint-python lint-node

lint-rust:
	cargo fmt --all -- --check
	cargo clippy -p mxfuse -p mxfuse_node_bindings -- -D warnings

lint-python:
	cd src/python-mxfuse && uv run ruff check .
	cd src/python-mxfuse && uv run ruff format --check .

lint-node:
	pnpm --dir src/node-mxfuse lint

typecheck: typecheck-python typecheck-node

typecheck-python:
	cd src/python-mxfuse && uv run mypy mxfuse tests

typecheck-node:
	pnpm --dir src/node-mxfuse typecheck

test: test-rust test-python test-node

test-rust:
	cargo test -p mxfuse

test-python: build-python
	cd src/python-mxfuse && uv run pytest

test-node: build-node
	pnpm --dir src/node-mxfuse test
