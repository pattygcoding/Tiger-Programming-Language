# Importing Tiger Files

[Guide index](README.md) | [Running and Building](running-and-building.md)

## Import a Module

Use `import "path/to/file.tg" as name;`, then access its declarations through the alias. The path must be a literal string ending in `.tg`; the alias and semicolon are required. Single and double quotes both work. Paths are relative to the file containing the import, not the process working directory.

This example uses the repository's [math module](../examples/modules/math.tg):

```tg
import "../examples/modules/math.tg" as math;
print(math.square(9));
print(f"Area: {math.circle_area(2)}");
```

```text
81
Area: 12.56636
```

Run [the complete import example](../examples/imports.tg) with `tiger run examples/imports.tg`, or select `imports.tg` in the playground.

## Scope and Exports

Each module has its own lexical environment and built-ins. It cannot see caller-local variables. Its top-level variables, constants, functions, classes, and imported aliases are automatically exported; built-ins and block-local names are not. There is no `export`, wildcard import, or named-import syntax.

The import alias is a constant binding in its current scope. Module properties are read-only from outside: `math.PI = 4;` is an error, even for an exported `var`. Exported functions may update their own module state, and exported collections or instances remain mutable under their ordinary rules. Functions keep their defining environment, so imports inside an exported function remain relative to that function's module.

Imports can appear in ordinary blocks or functions and execute when reached. Successful modules execute once per run, even when imported under different aliases; aliases then refer to the same module instance and live state. Each new execution has a fresh module cache. Circular imports are errors; failed imports are not cached and may be retried. Imports share the caller's execution-step budget and have a maximum nesting depth of 128. Missing files, syntax errors, and initialization failures are reported with source locations and may be caught by a surrounding `try`/`catch`, except uncatchable execution limits.

Imported classes can be constructed with `math.SomeClass()`. To inherit from an imported class, first bind it to a local name (`const Base = module.Base;`), then use `class Child(Base) { ... }`.

## Runtime Differences

| Runtime | Module source |
| --- | --- |
| Native CLI | Files relative to the importing file; absolute paths also supported |
| WASI | Files permitted by the host's filesystem grants |
| Standalone executable | All literal dependencies bundled during `tiger build`; original `.tg` files are unnecessary at runtime |
| Browser playground | Same-origin `.tg` URLs relative to the source example; no local disk access or cross-origin imports |
| Go embedding | Explicit `ModuleLoader`, `FileLoader`, `FSLoader`, or `BundleLoader` |

Use forward slashes for portable paths. Standalone builds discover dependencies even in functions and branches that do not execute, so every referenced file must exist and parse when building. Browser deployments must publish module subdirectories together with the examples. Browser module requests happen inside the worker; Stop terminates a blocked or long-running import along with that worker. Downloading editor source downloads only the entry file, not a multi-file bundle.

File imports are executable code, not a data-loading sandbox. Only run trusted local modules. Importing does not provide arbitrary file-reading or network APIs to Tiger programs.

## Go API

`evaluator.RunFile(filename, writer)` reads an entry file and enables filesystem imports. `evaluator.Run(source, writer)` deliberately has no loader and reports an error if an import executes.

For virtual filesystems, call `evaluator.RunWithLoader(source, "main.tg", evaluator.FSLoader{FS: files}, writer)`. `FSLoader` rejects absolute paths and paths escaping its filesystem root. A custom `ModuleLoader` implements `Resolve(importer, requested) (string, error)` and `Load(filename) (string, error)`; resolved names must be stable canonical cache keys.

For direct `Evaluator.Execute` calls, set `Loader` and `SourcePath`; use `parser.ParseSource(source, filename)` for filename-aware diagnostics. Reusing an evaluator resets module state on each execution and is not concurrency-safe.