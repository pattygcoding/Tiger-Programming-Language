# Playground Syntax Highlighting

The playground uses a lightweight textarea overlay with no editor dependencies.
The native textarea owns input, selection, accessibility, and the caret. An
`aria-hidden` `<pre><code>` underneath renders colored tokens using text nodes,
never source-derived HTML. It cannot intercept pointer events.

## Tiger Grammar

The tokenizer in [highlight.mjs](highlight.mjs) follows
[the Tiger lexer](../pkg/lexer/lexer.go). Classification is lexical, not semantic:
built-in names can be shadowed and are still colored as built-ins.

| Category | Grammar |
| --- | --- |
| Keywords | `const def class super return if elif else while for in and or not` |
| Primitive literals | Numbers, strings, `true`, `false`, `null` |
| Numbers | Decimal digits, optional fractional part with digits after the dot, optional `e`/`E` exponent with optional sign; unary `+`/`-` are separate operators |
| Strings | Single or double quotes; escapes `\n`, `\r`, `\t`, `\\`, `\"`, `\'` |
| Comments | `#` or `//` through the end of the line; no block comments |
| Symbolic operators | `= == != < <= > >= + - * / %` |
| Word operators | `and or not in`, styled as keywords |
| Punctuation | `(` `)` `[` `]` `{` `}` `:` `,` `;` `.` |
| Identifiers | Unicode letters or `_`, followed by Unicode letters, decimal digits, or `_` |
| Built-in functions | `print str len` |

Tiger is dynamically typed. There are **no reserved primitive-type names or type
annotations**: `number`, `string`, `bool`, `int`, and `float` remain identifiers.
Lists, dictionaries, functions, classes, and instances are runtime values, not
additional type keywords. `self` is also an ordinary identifier.

Highlighting tolerates incomplete code while typing. Unclosed strings stop at a
line ending; unfinished exponents stay colored as numbers. Unknown characters
and invalid escapes are preserved. The Tiger runtime remains responsible for
validation and error messages.

## Integration

The complete implementation is in [index.html](index.html),
[style.css](style.css), [app.js](app.js), and [highlight.mjs](highlight.mjs).
Serve these files together over HTTP, with the JavaScript entry point loaded as
`type="module"`. The existing Go web server serves the module MIME types.

For another textarea, use the same `.editor` wrapper, `#source` textarea with
`wrap="off"`, and `#source-highlight` pre containing an empty code element. Import
and call the helper after any initial source restoration:

```js
import { attachHighlighting } from "./highlight.mjs";

const source = document.querySelector("#source");
const refreshHighlighting = attachHighlighting(
  source,
  document.querySelector("#source-highlight"),
);

source.value = "print('Hello, Tiger!');";
refreshHighlighting();
```

Input and scrolling synchronize automatically. Call the returned refresh
function after programmatic changes such as `value` assignment or `setRangeText`.
The playground already does this for example loading, Reset, and Tab insertion.
Local storage, downloads, and Wasm execution use only the original textarea value.

Both layers share padding, font, line height, tab width, and disabled ligatures.
Lines do not wrap. A resize observer matches the mirror to the textarea's client
area, excluding native scrollbars, and scroll events synchronize both axes.
A display-only zero-width sentinel preserves the final empty line. Token styles
change color only, keeping character measurements stable. In forced-colors mode,
the native text is shown instead. The textarea also stays visible if JavaScript
does not initialize.

## Verification and Build

Run the tokenizer regression tests with Node.js 18 or newer:

```sh
node --test web/highlight.test.mjs
```

Build the browser runtime from the repository root:

```sh
go run ./cmd/web -build-only
```

This generates `web/tiger.wasm` and copies the matching Go `wasm_exec.js`.
Deploy both together with the web assets, including `highlight.mjs`, and serve the
repository's examples under `/examples/`. Alternatively, start the included server:

```sh
go run ./cmd/web -addr 127.0.0.1:8080
```