// Runs a Tiger program on the browser runtime and prints what the playground
// would show, so the browser path can be compared with the native CLI:
//
//   node crates/tiger-wasm/tests/runtime.mjs <tiger_wasm.wasm> <program.tg> [input.txt]
//
// The module is served over HTTP because the runtime fetches it, exactly as a
// page does, and input is answered from the main thread through the shared
// buffer, exactly as web/app.js does.

import { createServer } from "node:http";
import { readFileSync } from "node:fs";
import { basename, dirname } from "node:path";
import { Worker } from "node:worker_threads";

import { createInputChannel } from "../../../web/tiger.js";

const [wasmPath, programPath, inputPath] = process.argv.slice(2);
if (!wasmPath || !programPath) {
  console.error("usage: runtime.mjs <tiger_wasm.wasm> <program.tg> [input.txt]");
  process.exit(2);
}

const wasm = readFileSync(wasmPath);
const source = readFileSync(programPath, "utf8");
// A trailing newline ends the last line rather than starting an empty one, the
// way reading the file line by line would.
const input = inputPath ? readFileSync(inputPath, "utf8").replace(/\r\n/g, "\n") : "";
const lines = input === "" ? [] : (input.endsWith("\n") ? input.slice(0, -1) : input).split("\n");
let nextLine = 0;

// `TIGER_WASM_URL` points the runtime at an already-running server, which is how
// the startup check boots the playground; otherwise the module is served here.
const served = process.env.TIGER_WASM_URL;
let server = null;
let wasmUrl = served;
if (!served) {
  server = createServer((request, response) => {
    if (request.url !== "/tiger.wasm") {
      response.writeHead(404).end();
      return;
    }
    response.writeHead(200, { "Content-Type": "application/wasm" }).end(wasm);
  });
  await new Promise((listening) => server.listen(0, "127.0.0.1", listening));
  wasmUrl = `http://127.0.0.1:${server.address().port}/tiger.wasm`;
}

const worker = new Worker(new URL("worker.mjs", import.meta.url), {
  workerData: { wasmUrl, base: dirname(programPath) },
});

let channel = null;

worker.on("message", (event) => {
  if (event.type === "output") {
    process.stdout.write(event.text);
    return;
  }
  if (event.type === "ready") {
    channel = createInputChannel(event.shared);
    if (!event.input) console.error("harness: the runtime reported no input channel");
    worker.postMessage({ type: "run", source, filename: basename(programPath) });
    return;
  }
  if (event.type === "input") {
    // The worker is blocked in Atomics.wait, so answer through the buffer.
    if (nextLine < lines.length) channel.write(lines[nextLine++]);
    else channel.end();
    return;
  }
  if (event.type === "result") {
    if (event.error) process.stderr.write(`${event.error}\n`);
    finish(event.error ? 1 : 0);
    return;
  }
  if (event.type === "fatal") {
    process.stderr.write(`${event.error}\n`);
    finish(70);
  }
});

worker.on("error", (error) => {
  process.stderr.write(`harness: ${error.message}\n`);
  finish(70);
});

function finish(code) {
  server?.close();
  worker.terminate().then(() => process.exit(code));
}
