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

Single and double quotes have the same meaning. `+` joins strings, but it will not automatically turn a number into text. `print` accepts separate values of different types; concatenation requires explicit conversion with `str`.

## Escape Reference

| Escape | Meaning |
| --- | --- |
| `\n` | Newline |
| `\r` | Carriage return |
| `\t` | Tab |
| `\\` | Backslash |
| `\"` | Double quote |
| `\'` | Single quote |

Unknown escapes are lexical errors. There are no raw strings, interpolation, triple quotes, multiline literals, or `\u`/`\x` escapes. Literal Unicode text is supported; use an actual Unicode character rather than an escape for its code point.

## Index and Iterate

```tg
const text = "Tiger";
print(len(text), text[0], text[-1]);
reversed = "";
for character in text {
    reversed = character + reversed;
}
print(reversed);
print("ger" in text, "GER" in text);
```

Output:

```text
5 T r
regiT
true false
```

Indexing starts at zero. Negative indices count backward from the end; `-1` is the last character. `len`, indexing, and iteration operate on Unicode code points, not UTF-8 bytes or user-perceived grapheme clusters. A combining accent can therefore count separately from the letter it follows.

The index must be an integer-valued number and must be in range. Slicing is not implemented. String membership is case-sensitive substring matching, and ordering is lexicographic.

## Strings Are Immutable

**Expected error:**

```tg
text = "cat";
text[0] = "b";
```

```error
cannot assign an index of string
```

Create a new string instead. There are no built-in string methods such as `split`, `replace`, or `upper`; use loops and functions for these operations. The [text-processing benchmark](../benchmarks/05_strings.tg) demonstrates tokenization, frequency counts, and a Caesar cipher.

## Try It

Build a function that joins a list of strings with `", "`. Track whether you are adding the first element so that you do not prepend a separator. For an empty list, return `""`.