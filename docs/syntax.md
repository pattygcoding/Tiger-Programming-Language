# 2. Syntax

[Previous: Getting Started](getting-started.md) | [Guide index](README.md) | [Next: Variables and Scope](variables-and-scope.md)

## Statements and Blocks

Simple statements end in `;`. Compound statements use braces, not indentation or colons, to delimit their bodies.

```tg
// A line comment can precede a statement.
const limit = 3;
var counter = 0; // This comment ends at the newline.
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
| `const` or `var` declaration | `;` |
| Function or method call used as a statement | `;` |
| `return value` or bare `return` | `;` |
| `break`, `continue`, or `throw value` | `;` |
| Any other expression used as a statement | `;` |
| Function, class, conditional, loop, switch, or try/catch block | Closing `}`; a trailing `;` is optional |

An expression statement evaluates its expression but does not automatically display it. Use `print` for output. Dictionaries use colons between keys and values; switch `case` and `default` labels also end in colons. Other block headers do not.

## Names and Keywords

Names start with a letter or `_`, followed by letters, digits, or `_`. Unicode letters are accepted. Names are case-sensitive: `score` and `Score` are different bindings.

Reserved keywords:

```text
and break case catch cfor class const continue default
elif else false for function if in not null or private
protected public return super switch this throw true try var while
```

`this` is reserved and every method uses it as the first parameter. `init` is the constructor method name, not a keyword. `print`, `str`, `len`, and `range` are built-in constant bindings, not keywords. See the [editor highlighting reference](syntax-highlighting.md) for token categories.

## Comments and Strings

`//` starts a single-line comment. `/*` starts a block comment, which ends at the next `*/` and may span multiple lines. Block comments do not nest; an unclosed block comment is a syntax error. `#` is not a comment marker. Inside quoted strings, comment markers are ordinary characters. Multiline string literals are not supported.

```tg
// A single-line comment.
/* A block comment
    spanning two lines. */
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

## Migrating Older Programs

Replace `def` with `function` for functions and methods, and replace `#` comments with `//` or `/* ... */`. Prefix each variable's initial assignment with `const` unless that binding will be reassigned, in which case use `var`. Later assignments keep the form `name = value;`. The old function and comment syntax is no longer accepted.

Rename each method's first parameter and receiver references from `self` to `this`. `self` is now just an ordinary identifier and is not accepted as a method receiver declaration. Existing unmodified class methods remain public; adding `public` is optional.

## Try It

Put both assignments from the loop example on the same line, keeping their semicolons. The output stays the same. Removing a semicolon does not become valid just because the next statement starts on a new line.