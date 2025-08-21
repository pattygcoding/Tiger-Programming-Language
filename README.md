# 🐯 Tiger Programming Language

A modern, Python-like programming language with C-style syntax, built in Go and running both as a CLI tool and in the browser via WebAssembly.

## 🚀 Features

- **Variables & Constants**: `let x = 10;` and `const PI = 3.14;`
- **Functions**: `func name(params) { ... }`
- **Classes/Objects**: Basic OOP support
- **Control Flow**: `if/else`, `while`, `for` loops with `{}` blocks
- **Comments**: `//` single-line and `/* */` multi-line
- **WASM Ready**: Runs in browsers via WebAssembly
- **CLI Tool**: Downloadable binary for local development

## 🔧 Prerequisites

### All Platforms
- **Go 1.18+**: Download from [golang.org](https://golang.org/dl/)
- **Git**: For cloning the repository

### Additional Windows Requirements
- **PowerShell 5.1+** or **PowerShell Core 7+** (recommended)
- **Python 3.x** (optional, for development server): Download from [python.org](https://python.org/downloads/)

### Additional Unix/Linux/macOS Requirements  
- **Make**: Usually pre-installed or available via package manager
- **Python 3.x** (for development server): Usually pre-installed or `apt install python3` / `brew install python3`

## ⚠️ Windows Setup Notes

If you're on Windows and having trouble:

1. **PowerShell Execution Policy**: You may need to enable script execution:
   ```powershell
   Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope CurrentUser
   ```

2. **Go Environment**: Ensure Go is in your PATH by running `go version`

3. **WASM Files**: The scripts automatically handle copying `wasm_exec.js` from your Go installation

4. **Web Server**: If Python isn't available, you can use any static file server like:
   - `npx serve .` (if you have Node.js)
   - Any other HTTP server of your choice

## 📦 Quick Start

### Option 1: Build from Source (Unix/Linux/macOS)

```bash
# Clone the repository
git clone https://github.com/pattygcoding/Tiger-Programming-Language.git
cd Tiger-Programming-Language

# Build everything
make build

# Run a Tiger file
./tiger-cli run myfile.tg

# Start interactive REPL
./tiger-cli repl
```

### Option 2: Build from Source (Windows PowerShell)

```powershell
# Clone the repository
git clone https://github.com/pattygcoding/Tiger-Programming-Language.git
cd Tiger-Programming-Language

# Build everything
.\build.ps1 build

# Run a Tiger file
.\tiger-cli.exe run myfile.tg

# Start interactive REPL
.\tiger-cli.exe repl
```

### Option 3: Build from Source (Windows Command Prompt)

```cmd
# Clone the repository
git clone https://github.com/pattygcoding/Tiger-Programming-Language.git
cd Tiger-Programming-Language

# Build everything
build.bat build

# Run a Tiger file
tiger-cli.exe run myfile.tg

# Start interactive REPL
tiger-cli.exe repl
```

### Option 4: WebAssembly in Browser

**Unix/Linux/macOS:**
1. Build the WASM version: `make wasm`
2. Start a web server: `make serve`
3. Open `tiger_go.html` in your browser

**Windows PowerShell:**
1. Build the WASM version: `.\build.ps1 wasm`
2. Start a web server: `.\build.ps1 serve`
3. Open `tiger_go.html` in your browser

**Windows Command Prompt:**
1. Build the WASM version: `build.bat wasm`
2. Start a web server: `build.bat serve`
3. Open `tiger_go.html` in your browser

## 🛠️ Build Commands

### Unix/Linux/macOS (Make)

| Command | Description |
|---------|-------------|
| `make all` | Build CLI and WASM versions |
| `make cli` | Build CLI binary only |
| `make wasm` | Build WebAssembly version only |
| `make build` | Build everything including wasm_exec.js |
| `make serve` | Start development server |
| `make clean` | Clean build artifacts |
| `make test` | Run tests |
| `make build-all` | Build for all platforms |

### Windows PowerShell

| Command | Description |
|---------|-------------|
| `.\build.ps1 all` | Build CLI and WASM versions |
| `.\build.ps1 cli` | Build CLI binary only |
| `.\build.ps1 wasm` | Build WebAssembly version only |
| `.\build.ps1 build` | Build everything including wasm_exec.js |
| `.\build.ps1 serve` | Start development server |
| `.\build.ps1 clean` | Clean build artifacts |
| `.\build.ps1 test` | Run tests |
| `.\build.ps1 build-all` | Build for all platforms |

### Windows Command Prompt

| Command | Description |
|---------|-------------|
| `build.bat all` | Build CLI and WASM versions |
| `build.bat cli` | Build CLI binary only |
| `build.bat wasm` | Build WebAssembly version only |
| `build.bat build` | Build everything including wasm_exec.js |
| `build.bat serve` | Start development server |
| `build.bat clean` | Clean build artifacts |
| `build.bat test` | Run tests |

## 📝 Language Syntax

### Variables and Constants

```tiger
// Variables (mutable)
let name = "Tiger";
let age = 5;
let height = 1.85;
let active = true;

// Constants (immutable)
const PI = 3.14159;
const GREETING = "Hello World";
```

### Functions

```tiger
// Function definition
func greet(name) {
    print "Hello";
    print name;
}

// Function with multiple parameters
func add(a, b) {
    let result = a + b;
    print result;
    return result;
}

// Function calls
greet("Tiger");
add(5, 3);
```

### Control Flow

```tiger
// If-else statements (braces required)
if x > 10 {
    print "x is greater than 10";
} else {
    print "x is 10 or less";
}

// While loops
let counter = 0;
while counter < 5 {
    print counter;
    counter = counter + 1;
}

// For loops
for (let i = 0; i < 3; i = i + 1) {
    print "Iteration";
    print i;
}
```

### Classes (Basic OOP)

```tiger
// Class definition
class Person {
    func greet(name) {
        print "Hello";
        print name;
    }
    
    func age() {
        return 25;
    }
}

// Class usage (basic implementation)
let person = Person;
```

### Data Types

| Type | Example | Description |
|------|---------|-------------|
| String | `"Hello"` | Text values |
| Integer | `42` | Whole numbers |
| Float | `3.14` | Decimal numbers |
| Boolean | `true`, `false` | Boolean values |

### Comments

```tiger
// Single-line comment

/*
Multi-line comment
Can span multiple lines
*/

let x = 10; // Inline comment
```

## 🎮 Interactive Examples

### Example 1: Basic Variables
```tiger
const GREETING = "Welcome to Tiger!";
let user = "Developer";
let version = 1.0;

print GREETING;
print "User:";
print user;
print "Version:";
print version;
```

### Example 2: Functions and Logic
```tiger
func calculateArea(radius) {
    const PI = 3.14159;
    let area = PI * radius * radius;
    print "Area:";
    print area;
    return area;
}

let r = 5;
if r > 0 {
    calculateArea(r);
} else {
    print "Invalid radius";
}
```

### Example 3: Loops and Counters
```tiger
print "Counting down:";
let count = 5;
while count > 0 {
    print count;
    count = count - 1;
}
print "Blast off!";
```

## 🌐 WebAssembly Integration

### Testing WASM Locally

The Tiger language compiles to WebAssembly and can run directly in browsers. The main WebAssembly entry point is `go/main_wasm.go` (not a JavaScript file).

**Quick WASM Test:**
1. Build: `make build` (Unix) or `.\build.ps1 build` (Windows)
2. Serve: `make serve` (Unix) or `.\build.ps1 serve` (Windows)  
3. Open: http://localhost:8000/tiger_go.html

### Embedding in Your Website

1. Include the WASM files in your web directory:
   ```
   main.wasm          # Compiled Tiger interpreter
   wasm_exec.js       # Go's WebAssembly support library
   ```

2. Add Tiger to your HTML:
   ```html
   <script src="wasm_exec.js"></script>
   <script>
       const go = new Go();
       WebAssembly.instantiateStreaming(fetch("main.wasm"), go.importObject)
           .then((result) => {
               go.run(result.instance);
           })
           .catch((error) => {
               console.error("Failed to load Tiger WASM:", error);
           });

       function runTiger(code) {
           return evalTiger(code);
       }
   </script>
   ```

3. Execute Tiger code from JavaScript:
   ```javascript
   // Wait for WASM to load, then:
   const result = evalTiger('let x = 42; print x;');
   console.log(result);
   ```

### WASM Troubleshooting

**Common Issues:**
- **"evalTiger is not defined"**: WASM hasn't finished loading yet
- **"Failed to fetch main.wasm"**: File not found - run build first
- **CORS errors**: Use a proper web server, don't open HTML file directly

**Build Steps:**
1. **Unix/Linux/macOS**: `make build`
2. **Windows PowerShell**: `.\build.ps1 build`  
3. **Windows Command Prompt**: `build.bat build`

This generates:
- `main.wasm` - The compiled Tiger interpreter
- `wasm_exec.js` - Go's WebAssembly runtime (copied from Go installation)

## 🖥️ CLI Usage

### Unix/Linux/macOS
```bash
# Run a Tiger file
./tiger-cli run program.tg

# Start interactive REPL
./tiger-cli repl
```

### Windows
```cmd
# Run a Tiger file
tiger-cli.exe run program.tg

# Start interactive REPL
tiger-cli.exe repl
```

### REPL Commands
```
>>> let x = 10
>>> print x
10
>>> func hello() { print "Hello Tiger!"; }
>>> hello()
Hello Tiger!
>>> exit
```

## 🏗️ Development

### Project Structure
```
/
├── go/              # Go source code
│   ├── main_cli.go  # CLI entry point
│   ├── main_wasm.go # WASM entry point (compiles to main.wasm)
│   ├── lexer/       # Lexical analysis
│   ├── parser/      # Syntax analysis
│   ├── ast/         # Abstract Syntax Tree
│   └── eval/        # Interpreter/evaluator
├── tiger_go.html    # WASM demo page
├── build.ps1        # PowerShell build script
├── build.bat        # Windows batch build script
├── Makefile         # Unix/Linux/macOS build system
└── README.md
```

### Development Workflow

**Unix/Linux/macOS:**
```bash
make deps          # Install dependencies
make test          # Run tests  
make build         # Build everything
make serve         # Start dev server
```

**Windows PowerShell:**
```powershell
.\build.ps1 deps       # Install dependencies
.\build.ps1 test       # Run tests
.\build.ps1 build      # Build everything  
.\build.ps1 serve      # Start dev server
```

**Windows Command Prompt:**
```cmd
build.bat test         # Run tests
build.bat build        # Build everything
build.bat serve        # Start dev server
```

## 🎯 Language Design Goals

- **Simplicity**: Easy to learn Python-like syntax
- **Familiarity**: C-style blocks with `{}` and `;`
- **Modern**: Built-in constants, proper scoping
- **Portable**: Runs everywhere (CLI + Web)
- **Extensible**: Clean codebase for easy feature addition

## 🔧 Troubleshooting

### Windows Issues

**Problem: "execution of scripts is disabled on this system"**
```powershell
# Solution: Enable script execution (run as Administrator)
Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope CurrentUser
```

**Problem: "go: command not found" or "Go is not installed"**
1. Install Go from [golang.org/dl/](https://golang.org/dl/)
2. Restart your terminal/PowerShell
3. Verify with: `go version`

**Problem: "Could not find wasm_exec.js in Go installation"**
- Your Go installation may be incomplete
- Try reinstalling Go
- Or manually copy `wasm_exec.js` from your Go installation's `lib/wasm/` or `misc/wasm/` directory

### General Issues

**Problem: "Failed to load Tiger WASM" in browser**
1. Make sure you built the project: `.\build.ps1 build` (Windows) or `make build` (Unix)
2. Start a proper web server, don't open HTML files directly
3. Check browser console for specific error messages

**Problem: WebAssembly not working**
1. Ensure both `main.wasm` and `wasm_exec.js` are present
2. Check that files are served from a web server (not `file://` URLs)
3. Clear browser cache and try again

**Problem: Build fails**
1. Ensure Go 1.18+ is installed: `go version`
2. Run dependency installation: `go mod tidy`
3. Check that you're in the correct directory with `go.mod` file

### Getting Help

If you're still having issues:
1. Check that Go is properly installed and in PATH
2. Try building a simple Go program to verify your Go installation
3. Make sure you're running commands from the project root directory
4. Check file permissions on Unix systems

---

Built with ❤️ using Go and WebAssembly
