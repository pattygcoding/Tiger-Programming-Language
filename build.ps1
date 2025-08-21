# Tiger Programming Language Build Script for PowerShell
# This script provides all the functionality of the Makefile for Windows/PowerShell users

param(
    [string]$Target = "help"
)

function Write-Status {
    param([string]$Message)
    Write-Host "✅ $Message" -ForegroundColor Green
}

function Write-Info {
    param([string]$Message)
    Write-Host "$Message" -ForegroundColor Cyan
}

function Write-Error {
    param([string]$Message)
    Write-Host "❌ $Message" -ForegroundColor Red
}

function Test-GoInstalled {
    try {
        $null = Get-Command go -ErrorAction Stop
        return $true
    }
    catch {
        Write-Error "Go is not installed or not in PATH. Please install Go from https://golang.org/dl/"
        return $false
    }
}

function Build-CLI {
    Write-Info "Building Tiger CLI..."
    if (Test-GoInstalled) {
        go build -o tiger-cli.exe ./go
        if ($LASTEXITCODE -eq 0) {
            Write-Status "CLI built successfully: ./tiger-cli.exe"
        } else {
            Write-Error "Failed to build CLI"
            exit 1
        }
    }
}

function Build-WASM {
    Write-Info "Building Tiger WASM..."
    if (Test-GoInstalled) {
        $env:GOOS = "js"
        $env:GOARCH = "wasm"
        go build -o main.wasm ./go
        if ($LASTEXITCODE -eq 0) {
            Write-Status "WASM built successfully: ./main.wasm"
        } else {
            Write-Error "Failed to build WASM"
            exit 1
        }
        # Reset environment variables
        Remove-Item Env:GOOS -ErrorAction SilentlyContinue
        Remove-Item Env:GOARCH -ErrorAction SilentlyContinue
    }
}

function Copy-WasmExec {
    if (-not (Test-Path "wasm_exec.js")) {
        Write-Info "Copying wasm_exec.js..."
        $goRoot = go env GOROOT
        $wasmExecPath = Join-Path $goRoot "lib\wasm\wasm_exec.js"
        if (-not (Test-Path $wasmExecPath)) {
            # Try old location for older Go versions
            $wasmExecPath = Join-Path $goRoot "misc\wasm\wasm_exec.js"
        }
        if (Test-Path $wasmExecPath) {
            Copy-Item $wasmExecPath . -Force
            Write-Status "wasm_exec.js copied"
        } else {
            Write-Error "Could not find wasm_exec.js in Go installation"
            Write-Info "Please check your Go installation or manually copy wasm_exec.js"
            Write-Info "Expected locations:"
            Write-Info "  - $goRoot\lib\wasm\wasm_exec.js (Go 1.12+)"
            Write-Info "  - $goRoot\misc\wasm\wasm_exec.js (older Go versions)"
        }
    }
}

function Run-Tests {
    Write-Info "Running tests..."
    if (Test-GoInstalled) {
        go test ./go/...
        if ($LASTEXITCODE -eq 0) {
            Write-Status "Tests passed"
        } else {
            Write-Error "Tests failed"
            exit 1
        }
    }
}

function Clean-Build {
    Write-Info "Cleaning build artifacts..."
    @("tiger-cli.exe", "tiger-cli", "main.wasm", "test.tg", "simple.tg") | ForEach-Object {
        if (Test-Path $_) {
            Remove-Item $_ -Force
        }
    }
    Write-Status "Cleaned"
}

function Install-Dependencies {
    Write-Info "Installing dependencies..."
    if (Test-GoInstalled) {
        go mod tidy
        if ($LASTEXITCODE -eq 0) {
            Write-Status "Dependencies installed"
        } else {
            Write-Error "Failed to install dependencies"
            exit 1
        }
    }
}

function Start-Server {
    Write-Info "Starting development server on http://localhost:8000"
    Write-Info "Open tiger_go.html to test WASM version"
    Write-Info "Press Ctrl+C to stop the server"
    
    # Check if Python is available
    $pythonCmd = $null
    foreach ($cmd in @("python", "python3", "py")) {
        try {
            $null = Get-Command $cmd -ErrorAction Stop
            $pythonCmd = $cmd
            break
        }
        catch { }
    }
    
    if ($pythonCmd) {
        & $pythonCmd -m http.server 8000
    } else {
        Write-Error "Python is not installed or not in PATH"
        Write-Info "You can manually serve the files using any web server"
        Write-Info "Alternatives:"
        Write-Info "  - Install Python from https://python.org"
        Write-Info "  - Use 'npx serve .' if you have Node.js"
        Write-Info "  - Use any other static file server"
    }
}

function Build-All-Platforms {
    Write-Info "Building for multiple platforms..."
    if (Test-GoInstalled) {
        # Windows
        $env:GOOS = "windows"; $env:GOARCH = "amd64"
        go build -o tiger-cli-windows.exe ./go
        
        # macOS
        $env:GOOS = "darwin"; $env:GOARCH = "amd64"
        go build -o tiger-cli-darwin ./go
        
        # Linux
        $env:GOOS = "linux"; $env:GOARCH = "amd64"
        go build -o tiger-cli-linux ./go
        
        # Reset environment variables
        Remove-Item Env:GOOS -ErrorAction SilentlyContinue
        Remove-Item Env:GOARCH -ErrorAction SilentlyContinue
        
        if ($LASTEXITCODE -eq 0) {
            Write-Status "Built for all platforms"
        } else {
            Write-Error "Failed to build for all platforms"
            exit 1
        }
    }
}

function Show-Help {
    Write-Host ""
    Write-Host "🐯 Tiger Programming Language Build System (PowerShell)" -ForegroundColor Yellow
    Write-Host ""
    Write-Host "Usage: .\build.ps1 <target>" -ForegroundColor White
    Write-Host ""
    Write-Host "Targets:" -ForegroundColor White
    Write-Host "  all        - Build both CLI and WASM versions (default)" -ForegroundColor Gray
    Write-Host "  cli        - Build CLI binary only" -ForegroundColor Gray
    Write-Host "  wasm       - Build WebAssembly version only" -ForegroundColor Gray
    Write-Host "  build      - Build everything including wasm_exec.js" -ForegroundColor Gray
    Write-Host "  test       - Run tests" -ForegroundColor Gray
    Write-Host "  clean      - Clean build artifacts" -ForegroundColor Gray
    Write-Host "  deps       - Install dependencies" -ForegroundColor Gray
    Write-Host "  serve      - Start development server" -ForegroundColor Gray
    Write-Host "  build-all  - Build for multiple platforms" -ForegroundColor Gray
    Write-Host "  help       - Show this help message" -ForegroundColor Gray
    Write-Host ""
    Write-Host "Examples:" -ForegroundColor White
    Write-Host "  .\build.ps1 cli              # Build CLI only" -ForegroundColor Gray
    Write-Host "  .\build.ps1 wasm             # Build WASM only" -ForegroundColor Gray
    Write-Host "  .\build.ps1 serve            # Start development server" -ForegroundColor Gray
    Write-Host "  .\tiger-cli.exe run file.tg # Run a Tiger file" -ForegroundColor Gray
    Write-Host "  .\tiger-cli.exe repl         # Start interactive REPL" -ForegroundColor Gray
    Write-Host ""
}

# Main execution logic
switch ($Target.ToLower()) {
    "all" { 
        Build-CLI
        Build-WASM
    }
    "cli" { Build-CLI }
    "wasm" { Build-WASM }
    "build" { 
        Copy-WasmExec
        Build-CLI
        Build-WASM
    }
    "test" { Run-Tests }
    "clean" { Clean-Build }
    "deps" { Install-Dependencies }
    "serve" { 
        Copy-WasmExec
        Build-CLI
        Build-WASM
        Start-Server
    }
    "build-all" { 
        Build-CLI
        Build-WASM
        Build-All-Platforms
    }
    "help" { Show-Help }
    default { 
        Write-Error "Unknown target: $Target"
        Show-Help 
        exit 1
    }
}