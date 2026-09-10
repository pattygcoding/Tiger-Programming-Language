# 2. Syntax

[Previous: Getting Started](getting-started.md) | [Guide index](README.md) | [Next: Variables and Scope](variables-and-scope.md)

## Statements and Blocks

Simple statements end in `;`. Compound statements use braces, not indentation or colons, to delimit their bodies.

```tg
# A line comment can precede a statement.
const limit = 3;
counter = 0; // This comment ends at the newline.
while counter < limit {
    print(counter);
    counter = counter + 1;
}
print("finished");
```

Output:

```text
0
1
2
finished
```

Indentation is for readability. Code can span multiple lines inside parentheses, list literals, dictionary literals, and blocks. Spaces and newlines outside strings do not terminate statements.

| Construct | Terminator |
| --- | --- |
| Assignment, including a field or index assignment | `;` |
| `const` declaration | `;` |
| Function or method call used as a statement | `;` |
| `return value` or bare `return` | `;` |
| Any other expression used as a statement | `;` |
| Function, class, conditional, or loop block | Closing `}`; a trailing `;` is optional |

An expression statement evaluates its expression but does not automatically display it. Use `print` for output. A dictionary literal uses colons between keys and values; block headers never do.

## Names and Keywords

Names start with a letter or `_`, followed by letters, digits, or `_`. Unicode letters are accepted. Names are case-sensitive: `score` and `Score` are different bindings.

Reserved keywords:

```text
and class const def elif else false for if in
not null or return super true while
```

`self` is not a reserved keyword, but every method must use it as the first parameter. `init` is the constructor method name. `print`, `str`, and `len` are built-in constant bindings, not keywords.

## Comments and Strings

`#` and `//` start single-line comments. Inside a quoted string they are ordinary characters. Block comments and multiline string literals are not supported.

```tg
print("# not a comment", "// also text");
print('single quotes', "double quotes");
```

Output:

```text
# not a comment // also text
single quotes double quotes
```

## Missing Semicolons

**Expected error:**

```tg
print("unfinished")
```

```error
expected ";"
```

The parser reports a line and column. The position may point at the next token or the end of the file where the missing terminator was noticed.

## Try It

Put both assignments from the loop example on the same line, keeping their semicolons. The output stays the same. Removing a semicolon does not become valid just because the next statement starts on a new line.