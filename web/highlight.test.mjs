import assert from "node:assert/strict";
import { test } from "node:test";
import { tokenize } from "./highlight.mjs";

function tokens(source) {
  return tokenize(source).filter(({ kind }) => kind !== "plain");
}

test("all reserved words, literals, and built-ins", () => {
  for (const text of "const var function class super this public private protected return if elif else while for cfor in and or not break continue switch case default try catch throw import as".split(" ")) {
    assert.deepEqual(tokens(text), [{ text, kind: "keyword" }]);
  }
  for (const text of ["true", "false", "null"]) {
    assert.deepEqual(tokens(text), [{ text, kind: "literal" }]);
  }
  for (const text of ["print", "str", "len", "range"]) {
    assert.deepEqual(tokens(text), [{ text, kind: "builtin" }]);
  }
});

test("identifiers include Unicode and do not invent type keywords", () => {
  for (const text of ["def", "number", "string", "bool", "int", "float", "self", "true_value", "printable", "caf\u00e9", "\u53d8\u91cf\u0661"]) {
    assert.deepEqual(tokens(text), [{ text, kind: "identifier" }]);
  }
});

test("decimal and exponent numbers, operators, and punctuation", () => {
  assert.deepEqual(tokens("12 3.5 1e2 2E-3 4e+" ).map(({ kind }) => kind), Array(5).fill("number"));
  for (const text of "= += -= *= %= == != < <= > >= + - * / % ++ --".split(" ")) {
    assert.deepEqual(tokens(text), [{ text, kind: "operator" }]);
  }
  for (const text of "()[]{}:;,.") {
    assert.deepEqual(tokens(text), [{ text, kind: "punctuation" }]);
  }
});

test("strings protect comment markers and handle escapes", () => {
  for (const text of ['"# // /* true */"', "'hello'"]) {
    assert.deepEqual(tokens(text), [{ text, kind: "string" }]);
  }
  assert.deepEqual(tokens(String.raw`"a\"b\\c\n\r\t"`), [
    { text: '"a', kind: "string" }, { text: '\\"', kind: "escape" },
    { text: "b", kind: "string" }, { text: "\\\\", kind: "escape" },
    { text: "c", kind: "string" }, { text: "\\n", kind: "escape" },
    { text: "\\r", kind: "escape" }, { text: "\\t", kind: "escape" },
    { text: '"', kind: "string" },
  ]);
  assert.deepEqual(tokens(String.raw`'it\'s'`), [
    { text: "'it", kind: "string" }, { text: "\\'", kind: "escape" }, { text: "s'", kind: "string" },
  ]);
  assert.deepEqual(tokens('// "hello"\n// true'), [
    { text: '// "hello"', kind: "comment" },
    { text: "// true", kind: "comment" },
  ]);
});

test("unfinished strings recover on the next line", () => {
  for (const text of ['"unfinished', "'unfinished", '"escape\\']) {
    assert.deepEqual(tokens(`${text}\nconst`), [
      { text, kind: "string" }, { text: "const", kind: "keyword" },
    ]);
  }
});

test("tokenization preserves all source, including incomplete or HTML-like input", () => {
  for (const source of ["", "\n\n", "\tconst x = 12;  \n", '<img src=x onerror="alert(1)">', "\ud83d\udc05 ! & | ` \\ \r\n", "/* a block comment */"]) {
    assert.equal(tokenize(source).map(({ text }) => text).join(""), source);
    assert.deepEqual(tokenize(source), tokenize(source));
  }
});

test("block comments span lines and tolerate unfinished input", () => {
  for (const text of ["/**/", "/* first\nfunction var # // second */", "/* unfinished\ncomment"]) {
    assert.deepEqual(tokens(text), [{ text, kind: "comment" }]);
  }
  assert.deepEqual(tokens("/* done */var"), [
    { text: "/* done */", kind: "comment" },
    { text: "var", kind: "keyword" },
  ]);
  assert.deepEqual(tokens("# old"), [{ text: "old", kind: "identifier" }]);
});

test("formatted strings highlight embedded expressions and preserve nested source", () => {
  assert.deepEqual(tokens('f"Hello {name}, {1 + 2}!"'), [
    { text: 'f"Hello ', kind: "string" },
    { text: "{", kind: "punctuation" },
    { text: "name", kind: "identifier" },
    { text: "}", kind: "punctuation" },
    { text: ", ", kind: "string" },
    { text: "{", kind: "punctuation" },
    { text: "1", kind: "number" },
    { text: "+", kind: "operator" },
    { text: "2", kind: "number" },
    { text: "}", kind: "punctuation" },
    { text: '!"', kind: "string" },
  ]);
  for (const source of ['F"{{literal}}"', 'f"{data["key"]}"', `f"{f'{range(3)}'}"`, 'f"{ {"key": 1}["key"] }"', 'f"{1 /* } */ + 2}"', 'f"unfinished {', 'f"text\\', 'f"text\nconst']) {
    assert.equal(tokenize(source).map(({ text }) => text).join(""), source);
    assert.deepEqual(tokenize(source), tokenize(source));
  }
  assert.deepEqual(tokens('F"{{literal}}"'), [{text:'F"{{literal}}"', kind:"string"}]);
});