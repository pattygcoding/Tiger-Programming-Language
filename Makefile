CARGO ?= cargo
EXE :=
ifeq ($(OS),Windows_NT)
EXE := .exe
endif

.PHONY: build run test benchmarks fmt clippy wasm wasm-check startup web wasi timing compare

build:
	$(CARGO) build --release
	@echo "Built target/release/tiger$(EXE)"

run: build
	./target/release/tiger$(EXE) run examples/demo.tg

test:
	$(CARGO) test

benchmarks:
	$(CARGO) test -p tiger-conformance --test benchmarks -- --nocapture

# Times every benchmark program in process, with no process startup in the
# numbers. TIGER_ENGINE=tree runs the same programs on the tree-walker.
timing:
	$(CARGO) test -p tiger-conformance --test timing --release -- --ignored --nocapture

# Times a paired Tiger/Python workload on identical work and reports the ratio.
compare:
	$(CARGO) build --release
	python benchmarks/python/compare.py

fmt:
	$(CARGO) fmt --check

clippy:
	$(CARGO) clippy --all-targets

wasm:
	$(CARGO) run -p tiger-web -- -build-only

wasm-check:
	$(CARGO) build -p tiger-wasm --release --target wasm32-unknown-unknown
	$(CARGO) test -p tiger-wasm

# Boots the served playground over HTTP and runs a program on it.
startup:
	$(CARGO) build -p tiger-web --release
	node crates/tiger-web/tests/startup.mjs

web:
	$(CARGO) run -p tiger-web

wasi:
	$(CARGO) build -p tiger-wasi --release --target wasm32-wasip1
