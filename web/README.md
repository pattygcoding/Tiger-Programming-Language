# Playground Syntax Highlighting

The playground uses a lightweight textarea overlay with no editor dependencies.
The native textarea owns input, selection, accessibility, and the caret. An
`aria-hidden` `<pre><code>` underneath renders colored tokens using text nodes,
never source-derived HTML. It cannot intercept pointer events.

## Tiger Grammar

The tokenizer in [highlight.mjs](highlight.mjs) follows
[the Tiger lexer](../crates/tiger-lexer/src/lib.rs). Classification is lexical, not semantic:
built-in names can be shadowed and are still colored as built-ins.

| Category | Grammar |
| --- | --- |
| Keywords | `const var function class extends super this public private protected return if elif else cif celse while for cfor in and or not break continue switch case default try catch throw import as` |
| Primitive literals | Numbers, strings, `true`, `false`, `null` |
| Numbers | Decimal digits, optional fractional part with digits after the dot, optional `e`/`E` exponent with optional sign; unary `+`/`-` are separate operators |
| Strings | Single or double quotes; escapes `\n`, `\r`, `\t`, `\xHH`, `\uXXXX`, `\UXXXXXXXX`, `\\`, `\"`, `\'` are highlighted separately; an `r`/`R` prefix makes the text raw |
| Formatted strings | `f`/`F` directly before either quote; string text, `{expression}` tokens, and `{{`/`}}` literal braces |
| Comments | `//` through the end of the line; `/* ... */` across one or more lines, without nesting |
| Symbolic operators | `= == != < <= > >= + - * / % ** ++ -- << >> & \| ^ ~ && \|\|` |
| Word operators | `and or not in div`, styled as keywords |
| Punctuation | `(` `)` `[` `]` `{` `}` `:` `,` `;` `.` |
| Identifiers | Unicode letters or `_`, followed by Unicode letters, decimal digits, or `_` |
| Built-in functions | `print input str int float bool len range type isinstance open read_file write_file append_file file_exists remove_file math algo` |
| Module members | Names exported by `math`, `algo`, `random`, `json`, `time`, and `regex` (such as `math.sqrt` or `algo.isPrime`), styled as built-ins only directly after the module name and a dot |

Tiger is dynamically typed. There are **no reserved primitive-type names or type
annotations**: `number` and `string` remain identifiers, and `int`, `float`, and `bool` are built-in conversion functions.
Lists, dictionaries, functions, classes, and instances are runtime values, not
additional type keywords. `this` is reserved; `self`, `def`, and `init` are ordinary identifiers (constructors use the class name). See the [complete editor checklist](../docs/syntax-highlighting.md).

Highlighting tolerates incomplete code while typing. Unclosed strings stop at a
line ending; unfinished exponents stay colored as numbers. Unknown characters
and invalid escapes are preserved. The Tiger runtime remains responsible for
validation and error messages.

## Integration

The complete implementation is in [index.html](index.html),
[style.css](style.css), [app.js](app.js), [worker.js](worker.js),
[tiger.js](tiger.js), and [highlight.mjs](highlight.mjs).
Serve these files together over HTTP, with the JavaScript entry point loaded as
`type="module"`; the server must also send the cross-origin isolation headers
described below.

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

The worker accepts `tigerRun(source, filename)`; the optional filename controls relative imports. The editor persists its source path alongside source text. Imports load same-origin `.tg` URLs synchronously inside the worker, leaving the page responsive and stoppable. Deploy imported module directories with the examples. Editor downloads contain only the entry source, not its dependencies. See [module semantics](../docs/modules.md).

Programs that call `input()` block the worker, and a blocked worker cannot
receive messages, so the page answers through a `SharedArrayBuffer` instead:
the worker posts `input`, waits on the buffer, and the page's inline field
writes the line into it. `SharedArrayBuffer` needs a cross-origin-isolated page,
so the server sends `Cross-Origin-Opener-Policy: same-origin` and
`Cross-Origin-Embedder-Policy: require-corp`. Without isolation the playground
still runs every other program, and `input()` reports that no input stream is
connected. Ctrl+D ends input, as it does on a terminal.

The browser build omits the `regex` and `time` modules and raises the documented
error for each, matching the runtime size budget. Everything else — the language,
`math`, `algo`, `random`, `json`, and the in-memory filesystem — is available.

The browser has no host filesystem, so each run receives a fresh in-memory filesystem. `open`, `read_file`, `write_file`, `append_file`, `file_exists`, and `remove_file` work against it for the duration of the run, which keeps file-I/O examples runnable in the playground without touching the user's disk. See [Files](../docs/file-io.md).

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
node --test web/highlight.test.mjs editors/vscode/grammar.test.mjs
```

`cargo test -p tiger-evaluator` also checks that `highlight.mjs` and the VS Code
grammar list exactly the members of the `math`, `algo`, `random`, `json`, `time`,
and `regex` modules.

Build the browser runtime from the repository root:

```sh
cargo run -p tiger-web -- -build-only
```

This builds `crates/tiger-wasm` for `wasm32-unknown-unknown` and writes
`web/tiger.wasm`, which [tiger.js](tiger.js) instantiates directly — there is no
`wasm_exec.js` and no bindgen step. The stripped runtime is about 660 KB (about
250 KB with gzip). If Binaryen's `wasm-opt` is on `PATH`, the build also runs
`wasm-opt -Oz --converge`. Deploy the web assets, including `highlight.mjs` and
`tiger.js`, and serve the repository's examples under `/examples/`. Alternatively,
start the included server:

```sh
cargo run -p tiger-web -- -addr 127.0.0.1:7171
```