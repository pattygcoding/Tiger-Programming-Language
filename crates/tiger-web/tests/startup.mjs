// Boots the playground the way a browser does: starts the server, fetches the
// page and the runtime assets over HTTP, instantiates the served Wasm, runs a
// program, answers `input()`, and checks what comes back.
//
//   cargo build -p tiger-web --release
//   node crates/tiger-web/tests/startup.mjs
//
// The server builds the browser runtime itself, so this needs no other step.

import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync } from "node:fs";
import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..", "..");
const port = Number(process.env.TIGER_STARTUP_PORT ?? 7199);
const base = `http://127.0.0.1:${port}`;

const binary = ["target/release/tiger-web", "target/debug/tiger-web"]
  .map((path) => resolve(root, path) + (process.platform === "win32" ? ".exe" : ""))
  .find((path) => existsSync(path));
if (!binary) {
  console.error("startup: build the server first (cargo build -p tiger-web --release)");
  process.exit(2);
}

const server = spawn(binary, ["-addr", `127.0.0.1:${port}`], { cwd: root, stdio: ["ignore", "pipe", "pipe"] });
let serverOutput = "";
server.stdout.on("data", (chunk) => (serverOutput += chunk));
server.stderr.on("data", (chunk) => (serverOutput += chunk));

const failures = [];
const note = (message) => {
  failures.push(message);
  console.error(`FAIL ${message}`);
};

try {
  const page = await waitForServer();
  if (!page.includes("app.js")) note("the served page does not load app.js");

  // The served assets must be the ones in the tree.
  for (const asset of ["/app.js", "/tiger.js", "/worker.js", "/highlight.mjs"]) {
    const response = await fetch(base + asset);
    const served = await response.text();
    const onDisk = await readFile(resolve(root, "web", asset.slice(1)), "utf8");
    if (served !== onDisk) note(`${asset} was served differently from disk`);
  }

  // The served module must be the artifact the runtime tests cover.
  const artifact = await fetch(`${base}/tiger.wasm`);
  const served = Buffer.from(await artifact.arrayBuffer());
  const built = await readFile(resolve(root, "web", "tiger.wasm"));
  if (createHash("sha256").update(served).digest("hex") !== createHash("sha256").update(built).digest("hex")) {
    note("/tiger.wasm differs from web/tiger.wasm");
  }
  if (artifact.headers.get("content-type") !== "application/wasm") note("tiger.wasm is served with the wrong content type");
  if (artifact.headers.get("cross-origin-embedder-policy") !== "require-corp") {
    note("the server does not send the isolation headers input() needs");
  }
  console.log(`ok   served tiger.wasm: ${served.length} bytes`);

  // Boot it over HTTP and run a program that streams output, reads input, and
  // reports a module the browser omits.
  const program = resolve(root, "target", "startup-check.tg");
  const lines = resolve(root, "target", "startup-check.txt");
  await writeProgram(program, lines);
  const harness = resolve(root, "crates", "tiger-wasm", "tests", "runtime.mjs");
  const run = spawn(process.execPath, [harness, resolve(root, "web", "tiger.wasm"), program, lines], {
    cwd: root,
    env: { ...process.env, TIGER_WASM_URL: `${base}/tiger.wasm` },
    stdio: ["ignore", "pipe", "pipe"],
  });
  let output = "";
  run.stdout.on("data", (chunk) => (output += chunk));
  run.stderr.on("data", (chunk) => (output += chunk));
  const code = await new Promise((done) => run.on("close", done));
  const expected = "name: hi Ada!\n5 4\nregex unavailable\n";
  if (code !== 0 || output !== expected) note(`startup program printed ${JSON.stringify(output)} (exit ${code})`);
  else console.log("ok   ran a program on the served runtime, including input()");
} finally {
  server.kill();
}

if (failures.length > 0) {
  console.error(`startup: ${failures.length} check(s) failed\n--- server output ---\n${serverOutput}`);
  process.exit(1);
}
console.log("startup: the playground boots and runs a program");

async function waitForServer() {
  for (let attempt = 0; attempt < 120; attempt++) {
    try {
      const response = await fetch(base + "/");
      if (response.ok) return await response.text();
    } catch {
      // not listening yet
    }
    await new Promise((done) => setTimeout(done, 500));
  }
  throw new Error(`the server did not answer on ${base}\n${serverOutput}`);
}

async function writeProgram(program, lines) {
  const { writeFile } = await import("node:fs/promises");
  await writeFile(
    program,
    'const name = input("name: ");\n' +
      'print("hi " + name + "!");\n' +
      'print(len(graphemes("héllo")), math.sqrt(16));\n' +
      'try { regex.match("a", "a"); } catch (error) { print("regex unavailable"); }\n',
  );
  await writeFile(lines, "Ada\n");
}
