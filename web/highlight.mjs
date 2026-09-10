const keywords = new Set([
  "const", "def", "class", "super", "return", "if", "elif", "else",
  "while", "for", "in", "and", "or", "not",
]);
const literals = new Set(["true", "false", "null"]);
const builtins = new Set(["print", "str", "len"]);
const tokenPattern = /#[^\n]*|\/\/[^\n]*|"(?:\\[^\r\n]|[^"\\\r\n])*\\?"?|'(?:\\[^\r\n]|[^'\\\r\n])*\\?'?|[0-9]+(?:\.[0-9]+)?(?:[eE][+-]?[0-9]*)?|[\p{L}_][\p{L}\p{Nd}_]*|[=!<>]=|[=<>+*/%\-]|[()[\]{}:;,.]|\s+|[^]/gu;

export function tokenize(source) {
  return Array.from(source.matchAll(tokenPattern), ([text]) => {
    let kind = "plain";
    if (text.startsWith("#") || text.startsWith("//")) kind = "comment";
    else if (text[0] === '"' || text[0] === "'") kind = "string";
    else if (/^[0-9]/.test(text)) kind = "number";
    else if (keywords.has(text)) kind = "keyword";
    else if (literals.has(text)) kind = "literal";
    else if (builtins.has(text)) kind = "builtin";
    else if (/^[\p{L}_]/u.test(text)) kind = "identifier";
    else if (/^[=<>!+*/%\-]/.test(text)) kind = "operator";
    else if (/^[()[\]{}:;,.]/.test(text)) kind = "punctuation";
    return { text, kind };
  });
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