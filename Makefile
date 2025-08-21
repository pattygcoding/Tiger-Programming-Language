# Tiger Programming Language Makefile
.PHONY: all cli wasm clean install test help

# Default target
all: cli wasm

# Build CLI binary
cli:
	@echo "Building Tiger CLI..."
	go build -o tiger-cli ./go
	@echo "✅ CLI built successfully: ./tiger-cli"

# Build WebAssembly version
wasm:
	@echo "Building Tiger WASM..."
	GOOS=js GOARCH=wasm go build -o main.wasm ./go
	@echo "✅ WASM built successfully: ./main.wasm"

# Copy wasm_exec.js if it doesn't exist
wasm_exec.js:
	@if [ ! -f wasm_exec.js ]; then \
		echo "Copying wasm_exec.js..."; \
		cp "$$(go env GOROOT)/lib/wasm/wasm_exec.js" .; \
		echo "✅ wasm_exec.js copied"; \
	fi

# Build everything including wasm_exec.js
build: wasm_exec.js cli wasm

# Run tests
test:
	@echo "Running tests..."
	go test ./go/...
	@echo "✅ Tests passed"

# Clean build artifacts
clean:
	@echo "Cleaning build artifacts..."
	rm -f tiger-cli
	rm -f main.wasm
	rm -f test.tg simple.tg
	@echo "✅ Cleaned"

# Install dependencies
deps:
	@echo "Installing dependencies..."
	go mod tidy
	@echo "✅ Dependencies installed"

# Start local development server
serve: build
	@echo "Starting development server on http://localhost:8000"
	@echo "Open tiger_go.html to test WASM version"
	python3 -m http.server 8000

# Build for multiple platforms
build-all: cli wasm
	@echo "Building for multiple platforms..."
	GOOS=windows GOARCH=amd64 go build -o tiger-cli-windows.exe ./go
	GOOS=darwin GOARCH=amd64 go build -o tiger-cli-darwin ./go
	GOOS=linux GOARCH=amd64 go build -o tiger-cli-linux ./go
	@echo "✅ Built for all platforms"

# Development mode - rebuild on file changes (requires inotify-tools)
dev:
	@echo "Development mode - watching for changes..."
	while inotifywait -e modify -r ./go; do \
		make cli; \
	done

# Help target
help:
	@echo "Tiger Programming Language Build System"
	@echo ""
	@echo "Targets:"
	@echo "  all        - Build both CLI and WASM versions (default)"
	@echo "  cli        - Build CLI binary only"
	@echo "  wasm       - Build WebAssembly version only"
	@echo "  build      - Build everything including wasm_exec.js"
	@echo "  test       - Run tests"
	@echo "  clean      - Clean build artifacts"
	@echo "  deps       - Install dependencies"
	@echo "  serve      - Start development server"
	@echo "  build-all  - Build for multiple platforms"
	@echo "  help       - Show this help message"
	@echo ""
	@echo "Usage examples:"
	@echo "  make cli             # Build CLI only"
	@echo "  make wasm            # Build WASM only"
	@echo "  make serve           # Start development server"
	@echo "  ./tiger-cli run file.tg   # Run a Tiger file"
	@echo "  ./tiger-cli repl     # Start interactive REPL"