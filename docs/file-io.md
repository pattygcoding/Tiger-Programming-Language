# 10. Files

[Previous: Classes and Inheritance](classes-and-inheritance.md) | [Guide index](README.md) | [Next: Built-ins](builtins.md)

Tiger can read and write files with the Python-style `open` function and the
convenience helpers `read_file`, `write_file`, `append_file`, `file_exists`, and
`remove_file`. File I/O is available on the native CLI, in standalone
executables, and under WASI. The browser playground has no host filesystem, so
each run gets a fresh in-memory filesystem with the same behavior.

Relative paths resolve against the process working directory, exactly like
`open` in Python. Absolute paths are used as given.

## Write, Append, and Read

`write_file(path, text)` creates or truncates a file and writes `text`.
`append_file(path, text)` adds `text` at the end without erasing the current
contents. Both return the number of bytes written. `read_file(path)` returns the
whole file as a string.

```tg
const PATH = "notes.txt";
print(write_file(PATH, "alpha\nbeta\n"));
print(append_file(PATH, "gamma\n"));
print(read_file(PATH), end="");
remove_file(PATH);
```

```text
11
6
alpha
beta
gamma
```

## open(path, mode="r") and File Objects

`open` returns a file object. The default mode is `"r"`. Reads go through a
stream that remembers its position, so `readline` returns each line in turn
including its trailing newline; at end of file it returns `""`.

```tg
write_file("notes.txt", "first\nsecond\nthird\n");
const file = open("notes.txt", "r");
print(file.name, file.mode, file.closed);
var count = 0;
while true {
    const line = file.readline();
    if line == "" { break; }
    count = count + 1;
    print(count, line, end="");
}
file.close();
print(file.closed);
```

```text
notes.txt r false
1 first
2 second
3 third
true
```

## Modes

| Mode | Reads | Writes | Creates | Truncates | Position |
| --- | --- | --- | --- | --- | --- |
| `r` | Yes | No | No | No | Start |
| `w` | No | Yes | Yes | Yes | Start |
| `a` | No | Yes | Yes | No | End for writes |
| `r+` | Yes | Yes | No | No | Start |
| `w+` | Yes | Yes | Yes | Yes | Start |
| `a+` | Yes | Yes | Yes | No | End for writes |

Append a `b` for a binary stream (`"rb"`, `"wb"`, `"ab"`, `"r+b"`, `"rb+"`, and
so on). Tiger has no separate byte type, so a string holds the raw bytes in
either case; text mode and binary mode differ only in intent. Any other mode is
an error.

## File Object Members

| Member | Effect |
| --- | --- |
| `read(size?)` | All remaining text, or at most `size` bytes |
| `readline()` | Next line including `"\n"`, or `""` at end of file |
| `readlines()` | List of the remaining lines |
| `write(text)` | Writes a string and returns the number of bytes written |
| `writelines(list)` | Writes each string in a list |
| `flush()` | Flushes buffered data to the host file |
| `close()` | Closes the stream; further use is an error |
| `seek(offset, origin?)` | Moves the position and returns it |
| `tell()` | Returns the current byte position |
| `closed` | `true` after `close()` |
| `name` | The path passed to `open` |
| `mode` | The mode passed to `open` |

`seek` accepts an origin of `0` (start, the default), `1` (current), or `2`
(end). Positions are byte offsets. Read methods refuse a stream opened without
read access, and `write` refuses a stream opened without write access.

```tg
write_file("notes.txt", "alpha\nbeta\ngamma\n");
const file = open("notes.txt");
print(file.readlines());
file.close();
const probe = open("notes.txt");
print(probe.read(5), probe.tell());
probe.seek(6);
print(probe.readline(), end="");
probe.seek(0, 2);
print(probe.tell());
probe.close();
```

```text
["alpha\n", "beta\n", "gamma\n"]
alpha 5
beta
17
```

## Updating and Appending in Place

```tg
write_file("notes.txt", "one\ntwo\n");
const update = open("notes.txt", "r+");
print(update.read(), end="");
update.seek(0);
update.write("ONE");
update.close();
print(read_file("notes.txt"), end="");
const log = open("notes.txt", "a+");
log.write("three\n");
log.seek(0);
print(log.readlines());
log.close();
```

```text
one
two
ONE
two
["ONE\n", "two\n", "three\n"]
```

## Binary Streams

```tg
const handle = open("data.bin", "wb");
print(handle.write("PQRS"));
handle.close();
const reader = open("data.bin", "rb");
print(reader.read());
reader.close();
remove_file("data.bin");
```

```text
4
PQRS
```

## Checking, Removing, and Printing a File

`file_exists(path)` reports whether a regular file can be opened, and
`remove_file(path)` deletes it. `print` and `str` show an open file as
`<file "name">`.

```tg
const file = open("notes.txt", "w");
print(file);
file.close();
print(file_exists("notes.txt"), file_exists("absent.txt"));
remove_file("notes.txt");
print(file_exists("notes.txt"));
```

```text
<file "notes.txt">
true false
false
```

File objects are not closed automatically when they go out of scope. Close each
stream when you are finished; on Windows an unclosed handle can prevent the file
from being removed or reopened.

## Errors

A missing file, an invalid mode, or misuse of a closed or write-only stream is
an ordinary runtime error. It stops the program unless a `try`/`catch` handles
it (see [Runtime and Errors](runtime-and-errors.md)).

```tg
try {
    read_file("absent.txt");
} catch (error) {
    print("caught:", "absent.txt" in str(error));
}
try {
    open("notes.txt", "q");
} catch (error) {
    print("caught:", "invalid mode" in str(error));
}
```

```text
caught: true
caught: true
```

**Expected error:** reading a closed file.

```tg
write_file("notes.txt", "x");
const file = open("notes.txt");
file.close();
file.read();
```

```error
I/O operation on closed file
```

## Where Files Live

| Target | Filesystem |
| --- | --- |
| Native CLI, standalone executables | The host filesystem |
| WASI | The host filesystem, limited to directories the host grants |
| Browser playground | A fresh in-memory filesystem for each run |

See [Running and Building](running-and-building.md) for host setup. The
[file I/O benchmark](../benchmarks/19_file_io.tg) and the
[file I/O example](../examples/file_io.tg) exercise every mode and method.

[Previous: Classes and Inheritance](classes-and-inheritance.md) | [Guide index](README.md) | [Next: Built-ins](builtins.md)