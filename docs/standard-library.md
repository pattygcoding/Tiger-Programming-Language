# Standard Library Additions

[Guide index](README.md) | [Built-ins](builtins.md) | [Files](file-io.md)

Beyond `print`/`len`/conversions and the [`math`](math.md) and [`algo`](algo.md) modules, Tiger ships six more read-only modules — `random`, `secrets`, `json`, `time`, `regex`, and `sys` — plus the `list_dir` file helper. All are available without an import.

## random

`random` is a deterministic xorshift64* generator. It needs no clock, so it works on every target, including the browser. The default seed is fixed, so an unseeded program produces the same sequence every run; `random.seed(n)` reseeds it.

| Function | Result |
| --- | --- |
| `random.seed(n)` | Reseeds the generator; `0` falls back to the default seed |
| `random.random()` | A number in `[0, 1)` |
| `random.int(low, high)` | An integer in `[low, high]`, both inclusive |
| `random.uniform(low, high)` | A number in `[low, high)` |
| `random.choice(list)` | A random element; an empty list is an error |
| `random.shuffle(list)` | Shuffles a list in place and returns `null` |

```tg
random.seed(7);
var first = [random.int(0, 1000), random.int(0, 1000)];
random.seed(7);
var second = [random.int(0, 1000), random.int(0, 1000)];
print(first == second);
print(random.int(4, 4), random.choice(["only"]));
var cards = [1, 2, 3, 4, 5];
random.shuffle(cards);
cards.sort();
print(cards);
```

Output:

```text
true
4 only
[1, 2, 3, 4, 5]
```

## secrets

`secrets` produces non-reproducible randomness. It needs no seeding: the module
draws from host entropy when it is created and mixes in a fresh reading on every
call, so two runs of the same program differ. Use it for anything that must not
repeat, such as a one-time code or a shuffled deck, and use [`random`](#random)
when a program has to be reproducible. It is available on the native CLI, in
standalone executables, and under WASI; the browser build has no host entropy and
raises an error instead.

| Function | Result |
| --- | --- |
| `secrets.random()` | A number in `[0, 1)` |
| `secrets.int(low, high)` | An integer in `[low, high]`, both inclusive |
| `secrets.uniform(low, high)` | A number in `[low, high)` |
| `secrets.choice(list)` | A random element; an empty list is an error |
| `secrets.shuffle(list)` | Shuffles a list in place and returns `null` |

```tg
const fraction = secrets.random();
print(fraction >= 0 and fraction < 1);
const roll = secrets.int(1, 6);
print(roll >= 1 and roll <= 6);
const faces = ["north", "south", "east"];
print(secrets.choice(faces) in faces);
```

Output:

```text
true
true
true
```

Every call draws a fresh value, so there is no `secrets.seed` and no way to
reproduce a sequence; use `random` when reproducibility matters.

## json

`json.parse(text)` converts JSON text into Tiger values: objects become dictionaries, arrays become lists, and scalars become strings, numbers, `true`/`false`, or `null`. `json.stringify(value)` produces compact JSON. Dictionaries must have string keys, a tuple is written as an array, a set is rejected (it has no order to preserve), and cyclic structures are rejected.

```tg
const config = json.parse("{\"port\": 8080, \"tags\": [\"a\", \"b\"], \"on\": true}");
print(config["port"], config["tags"]);
print(json.stringify({"ok": [1, null]}));
print(json.stringify(json.parse("[1, 2.5, 1e2]")));
```

Output:

```text
8080 ["a", "b"]
{"ok":[1,null]}
[1,2.5,100]
```

**Expected error:**

```tg
json.parse("{oops}");
```

```error
json.parse
```

## time

`time` reads the host clock. It is available on the native CLI, standalone executables, and WASI; the browser build has no clock and raises an error instead. Timestamps are Unix seconds in UTC.

| Function | Result |
| --- | --- |
| `time.now()` | Current Unix seconds, with a fractional part |
| `time.now_ms()` | Current Unix milliseconds |
| `time.sleep(seconds)` | Pauses for a non-negative number of seconds |
| `time.iso(seconds)` | `YYYY-MM-DDTHH:MM:SSZ` |
| `time.parts(seconds)` | A dictionary of `year`, `month`, `day`, `hour`, `minute`, `second`, `weekday` (0 = Sunday), and `yearday` |

```tg
print(time.iso(0), time.iso(1600000000));
const moment = time.parts(1600000000);
print(moment["year"], moment["month"], moment["day"], moment["weekday"]);
```

Output:

```text
1970-01-01T00:00:00Z 2020-09-13T12:26:40Z
2020 9 13 0
```

## regex

`regex` wraps the standard RE2 engine on native and WASI builds; the browser build omits it and raises an error. Patterns support `.`, character classes, `\d`/`\w`/`\s`, the anchors `^` and `$`, groups, alternation, and repetition. Backreferences and lookaround are not supported.

| Function | Result |
| --- | --- |
| `regex.match(pattern, text)` | `true` if the pattern matches anywhere in `text` |
| `regex.find(pattern, text)` | The first match, or `null` |
| `regex.findAll(pattern, text)` | Every non-overlapping match, as a list |
| `regex.groups(pattern, text)` | The capture groups of the first match, or `null` |
| `regex.replace(pattern, text, replacement)` | `text` with every match replaced |
| `regex.split(pattern, text)` | The pieces of `text` between matches, as a list |

```tg
print(regex.match("^\\w+\\d$", "item7"));
print(regex.find("\\d+", "order 42 shipped"));
print(regex.findAll("\\d+", "1 22 333"));
print(regex.groups("(\\w+)@(\\w+)", "user@host"));
print(regex.replace("\\s+", "a b  c", "-"));
print(regex.split("\\s+", "a b  c"));
```

Output:

```text
true
42
["1", "22", "333"]
["user", "host"]
a-b-c
["a", "b", "c"]
```

**Expected error:**

```tg
regex.match("[", "x");
```

```error
invalid pattern
```

## sys

`sys` is the program's own context: the arguments it was started with, the environment it inherited, and the status it exits with. It follows Python's `sys`: `sys.argv[0]` is the program's own path, `sys.env` plays the part of `os.environ`, and `sys.exit` stops the program with the status Python would use.

| Member | Result |
| --- | --- |
| `sys.argv` | The program's arguments as a list, its own path first |
| `sys.env` | A dictionary of the inherited environment variables. It is a snapshot: changing the dictionary does not change the process |
| `sys.exit(status)` | Stops the program. An integer is the process status, `null` means success, and any other value is printed on standard error with status 1 |

```sh
tiger run greet.tg Ada Grace   # sys.argv is ["greet.tg", "Ada", "Grace"]
tiger run status.tg            # exits with whatever the program chose
```

A host that runs a program without arguments leaves `sys.argv` empty, which is what the browser playground does. `sys.env` reports the variables the program was started with; the browser build has no environment to read, so its `sys.env` is empty.

```tg
// With no arguments given, everything after the program's own path is empty, and
// the members keep their shape.
print(sys.argv[1:], type(sys.argv), type(sys.env));
print("TIGER_NO_SUCH_VARIABLE" in sys.env, sys.env.get("TIGER_NO_SUCH_VARIABLE", "unset"));
```

Output:

```text
[] list dict
false unset
```

`sys.exit` stops the program where it stands: nothing after the call runs, every `finally` block on the way out still does, and no `catch` clause takes it — a program stops for the same reason Python's `sys.exit` does. The status reaches the caller, so a program can report failure without printing anything, and `tiger run` passes it on.

```tg
function finish() {
    try {
        print("checking");
        sys.exit(0);
    } finally {
        print("cleanup");
    }
}
finish();
print("not reached");
```

Output:

```text
checking
cleanup
```

Because the language has one number type, an integral argument is a status and a fractional one is Python's "print the value and exit 1" case: `sys.exit(2)` exits with 2, while `sys.exit(2.5)` prints `2.5` and exits with 1. A status outside the range a process can report is treated the same way, as a message. Nothing else about the module depends on the host: `sys.argv` is a list, `sys.env` is a dictionary, and every program that runs to its end exits with 0.

**Expected error:**

```tg
sys.exit(1, "extra");
```

```error
sys.exit expects at most 1 argument
```

## list_dir(path)

`list_dir` returns a sorted list of the immediate entries in a directory. On the host filesystem, directory entries end with `/`; the browser's in-memory filesystem holds only files. A missing directory is an error.

```tg
write_file("notes/a.txt", "alpha");
write_file("notes/b.txt", "beta");
write_file("readme.txt", "top");
print(list_dir("notes"));
print(list_dir("."));
```

Output:

```text
["a.txt", "b.txt"]
["readme.txt"]
```

See [Files](file-io.md) for the other file helpers and [Built-ins](builtins.md) for the module list.

[Guide index](README.md) | [Built-ins](builtins.md) | [Files](file-io.md)
