importScripts("wasm_exec.js");

self.tigerResolveModule = (importer, requested) => {
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
};

self.tigerLoadModule = (filename) => {
  try {
    const resolved = self.tigerResolveModule("", filename);
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
};

async function initialize() {
  const go = new Go();
  const response = await fetch("tiger.wasm");
  if (!response.ok) throw new Error(`Runtime download failed (${response.status})`);
  const result = await WebAssembly.instantiate(await response.arrayBuffer(), go.importObject);
  go.run(result.instance).catch((error) => {
    self.postMessage({ type: "fatal", error: error.message });
  });
  if (typeof self.tigerRun !== "function") throw new Error("Runtime did not initialize");
  self.postMessage({ type: "ready" });
}

self.onmessage = (event) => {
  if (event.data.type !== "run") return;
  const started = performance.now();
  try {
    const result = self.tigerRun(event.data.source, event.data.filename || "examples/playground.tg");
    self.postMessage({ type: "result", ...result, duration: performance.now() - started });
  } catch (error) {
    self.postMessage({ type: "fatal", error: error.message });
  }
};

initialize().catch((error) => self.postMessage({ type: "fatal", error: error.message }));