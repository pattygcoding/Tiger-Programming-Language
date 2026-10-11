# 5. Strings

[Previous: Types and Operators](types-and-operators.md) | [Guide index](README.md) | [Next: Lists and Dictionaries](collections.md)

## Create and Combine Text

```tg
const name = 'Tiger';
const version = 1;
print("Hello, " + name + "!");
print(name + " version " + str(version));
print("first\nsecond");
```

Output:

```text
Hello, Tiger!
Tiger version 1
first
second
```

Single and double quotes have the same meaning. `+` joins strings, but it will not automatically turn a number into text. `print` accepts separate values of different types; concatenation requires explicit conversion with `str` or `.ToString()`. Convert numeric text back with `int` or `float`, as described in [conversions](builtins.md#intvalue-floatvalue-boolvalue).

## Escape Reference

| Escape | Meaning |
| --- | --- |
| `\n` | Newline |
| `\r` | Carriage return |
| `\t` | Tab |
| `\\` | Backslash |
| `\"` | Double quote |
| `\'` | Single quote |
| `\xHH` | The character with a two-digit hexadecimal code point |
| `\uXXXX` | The character with a four-digit hexadecimal code point |
| `\UXXXXXXXX` | The character with an eight-digit hexadecimal code point |

```tg
print("\u0041\u00e9\u4e2d", "\x41\x42", "\U0001F600");
```

Output:

```text
Aé中 AB 😀
```

Unknown escapes, incomplete hexadecimal escapes, surrogate code points, and values above `U+10FFFF` are lexical errors. Literal Unicode text is also supported, so `"é"` and `"\u00e9"` are the same string.

### Raw Strings

An `r` or `R` prefix makes a string raw: backslashes are literal, and there are no escapes or interpolation. Raw strings are convenient for regular-expression patterns.

```tg
print(r"\d+", len(r"\n"));
print(regex.find(r"\d+", "order 42"));
```

Output:

```text
\d+ 2
42
```

A raw string cannot contain its own quote delimiter, because there is no escape mechanism; use the other quote instead. There are no triple-quoted or multiline literals.

## Formatted Strings

```tg
const name = "Tiger";
const scores = {"total": 12};
print(f"Hello, {name}! Total: {scores["total"]}");
print(f'Next: {scores["total"] + 1}; literal braces: {{value}}');
const message = F"Values: {range(3)}";
print(message);
```

```text
Hello, Tiger! Total: 12
Next: 13; literal braces: {value}
Values: [0, 1, 2]
```

Prefix a single- or double-quoted string with `f` or `F`, without whitespace. Each `{expression}` is evaluated in the current scope, left to right, and converted using the same formatting as `str`. Calls, member access, indexing, dictionaries, arithmetic, and nested f-strings work inside braces. Quotes inside expressions may match the outer quotes. Write `{{` and `}}` for literal braces; ordinary string escapes still apply to literal text.

F-strings are ordinary string-valued expressions, not just a special `print` feature: assign, return, concatenate, or throw them. Empty expressions, unmatched braces, and invalid expressions are errors. Errors inside an expression retain its source position and can be caught when they are runtime errors. Conversions (`!r`, `!s`) and debug expressions (`{name=}`) are not supported.

### Format Specifications

A field may include a format specification after a colon: `{expression:spec}`, where the spec is `[<|>|^][0][width][.Nf]`.

| Spec | Meaning |
| --- | --- |
| `.Nf` | Fixed-point with `N` decimals (`0` through `17`) |
| `width` | Pad to at least `width` code points |
| `<`, `>`, `^` | Left, right, or center alignment; the default is left for strings and right for numbers |
| `0` | Pad with zeros instead of spaces |

```tg
const price = 3.5;
print(f"{price:.2f}");
print(f"[{price:>8.2f}]");
print(f"[{'cow':^7}]");
print(f"{42:05}");
```

Output:

```text
3.50
[    3.50]
[  cow  ]
00042
```

A field without a spec formats like `str`. A non-empty spec with an unknown form is an error, and `.Nf` requires a number. Conversions and nested fields inside the spec are not supported.

## Index and Iterate

```tg
const text = "Tiger";
print(len(text), text.size(), text.length(), text[0], text[-1]);
var reversed = "";
for character in text {
    reversed = character + reversed;
}
print(reversed);
print("ger" in text, "GER" in text);
```

Output:

```text
5 5 5 T r
regiT
true false
```

Indexing starts at zero. Negative indices count backward from the end; `-1` is the last character. `len(text)`, `text.size()`, `text.length()`, indexing, and iteration operate on Unicode code points, not UTF-8 bytes or user-perceived grapheme clusters. A combining accent can therefore count separately from the letter it follows; use [`graphemes`](#grapheme-clusters) to group code points into approximate characters.

The index must be an integer-valued number and must be in range. String membership is case-sensitive substring matching, and ordering is lexicographic.

## Slicing

`text[start:stop:step]` returns a new string counted in Unicode code points. Every part may be omitted, negative bounds count from the end, and out-of-range bounds clamp.

```tg
const text = "tiger";
print(text[1:4], text[:2], text[2:]);
print(text[::-1], text[::2]);
```

Output:

```text
ige ti ger
regit tgr
```

Slicing never changes the original string, and it is a runtime error to use a zero step, a non-integer bound, or a slice on a value that is not a list or string. See [Lists and Dictionaries](collections.md#slicing) for the list form.

## Grapheme Clusters

`graphemes(text)` groups an approximate user-perceived character: a base character together with its combining marks, variation selectors, emoji modifiers, zero-width-joiner continuations, and paired flag indicators. It complements the code-point based `len`, indexing, and iteration.

```tg
const flag = "\U0001F1FA\U0001F1F8";
print(len(flag), len(graphemes(flag)));
const accented = "e\u0301";
print(len(accented), len(graphemes(accented)));
```

Output:

```text
2 1
2 1
```

The grouping approximates Unicode UAX #29. It handles combining accents, skin-tone modifiers, flags, and joined emoji sequences without shipping the full Unicode tables, so an unusual script may still split differently from a fully conformant implementation. `graphemes("")` is the empty list.

## String Methods

Strings provide methods for common text operations, so most helpers no longer need hand-written loops. Each method is read from the string and called; the original string is never changed.

```tg
print("a,b,c".split(","));
print("  hello  ".trim().upper());
print("Tiger".lower(), "Tiger".startswith("Ti"), "Tiger".endswith("er"));
print("Tiger".find("g"), "Tiger".find("z"));
print("a-b-a".replace("a", "o"));
print("7".padStart(3, "0"), "ab".padEnd(4, "."));
print(["alpha", "beta"].join(", "));
```

```text
["a", "b", "c"]
HELLO
tiger true true
2 -1
o-b-o
007 ab..
alpha, beta
```

| Method | Result |
| --- | --- |
| `text.split()` | Splits on runs of whitespace and drops empty fields |
| `text.split(separator)` | Splits on the exact `separator` and keeps empty fields; an empty separator is an error |
| `text.upper()`, `text.lower()` | Case conversion following Unicode rules |
| `text.trim()` | Removes leading and trailing whitespace |
| `text.replace(old, new)` | Replaces every occurrence of `old` |
| `text.startswith(prefix)`, `text.endswith(suffix)` | Boolean prefix or suffix test |
| `text.find(substring)` | Code-point index of the first occurrence, or `-1` |
| `text.padStart(width, fill)`, `text.padEnd(width, fill)` | Pads to `width` code points; `fill` defaults to a space and must be a single character |

`split` returns a list of strings, and a list of strings joins back into one string with the list method `items.join(separator)`. Methods compose with the rest of the language: a method value can be stored (`const shout = "tiger".upper;`), and `find` reports a code-point index just like string indexing. Widths and fills are validated when the method runs, and no string method accepts keyword arguments.

**Expected error:**

```tg
print("abc".split(""));
```

```error
split: empty separator
```

## Strings Are Immutable

**Expected error:**

```tg
const text = "cat";
text[0] = "b";
```

```error
cannot assign an index of string
```

Create a new string instead; the [string methods](#string-methods) return new strings rather than changing the original. The [text-processing benchmark](../benchmarks/05_strings.tg) demonstrates tokenization, frequency counts, and a Caesar cipher.

## Try It

Build a function that joins a list of strings with `", "`. Track whether you are adding the first element so that you do not prepend a separator. For an empty list, return `""`.