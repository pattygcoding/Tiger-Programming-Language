// The worker half of the browser harness: it mirrors web/worker.js, so the
// runtime is exercised the way the playground drives it.
//
//   node crates/tiger-wasm/tests/runtime.mjs <tiger_wasm.wasm> <program.tg> [input.txt]

import { readFileSync } from "node:fs";
import { dirname, isAbsolute, resolve } from "node:path";
import { parentPort, workerData } from "node:worker_threads";

import { createRuntime } from "../../../web/tiger.js";

const runtime = await createRuntime({
  url: workerData.wasmUrl,
  output: (text) => parentPort.postMessage({ type: "output", text }),
  requestInput: () => parentPort.postMessage({ type: "input" }),
  resolveModule: (importer, requested) => {
    try {
      if (!requested.endsWith(".tg") || requested.includes("\\")) {
        throw new Error("Imports require .tg paths with forward slashes");
      }
      const base = importer ? dirname(importer) : workerData.base;
      return { path: isAbsolute(requested) ? requested : resolve(base, requested), error: "" };
    } catch (error) {
      return { path: "", error: error.message };
    }
  },
  loadModule: (filename) => {
    try {
      return { source: readFileSync(filename, "utf8"), error: "" };
    } catch (error) {
      return { source: "", error: error.message };
    }
  },
});

parentPort.postMessage({ type: "ready", shared: runtime.shared, input: runtime.input });

parentPort.on("message", (event) => {
  if (event.type !== "run") return;
  const started = Date.now();
  try {
    const result = runtime.run(event.source, event.filename);
    parentPort.postMessage({ type: "result", error: result.error, duration: Date.now() - started });
  } catch (error) {
    parentPort.postMessage({ type: "fatal", error: error.message });
  }
});
