import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { builtins, keywords, literals } from "../../web/highlight.mjs";

const grammar = JSON.parse(readFileSync(new URL("./syntaxes/tiger.tmLanguage.json", import.meta.url), "utf8"));
const rules = Object.values(grammar.repository).flatMap((rule) => [rule, ...(rule.patterns ?? [])]);

function scopeOf(word) {
  const matching = ["module-members", "keywords", "literals", "builtins"]
    .flatMap((key) => grammar.repository[key].patterns ?? [grammar.repository[key]])
    .filter(({ match }) => new RegExp(`^(?:${match})$`, "u").test(word));
  return matching.map(({ name }) => name);
}

test("every pattern compiles", () => {
  for (const rule of rules) {
    for (const key of ["match", "begin", "end"]) {
      if (rule[key]) assert.doesNotThrow(() => new RegExp(rule[key], "u"), rule[key]);
    }
  }
});

test("keywords, literals, and built-ins match the playground", () => {
  for (const word of keywords) assert.equal(scopeOf(word).length, 1, word);
  for (const word of literals) assert.deepEqual(scopeOf(word), ["constant.language.tiger"]);
  for (const word of builtins) assert.match(scopeOf(word)[0], /^support\./, word);
  for (const word of ["self", "def", "init", "number", "printable", "café"]) assert.deepEqual(scopeOf(word), [], word);
});

test("language contribution points at the grammar and configuration", () => {
  const manifest = JSON.parse(readFileSync(new URL("./package.json", import.meta.url), "utf8"));
  assert.deepEqual(manifest.contributes.languages[0].extensions, [".tg"]);
  for (const { path } of manifest.contributes.grammars) {
    assert.doesNotThrow(() => JSON.parse(readFileSync(new URL(path, import.meta.url), "utf8")), path);
  }
  JSON.parse(readFileSync(new URL(manifest.contributes.languages[0].configuration, import.meta.url), "utf8"));
});
