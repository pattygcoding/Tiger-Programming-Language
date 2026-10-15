export const keywords = new Set([
  "const", "var", "function", "class", "extends", "super", "return", "if", "elif", "else",
  "while", "do", "for", "in", "and", "or", "not", "div",
  "break", "continue", "switch", "match", "case", "default",
  "this", "public", "private", "protected", "try", "catch", "finally", "throw",
  "import", "as",
]);
export const literals = new Set(["true", "false", "null"]);
export const builtins = new Set(["print", "input", "str", "int", "float", "bool", "len", "range", "type", "isinstance", "graphemes", "sum", "min", "max", "any", "all", "zip", "enumerate", "map", "filter", "sorted", "list", "tuple", "set", "open", "read_file", "write_file", "append_file", "file_exists", "remove_file", "list_dir", "math", "algo", "random", "secrets", "json", "time", "regex"]);
export const moduleMembers = {
  math: new Set(["ceil", "floor", "round", "sqrt", "abs", "pow", "log", "sin", "cos", "tan", "min", "max"]),
  random: new Set(["seed", "random", "int", "uniform", "choice", "shuffle"]),
  secrets: new Set(["random", "int", "uniform", "choice", "shuffle"]),
  json: new Set(["parse", "stringify"]),
  time: new Set(["now", "now_ms", "sleep", "iso", "parts"]),
  regex: new Set(["match", "find", "findAll", "groups", "replace", "split"]),
  algo: new Set([
    "fibonacci", "fibonacciList", "factorial", "factorialList", "removeDuplicates", "findMatches",
    "takeInventory", "findPlace", "isPrime", "rainwater", "medianSorted", "mergeKSorted",
    "editDistance", "regexMatch", "slidingWindowMax", "slidingWindowSumMax", "slidingWindowMinLen",
    "slidingWindowLongestUnique", "slidingWindowMinSubstring", "palindromeValid", "palindromeCanBeValid",
    "palindromeLongest", "palindromeCount", "palindromeMinCuts",
  ]),
};
const tokenPattern = /\/\/[^\n]*|\/\*[\s\S]*?(?:\*\/|$)|"(?:\\[^\r\n]|[^"\\\r\n])*\\?"?|'(?:\\[^\r\n]|[^'\\\r\n])*\\?'?|[0-9]+(?:\.[0-9]+)?(?:[eE][+-]?[0-9]*)?|[\p{L}_][\p{L}\p{Nd}_]*|\+\+|--|\*\*|<<|>>|&&|\|\||[=!<>+*%\-]=|[=<>+*/%&^|~\-]|[()[\]{}:;,.]|\s+|[^]/gu;

export function tokenize(source) {
  const result = [];
  let position = 0;
  const escapes = new Set(["n", "r", "t", "x", "u", "U", "\\", '"', "'"]);

  function stringLiteral() {
    const quote = source[position++];
    let start = position - 1;
    while (position < source.length && source[position] !== quote) {
      if (source[position] === "\\" && position + 1 < source.length && escapes.has(source[position + 1])) {
        if (position > start) result.push({ text: source.slice(start, position), kind: "string" });
        result.push({ text: source.slice(position, position + 2), kind: "escape" });
        position += 2;
        start = position;
      } else if (source[position] === "\n" || source[position] === "\r") {
        break;
      } else {
        position++;
      }
    }
    if (position < source.length && source[position] === quote) position++;
    if (position > start) result.push({ text: source.slice(start, position), kind: "string" });
  }

  // True for `math.sqrt` / `algo.isPrime`, allowing whitespace around the dot.
  function isModuleMember(name) {
    const previous = result.filter(({ text }, index) => index >= result.length - 4 && !/^\s+$/.test(text));
    const [module, dot] = previous.slice(-2);
    return dot?.text === "." && module?.kind === "builtin" && moduleMembers[module.text]?.has(name) === true;
  }

  // Raw strings keep backslashes literal, so no escape tokens are produced.
  function rawString() {
    const start = position;
    position += 2; // r/R and the opening quote
    const quote = source[position - 1];
    while (position < source.length && source[position] !== quote) {
      if (source[position] === "\n" || source[position] === "\r") break;
      position++;
    }
    if (position < source.length && source[position] === quote) position++;
    result.push({ text: source.slice(start, position), kind: "string" });
  }

  function consume(depth = 0) {
    if (depth < 512 && /^[fF]["']/.test(source.slice(position, position + 2))) {
      formatted(depth + 1);
      return;
    }
    if (/^[rR]["']/.test(source.slice(position, position + 2))) {
      rawString();
      return;
    }
    if (source[position] === '"' || source[position] === "'") {
      stringLiteral();
      return;
    }
    tokenPattern.lastIndex = position;
    const text = tokenPattern.exec(source)[0];
    position += text.length;
    let kind = "plain";
    if (text.startsWith("//") || text.startsWith("/*")) kind = "comment";
    else if (text[0] === '"' || text[0] === "'") kind = "string";
    else if (/^[0-9]/.test(text)) kind = "number";
    // Before the keyword set: a keyword is a member name after `.`, so
    // `regex.match` is the module member rather than the `match` keyword.
    else if (isModuleMember(text)) kind = "builtin";
    else if (keywords.has(text)) kind = "keyword";
    else if (literals.has(text)) kind = "literal";
    else if (builtins.has(text)) kind = "builtin";
    else if (/^[\p{L}_]/u.test(text)) kind = "identifier";
    else if (/^(?:[=<>!+*/%&^|~\-]|&&|\|\|)/.test(text)) kind = "operator";
    else if (/^[()[\]{}:;,.]/.test(text)) kind = "punctuation";
    result.push({ text, kind });
  }

  function formatted(depth) {
    const quote = source[position + 1];
    let start = position;
    position += 2;
    function flush() {
      if (position > start) result.push({ text: source.slice(start, position), kind: "string" });
    }
    while (position < source.length) {
      const char = source[position];
      if (char === "\n" || char === "\r") break;
      if (char === "\\") {
        if (position + 1 < source.length && escapes.has(source[position + 1])) {
          if (position > start) result.push({ text: source.slice(start, position), kind: "string" });
          result.push({ text: source.slice(position, position + 2), kind: "escape" });
          position += 2;
          start = position;
        } else {
          position++;
          if (position < source.length && !/[\r\n]/.test(source[position])) position++;
        }
      } else if (char === quote) {
        position++;
        break;
      } else if ((char === "{" || char === "}") && source[position + 1] === char) {
        position += 2;
      } else if (char === "{") {
        flush();
        result.push({ text: "{", kind: "punctuation" });
        position++;
        let braces = 0;
        while (position < source.length) {
          if (source[position] === "}" && braces === 0) {
            result.push({ text: "}", kind: "punctuation" });
            position++;
            break;
          }
          const char = source[position];
          if (char === "{") braces++;
          if (char === "}") braces--;
          consume(depth);
        }
        start = position;
      } else {
        position++;
      }
    }
    flush();
  }

  while (position < source.length) consume();
  return result;
}

export function attachHighlighting(source, mirror) {
  const code = mirror.querySelector("code");

  function syncScroll() {
    mirror.scrollTop = source.scrollTop;
    mirror.scrollLeft = source.scrollLeft;
  }

  function resize() {
    mirror.style.width = `${source.clientWidth}px`;
    mirror.style.height = `${source.clientHeight}px`;
    syncScroll();
  }

  function refresh() {
    const fragment = document.createDocumentFragment();
    for (const { text, kind } of tokenize(source.value)) {
      if (kind === "plain") {
        fragment.append(document.createTextNode(text));
      } else {
        const span = document.createElement("span");
        span.className = `token-${kind}`;
        span.textContent = text;
        fragment.append(span);
      }
    }
    fragment.append(document.createTextNode("\u200b"));
    code.replaceChildren(fragment);
    syncScroll();
  }

  source.addEventListener("input", refresh);
  source.addEventListener("scroll", syncScroll);
  const observer = new ResizeObserver(resize);
  observer.observe(source);
  refresh();
  resize();
  source.classList.add("highlighted");
  return refresh;
}