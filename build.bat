@echo off
REM Tiger Programming Language Build Script for Windows Command Prompt
REM This provides basic build functionality for Windows users

if "%1"=="" goto help
if "%1"=="help" goto help
if "%1"=="cli" goto cli
if "%1"=="wasm" goto wasm
if "%1"=="build" goto build
if "%1"=="clean" goto clean
if "%1"=="test" goto test
if "%1"=="serve" goto serve
if "%1"=="all" goto all

echo Unknown command: %1
goto help

:cli
echo Building Tiger CLI...
go build -o tiger-cli.exe ./go
if %errorlevel% equ 0 (
    echo ✅ CLI built successfully: tiger-cli.exe
) else (
    echo ❌ Failed to build CLI
    exit /b 1
)
goto end

:wasm
echo Building Tiger WASM...
set GOOS=js
set GOARCH=wasm
go build -o main.wasm ./go
set GOOS=
set GOARCH=
if %errorlevel% equ 0 (
    echo ✅ WASM built successfully: main.wasm
) else (
    echo ❌ Failed to build WASM
    exit /b 1
)
goto end

:build
if not exist wasm_exec.js (
    echo Copying wasm_exec.js...
    for /f %%i in ('go env GOROOT') do copy "%%i\lib\wasm\wasm_exec.js" . 2>nul || copy "%%i\misc\wasm\wasm_exec.js" .
    echo ✅ wasm_exec.js copied
)
call :cli
call :wasm
goto end

:all
call :cli
call :wasm
goto end

:clean
echo Cleaning build artifacts...
if exist tiger-cli.exe del tiger-cli.exe
if exist tiger-cli del tiger-cli
if exist main.wasm del main.wasm
if exist test.tg del test.tg
if exist simple.tg del simple.tg
echo ✅ Cleaned
goto end

:test
echo Running tests...
go test ./go/...
if %errorlevel% equ 0 (
    echo ✅ Tests passed
) else (
    echo ❌ Tests failed
    exit /b 1
)
goto end

:serve
call :build
echo Starting development server on http://localhost:8000
echo Open tiger_go.html to test WASM version
echo Press Ctrl+C to stop the server
python -m http.server 8000 2>nul || python3 -m http.server 8000 2>nul || py -m http.server 8000 2>nul || (
    echo ❌ Python not found. Please install Python or use another web server.
    echo You can manually serve files using any static file server.
)
goto end

:help
echo.
echo 🐯 Tiger Programming Language Build System (Windows)
echo.
echo Usage: build.bat ^<command^>
echo.
echo Commands:
echo   all      - Build both CLI and WASM versions
echo   cli      - Build CLI binary only
echo   wasm     - Build WebAssembly version only
echo   build    - Build everything including wasm_exec.js
echo   test     - Run tests
echo   clean    - Clean build artifacts
echo   serve    - Start development server
echo   help     - Show this help message
echo.
echo Examples:
echo   build.bat cli              # Build CLI only
echo   build.bat wasm             # Build WASM only
echo   build.bat serve            # Start development server
echo   tiger-cli.exe run file.tg # Run a Tiger file
echo   tiger-cli.exe repl         # Start interactive REPL
echo.

:end