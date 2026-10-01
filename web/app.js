import { attachHighlighting } from "./highlight.mjs";

const source = document.querySelector("#source");
const output = document.querySelector("#output");
const errorOutput = document.querySelector("#error");
const status = document.querySelector("#status");
const runButton = document.querySelector("#run");
const stopButton = document.querySelector("#stop");
const example = document.querySelector("#example");
const timing = document.querySelector("#timing");
let worker;
let ready = false;
let running = false;
let loadVersion = 0;
let sourceFilename = "examples/demo.tg";
let pendingInput = null;

try {
  source.value = localStorage.getItem("tiger-source") ?? source.value;
  sourceFilename = localStorage.getItem("tiger-source-path") ?? sourceFilename;
} catch {}

const refreshHighlighting = attachHighlighting(source, document.querySelector("#source-highlight"));

function showError(message) {
  errorOutput.textContent = message;
  errorOutput.hidden = !message;
}

function setState(label, isReady, isRunning = false) {
  status.textContent = label;
  ready = isReady;
  running = isRunning;
  runButton.disabled = !ready || running;
  stopButton.disabled = !running;
}

function appendOutput(text) {
  output.insertBefore(document.createTextNode(text), pendingInput);
  output.scrollTop = output.scrollHeight;
}

function closeInput() {
  pendingInput?.remove();
  pendingInput = null;
}

// Shows an inline field after the prompt; Enter sends the line and Ctrl+D ends input.
function requestInput(activeWorker) {
  closeInput();
  const field = document.createElement("input");
  field.className = "stdin";
  field.setAttribute("aria-label", "Program input");
  field.autocomplete = "off";
  field.spellcheck = false;
  field.addEventListener("keydown", (event) => {
    const endOfInput = event.ctrlKey && (event.key === "d" || event.key === "D");
    if (event.key !== "Enter" && !endOfInput) return;
    event.preventDefault();
    const text = endOfInput ? null : field.value;
    closeInput();
    appendOutput(endOfInput ? "^D\n" : `${text}\n`);
    setState("Running", true, true);
    activeWorker.postMessage({ type: "input", text });
  });
  pendingInput = field;
  output.append(field);
  output.scrollTop = output.scrollHeight;
  field.focus();
  setState("Waiting for input", true, true);
}

function startWorker(autoRun = false) {
  worker?.terminate();
  closeInput();
  setState("Loading runtime", false);
  const activeWorker = new Worker("worker.js");
  worker = activeWorker;
  activeWorker.onmessage = ({ data }) => {
    if (worker !== activeWorker) return;
    if (data.type === "ready") {
      setState("Ready", true);
      if (autoRun) run();
    } else if (data.type === "output") {
      appendOutput(data.text);
    } else if (data.type === "input") {
      requestInput(activeWorker);
    } else if (data.type === "result") {
      closeInput();
      showError(data.error);
      timing.textContent = `${data.duration.toFixed(1)} ms`;
      setState(data.error ? "Error" : "Finished", true);
    } else if (data.type === "fatal") {
      showError(data.error);
      setState("Runtime unavailable", false);
      activeWorker.terminate();
    }
  };
  activeWorker.onerror = (event) => {
    if (worker !== activeWorker) return;
    showError(event.message || "Unable to load the runtime");
    setState("Runtime unavailable", false);
    activeWorker.terminate();
  };
}

function remember() {
  try {
    localStorage.setItem("tiger-source", source.value);
    localStorage.setItem("tiger-source-path", sourceFilename);
  } catch {}
}

function position() {
  const lines = source.value.slice(0, source.selectionStart).split("\n");
  document.querySelector("#position").textContent = `Ln ${lines.length}, Col ${lines.at(-1).length + 1}`;
}

function run() {
  if (!ready || running) return;
  remember();
  output.textContent = "";
  showError("");
  timing.textContent = "";
  setState("Running", true, true);
  worker.postMessage({ type: "run", source: source.value, filename: sourceFilename });
}

async function loadExample() {
  const version = ++loadVersion;
  const directory = example.selectedOptions[0]?.dataset.dir || "examples";
  const filename = `${directory}/${example.value}.tg`;
  try {
    const response = await fetch(filename);
    if (!response.ok) throw new Error("Could not load example");
    const text = await response.text();
    if (version !== loadVersion) return;
    source.value = text;
    sourceFilename = filename;
    refreshHighlighting();
    remember();
    position();
    showError("");
  } catch (error) {
    if (version === loadVersion) showError(error.message);
  }
}

source.addEventListener("input", () => { loadVersion++; remember(); position(); });
source.addEventListener("click", position);
source.addEventListener("keyup", position);
source.addEventListener("keydown", (event) => {
  if (event.key === "Tab") {
    event.preventDefault();
    source.setRangeText("    ", source.selectionStart, source.selectionEnd, "end");
    refreshHighlighting();
    loadVersion++;
    remember();
    position();
  }
  if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
    event.preventDefault();
    run();
  }
});
runButton.addEventListener("click", run);
output.addEventListener("click", () => pendingInput?.focus());
stopButton.addEventListener("click", () => {
  startWorker();
  showError("Execution stopped.");
});
document.querySelector("#clear").addEventListener("click", () => {
  output.textContent = "";
  if (pendingInput) {
    output.append(pendingInput);
    pendingInput.focus();
  }
  showError("");
  timing.textContent = "";
});
document.querySelector("#reset").addEventListener("click", loadExample);
example.addEventListener("change", loadExample);
document.querySelector("#download").addEventListener("click", () => {
  const url = URL.createObjectURL(new Blob([source.value], { type: "text/plain" }));
  const link = document.createElement("a");
  link.href = url;
  link.download = `${example.value}.tg`;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
});
position();
startWorker(true);