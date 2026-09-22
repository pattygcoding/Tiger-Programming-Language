# Editor Highlighting Reference

[Guide index](README.md) | [Syntax](syntax.md) | [Playground implementation](../web/README.md)

Use `.tg` as the source extension and `tiger` as the editor language ID. The authoritative tokens are in [the lexer](../pkg/lexer/lexer.go); the dependency-free browser implementation is [highlight.mjs](../web/highlight.mjs).

## Lexical Categories

| Category | Tokens or rules | Suggested scope |
| --- | --- | --- |
| Declarations | `const var function class` | `storage.type.tiger` |
| Module imports | `import as` | `keyword.control.import.tiger` |
| Access modifiers | `public private protected` | `storage.modifier.tiger` |
| Control flow | `if elif else while for cfor return break continue switch case default try catch throw` | `keyword.control.tiger` |
| Receiver / parent | `this super` | `variable.language.tiger` |
| Word operators | `and or not in` | `keyword.operator.word.tiger` |
| Literal values | `true false null` | `constant.language.tiger` |
| Built-in calls | `print str len range` | `support.function.tiger` |
| Arithmetic / update | `+ - * / % ++ --` | `keyword.operator.arithmetic.tiger` |
| Assignment / comparison | `= == != < <= > >=` | `keyword.operator.tiger` |
| Delimiters | `( ) [ ] { } , ; : .` | `punctuation.tiger` |
| Numbers | Decimal digits, optional fractional digits, optional `e`/`E` exponent with optional sign | `constant.numeric.tiger` |
| Strings | Single or double quoted, one line | `string.quoted.single.tiger` / `string.quoted.double.tiger` |
| Formatted strings | `f"..."`, `f'...'` or uppercase `F`; `{expression}` interpolation, `{{` / `}}` literal braces | `string.quoted` with `meta.interpolation.tiger` |
| Escapes | `\n \r \t \\ \" \'` | `constant.character.escape.tiger` |
| Line comments | `//` through newline | `comment.line.double-slash.tiger` |
| Block comments | `/*` through the next `*/`, including newlines; no nesting | `comment.block.tiger` |
| Identifiers | Unicode letter or `_`, followed by letters, decimal digits, or `_` | `variable.other.tiger` |

Recognize comments and strings before operators; markers inside strings are text. Match `++`, `--`, `==`, `!=`, `<=`, and `>=` before their single-character prefixes. Signs are operators, not part of numeric literals except within exponents. Keyword matching must respect identifier boundaries.

`range` is a built-in binding, not a reserved keyword. `init` is a conventional constructor name, not a keyword. `self`, `def`, `number`, `string`, `bool`, `int`, and `float` are ordinary identifiers. `#` is invalid outside strings. There are no backtick templates, hexadecimal literals, type annotations, `+=`, `&&`, `||`, or standalone `!` operators. F-string format specifications and conversions are not supported.

Within f-strings, highlight literal text as string, interpolation delimiters as punctuation, and embedded expressions using normal Tiger token rules. Track nested dictionary braces, quoted strings, comments, and nested f-strings so their braces do not prematurely close an interpolation. Doubled literal braces are string text. The `f`/`F` prefix must directly precede the opening quote.

## Optional Semantic Highlighting

| Symbol | Suggested semantic token |
| --- | --- |
| Name declared by `class`, including parent references | `class` |
| Import alias and resolved module references | `namespace` |
| Name declared by `function` and resolved calls | `function` or `method` |
| Function/method parameters and catch binding | `parameter` or `variable` |
| `const` variable / field | `variable` / `property` with `readonly` |
| `var`, for iteration binding, cfor initializer binding | `variable` |
| Declared and dynamically assigned fields | `property` |
| Built-in binding when not shadowed | `function` with `defaultLibrary` |

Semantic classification needs scope resolution. A lightweight lexical highlighter may color built-in spellings everywhere, but must not claim that a shadowed `range` is necessarily the built-in. Omitted class access modifiers mean public; do not mark them as missing or erroneous.

## Editor Checklist

- Pair and indent braces, brackets, parentheses, and quotes.
- Configure `//` line-comment toggling and `/* ... */` block-comment toggling.
- Keep multiline comment state across lines and stop at the first `*/`.
- Recover incomplete strings at newline; allow an unfinished block comment to remain colored to end of file while typing.
- Distinguish declaration names, parameters, member names, constants, and calls when semantic information is available.
- Highlight prefix and postfix updates identically as operators.
- Fold class/function/loop/try/catch/switch bodies and multiline comments.
- Do not treat switch-label colons as Python-style block syntax.
- Test Unicode identifiers, escaped quotes, comment markers in strings, adjacent operators, and incomplete input.
- Test f-strings with same-quote indexing, nested expressions, escaped braces, and unfinished interpolation.
- Preserve every source character and render source as text, never HTML.

## Smoke Example

```tg
/* All major categories in one executable sample. */
class Counter {
    private var value = 0;
    protected const step = 1;
    public function next(this) {
        this.value = this.value + this.step;
        return this.value;
    }
}
const counter = Counter();
cfor (var index = 0; index < 2; ++index) {
    switch counter.next() {
        case 1: print("first"); break;
        default: continue;
    }
} else { print("complete"); }
try { throw range(2); } catch (error) { print(error); }
```

```text
first
complete
[0, 1]
```