# The math Module

`math` is a built-in module, available in every file without an import. It is read-only: its functions can be called and stored like other values (`const root = math.sqrt;`), but its members cannot be reassigned. A declaration or import named `math` (such as `import "lib/math.tg" as math;`) shadows the built-in in that scope; see [Importing Tiger Files](modules.md).

## Functions

| Function | Result |
| --- | --- |
| `math.ceil(x)` | smallest integer `>= x` |
| `math.floor(x)` | largest integer `<= x` |
| `math.round(x)` | nearest integer; exact halves round to the even neighbor, like Python's `round` |
| `math.sqrt(x)` | square root; `x` must be `>= 0` |
| `math.abs(x)` | absolute value |
| `math.pow(x, y)` | `x` raised to `y` |
| `math.log(x)` | natural logarithm; `x` must be `> 0` |
| `math.sin(x)`, `math.cos(x)`, `math.tan(x)` | trigonometry in radians |
| `math.min(a, b)`, `math.max(a, b)` | smaller or larger of two numbers |

```tg
print(math.ceil(2.1), math.floor(-2.1), math.round(2.5), math.round(3.5), math.round(2.6));
print(math.sqrt(16), math.abs(-3.5), math.pow(2, 10), math.pow(9, 0.5));
print(math.log(1), math.sin(0), math.cos(0), math.tan(0));
print(math.min(3, -2), math.max(3, -2));

function hypotenuse(a, b) {
    return math.sqrt(math.pow(a, 2) + math.pow(b, 2));
}
print(hypotenuse(3, 4));
```

```text
3 -3 2 4 3
4 3.5 1024 3
0 0 1 0
-2 3
5
```

## Rounding

`ceil`, `floor`, and `round` return whole numbers. `round` uses banker's rounding, so halves go to the nearest even integer instead of always rounding up. To round to a number of decimal places, scale first:

```tg
function fixed(value, places) {
    const scale = math.pow(10, places);
    return math.round(value * scale) / scale;
}
print(math.round(0.5), math.round(1.5), math.round(-2.5));
print(fixed(math.log(2), 3), fixed(3.14159, 2));
```

```text
0 2 -2
0.693 3.14
```

## Arguments and Errors

Arguments must be numbers; booleans and numeric strings are rejected, so convert with `float` first. Each function takes a fixed number of positional arguments and no keyword arguments. Like Python, inputs outside a function's domain raise `math domain error` (`math.sqrt(-1)`, `math.log(0)`, `math.pow(0, -1)`, `math.pow(-8, 0.5)`), and a result too large to represent raises `math range error`. These errors can be caught with `try`/`catch`. `math` has no constants such as `pi`.

```tg
try {
    math.sqrt(-1);
} catch (error) {
    print("domain error" in error);
}
print(math.sqrt(float("2.25")));
```

```text
true
1.5
```

**Expected error:**

```tg
print(math.sqrt(-1));
```

```error
math.sqrt: math domain error
```

See also the [algo module](algo.md), the other [built-ins](builtins.md), and the [math benchmark](../benchmarks/23_math.tg).
