importScripts("wasm_exec.js");

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
    const result = self.tigerRun(event.data.source);
    self.postMessage({ type: "result", ...result, duration: performance.now() - started });
  } catch (error) {
    self.postMessage({ type: "fatal", error: error.message });
  }
};

initialize().catch((error) => self.postMessage({ type: "fatal", error: error.message }));