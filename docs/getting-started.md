# 1. Getting Started

[Guide index](README.md) | [Next: Syntax](syntax.md)

## Your First Program

Tiger source files end in `.tg`. With Go 1.22 or newer installed, run a source file from the repository root:

```sh
go run ./cmd/tiger run examples/demo.tg
```

For your own program, use the same command with your file's path. Here is a complete first program:

```tg
const greeting = "Hello, Tiger!";
print(greeting);
print("The answer is", 6 * 7);
```

Output:

```text
Hello, Tiger!
The answer is 42
```

`const` creates a binding that cannot be reassigned. `print` writes values separated by spaces, then a newline. The semicolon finishes each simple statement; a newline alone does not.

## A Small Calculation

```tg
const price = 12;
const quantity = 3;
var total = price * quantity;
if total >= 30 {
    total = total - 5;
}
print("Total:", total);
```

Output:

```text
Total: 31
```

The assignment inside the `if` updates the existing `total`. Braces delimit the body, and there is no colon after the condition.

## Use the Browser

Start the local playground from the repository root:

```sh
go run ./cmd/web
```

Open `http://127.0.0.1:8080`, choose an example or edit the source, and select **Run**. **Stop** terminates the current worker. **Reset** restores the selected example. The editor retains source locally between visits, but variables and objects do not persist across runs.

## Try It

Change `quantity` to `2`. The discount condition becomes false, and the result is `24`. Then make `price` a constant: reading it in the multiplication still works, but assigning it a second value would fail.

See [Running and Building](running-and-building.md) for native executables, standalone builds, PowerShell commands, and Wasm targets.