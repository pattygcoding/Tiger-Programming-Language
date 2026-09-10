GO ?= go
EXE :=
ifeq ($(OS),Windows_NT)
EXE := .exe
endif

.PHONY: build run test benchmarks wasm web wasi

build:
	$(GO) build -o bin/tiger$(EXE) ./cmd/tiger

run: build
	./bin/tiger$(EXE) run examples/demo.tg

test:
	$(GO) test ./...

benchmarks:
	$(GO) test ./benchmarks -count=1 -v

wasm:
	$(GO) run ./cmd/web -build-only

web:
	$(GO) run ./cmd/web

wasi:
	GOOS=wasip1 GOARCH=wasm $(GO) build -o bin/tiger-wasi.wasm ./cmd/wasi