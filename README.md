# Tiger-Programming-Language
 
To compile the Tiger programming language to WebAssembly, you need to have Go and Rust installed on your system. The project uses Go for the interpreter and Rust for the WebAssembly compilation.:
```powershell
$env:GOOS="js"; $env:GOARCH="wasm"; go build -o main.wasm ./go # Compiles Go to WebAssembly
```
```powershell
wasm-pack build rust --target web --out-dir pkg-rust --release # Compiles Rust to WebAssembly
```


To clean compile:
```powershell
Remove-Item .\main.wasm -Force # Removes the old Go WebAssembly file generated from go
```
```powershell
cargo clean --manifest-path rust/Cargo.toml # Cleans the Rust project
Remove-Item -Recurse -Force rust/pkg-rust # May need to remove the pkg-rust folder if it exists
```

To obtain wasm_exec.js (for running Go WebAssembly):
```powershell
Copy-Item -Path "$(go env GOROOT)\misc\wasm\wasm_exec.js" -Destination ".\wasm_exec.js"
```

To run locally:
```powershell
python -m http.server 8000 # Go (requires Python)
```
```powershell
npx serve . # Rust (requires Node)
```
Then go to http://localhost:8000/tiger_go.html or http://localhost:3000/tiger_rust.html

# Syntax Rules

## File Structure
- Entry point: `main.tiger`
- Code is interpreted from top to bottom.

## Variable Declarations
let name = "Tiger"  
let age = 5  
let active = true  
let height = 5.9

### Types Supported:
- Strings: `"text"`
- Integers: `42`
- Floats: `3.14`
- Booleans: `true`, `false`

## Keywords
| Keyword     | Purpose                          |
|-------------|----------------------------------|
| let         | Variable declaration             |
| print       | Output to screen                 |
| if          | Conditional execution            |
| else        | Alternative branch               |
| while       | Loop while condition is true     |
| func        | Function declaration             |
| true/false  | Boolean literals                 |

## Expressions & Operators
| Type           | Example                             |
|----------------|-------------------------------------|
| Arithmetic     | `+`, `-`, `*`, `/`                  |
| Comparison     | `==`, `!=`, `<`, `>`, `<=`, `>=`    |
| Logical        | `&&`, `||`, `!`                     |
| Assignment     | `=`                                 |

## Print Statement
print "Hello, World!"  
print name  
print age + 5

## Conditional Statements
if age > 18 {  
&nbsp;&nbsp;&nbsp;&nbsp;print "Adult"  
} else {  
&nbsp;&nbsp;&nbsp;&nbsp;print "Minor"  
}

## Loops
let i = 0  
while i < 5 {  
&nbsp;&nbsp;&nbsp;&nbsp;print i  
&nbsp;&nbsp;&nbsp;&nbsp;let i = i + 1  
}

## Function Definitions and Calls

### Declare a function:
func greet(name) {  
&nbsp;&nbsp;&nbsp;&nbsp;print "Hello"  
&nbsp;&nbsp;&nbsp;&nbsp;print name  
}

### Call a function:
let user = "Tiger"  
greet(user)

### You can also use literals:
greet("Tiger")

> Functions currently do not return values — they are used for **side effects only** like printing.

## Reserved Words (cannot be used as variable names)
let, print, if, else, while, func, true, false

## Errors
- Undefined variables cause runtime errors.
- Type mismatch in arithmetic/logical operations is not allowed.
- Blocks must be enclosed in `{}`.
- Function calls must match parameter count.
