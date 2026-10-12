// The playground's worker: it loads the runtime and runs one program per message.
//
// The page answers `input()` by writing into the shared buffer directly, because
// a worker blocked in `Atomics.wait` cannot receive messages. See web/tiger.js.
import { createRuntime } from "./tiger.js";

function resolveModule(importer, requested) {
  try {
    const base = new URL(importer || "examples/playground.tg", self.location.href);
    const url = new URL(requested, importer ? base : self.location.href);
    if (url.origin !== self.location.origin || url.username || url.password || url.search || url.hash || !url.pathname.endsWith(".tg")) {
      throw new Error("Imports require same-origin .tg paths without query strings or fragments");
    }
    return { path: url.href, error: "" };
  } catch (error) {
    return { path: "", error: error.message };
  }
}

function loadModule(filename) {
  try {
    const resolved = resolveModule("", filename);
    if (resolved.error) throw new Error(resolved.error);
    const request = new XMLHttpRequest();
    request.open("GET", resolved.path, false);
    request.send();
    if (request.status !== 200) throw new Error(`Module download failed (${request.status}): ${resolved.path}`);
    if (request.responseURL && new URL(request.responseURL).origin !== self.location.origin) {
      throw new Error("Cross-origin module redirects are not allowed");
    }
    return { source: request.responseText, error: "" };
  } catch (error) {
    return { source: "", error: error.message };
  }
}

let runtime = null;

async function initialize() {
  runtime = await createRuntime({
    url: "tiger.wasm",
    output: (text) => self.postMessage({ type: "output", text }),
    requestInput: () => self.postMessage({ type: "input" }),
    resolveModule,
    loadModule,
  });
  // `shared` lets the page answer input; `input` is false when the page is not
  // cross-origin isolated, in which case `input()` reports no stream.
  self.postMessage({ type: "ready", shared: runtime.shared, input: runtime.input });
}

self.onmessage = (event) => {
  if (event.data.type !== "run") return;
  const started = performance.now();
  try {
    const result = runtime.run(event.data.source, event.data.filename || "examples/playground.tg");
    self.postMessage({ type: "result", error: result.error, duration: performance.now() - started });
  } catch (error) {
    self.postMessage({ type: "fatal", error: error.message });
  }
};

initialize().catch((error) => self.postMessage({ type: "fatal", error: error.message }));
