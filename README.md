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

## 📦 Quick Start

### Option 1: Build from Source

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

### Option 2: WebAssembly in Browser

1. Build the WASM version: `make wasm`
2. Start a web server: `make serve`
3. Open `tiger_go.html` in your browser

## 🛠️ Build Commands

| Command | Description |
|---------|-------------|
| `make all` | Build CLI and WASM versions |
| `make cli` | Build CLI binary only |
| `make wasm` | Build WebAssembly version only |
| `make serve` | Start development server |
| `make clean` | Clean build artifacts |
| `make build-all` | Build for all platforms |

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

### Embedding in Your Website

1. Include the WASM files in your web directory:
   ```
   main.wasm
   wasm_exec.js
   ```

2. Add Tiger to your HTML:
   ```html
   <script src="wasm_exec.js"></script>
   <script>
       const go = new Go();
       WebAssembly.instantiateStreaming(fetch("main.wasm"), go.importObject)
           .then((result) => {
               go.run(result.instance);
           });

       function runTiger(code) {
           return evalTiger(code);
       }
   </script>
   ```

3. Execute Tiger code from JavaScript:
   ```javascript
   const result = evalTiger('let x = 42; print x;');
   console.log(result);
   ```

## 🖥️ CLI Usage

```bash
# Run a Tiger file
./tiger-cli run program.tg

# Start interactive REPL
./tiger-cli repl

# Help
./tiger-cli help
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
│   ├── main_wasm.go # WASM entry point
│   ├── lexer/       # Lexical analysis
│   ├── parser/      # Syntax analysis
│   ├── ast/         # Abstract Syntax Tree
│   └── eval/        # Interpreter/evaluator
├── tiger_go.html    # WASM demo page
├── Makefile         # Build system
└── README.md
```

## 🎯 Language Design Goals

- **Simplicity**: Easy to learn Python-like syntax
- **Familiarity**: C-style blocks with `{}` and `;`
- **Modern**: Built-in constants, proper scoping
- **Portable**: Runs everywhere (CLI + Web)
- **Extensible**: Clean codebase for easy feature addition

---

Built with ❤️ using Go and WebAssembly
