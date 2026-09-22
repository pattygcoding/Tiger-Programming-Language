const keywords = new Set([
  "const", "var", "function", "class", "super", "return", "if", "elif", "else",
  "while", "for", "in", "and", "or", "not",
  "cfor", "break", "continue", "switch", "case", "default",
  "this", "public", "private", "protected", "try", "catch", "throw",
]);
const literals = new Set(["true", "false", "null"]);
const builtins = new Set(["print", "str", "len", "range"]);
const tokenPattern = /\/\/[^\n]*|\/\*[\s\S]*?(?:\*\/|$)|"(?:\\[^\r\n]|[^"\\\r\n])*\\?"?|'(?:\\[^\r\n]|[^'\\\r\n])*\\?'?|[0-9]+(?:\.[0-9]+)?(?:[eE][+-]?[0-9]*)?|[\p{L}_][\p{L}\p{Nd}_]*|[=!<>]=|\+\+|--|[=<>+*/%\-]|[()[\]{}:;,.]|\s+|[^]/gu;

export function tokenize(source) {
  const result = [];
  let position = 0;

  function consume(depth = 0) {
    if (depth < 512 && /^[fF]["']/.test(source.slice(position, position + 2))) {
      formatted(depth + 1);
      return;
    }
    tokenPattern.lastIndex = position;
    const text = tokenPattern.exec(source)[0];
    position += text.length;
    let kind = "plain";
    if (text.startsWith("//") || text.startsWith("/*")) kind = "comment";
    else if (text[0] === '"' || text[0] === "'") kind = "string";
    else if (/^[0-9]/.test(text)) kind = "number";
    else if (keywords.has(text)) kind = "keyword";
    else if (literals.has(text)) kind = "literal";
    else if (builtins.has(text)) kind = "builtin";
    else if (/^[\p{L}_]/u.test(text)) kind = "identifier";
    else if (/^[=<>!+*/%\-]/.test(text)) kind = "operator";
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
        position++;
        if (position < source.length && !/[\r\n]/.test(source[position])) position++;
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