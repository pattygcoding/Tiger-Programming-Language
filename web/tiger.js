// Drives the Tiger WebAssembly runtime.
//
// The runtime is a plain `wasm32-unknown-unknown` module with an `extern "C"`
// ABI, so this file replaces the Go toolchain's `wasm_exec.js`: it instantiates
// the module, supplies the `tiger` imports, and streams output.
//
// Input is the one place where the Rust runtime differs from the Go one. Go
// parks the interpreter's goroutine while its event loop keeps delivering
// messages, so the worker itself could receive the answer. Rust has no
// goroutines, so the worker blocks inside `Atomics.wait` instead, and a blocked
// worker cannot receive messages. The answer therefore travels through a
// `SharedArrayBuffer` the page's main thread writes directly:
//
//   worker: postMessage({ type: "input" })  ->  Atomics.wait(...)
//   page:   show the field, write the line into the buffer, Atomics.notify(...)
//
// The worker copies the delivered bytes out of the shared buffer before decoding
// them: `TextDecoder.decode` is not declared `[AllowShared]`, so a browser fails
// the call with "The provided ArrayBufferView value must not be shared".
//
// `SharedArrayBuffer` needs a cross-origin-isolated page, so the playground
// server sends COOP and COEP. Without it `input()` reports that no input stream
// is connected, exactly as the native runtime does.

const IMPORT_MODULE = "tiger";
/** The shared input region; it caps one line the way output is capped. */
const INPUT_BYTES = 1 << 20;
/** `control[0]` is the state: 0 waiting, 1 a line, -1 end of input. */
const STATE = 0;
/** `control[1]` is the delivered line's length in bytes. */
const LENGTH = 1;
/** The delivered line's bytes begin here, past the two control words. */
const TEXT = 8;

/**
 * Loads the runtime and returns a driver for it.
 *
 * @param {object} options
 * @param {string} [options.url] the compiled module's URL
 * @param {(text: string) => void} [options.output] receives streamed output
 * @param {() => void} [options.requestInput] asks the page for a line
 * @param {(importer: string, requested: string) => {path: string, error: string}} [options.resolveModule]
 * @param {(filename: string) => {source: string, error: string}} [options.loadModule]
 */
export async function createRuntime({ url = "tiger.wasm", output = null, requestInput = null, resolveModule = null, loadModule = null } = {}) {
  const encoder = new TextEncoder();
  const decoder = new TextDecoder();
  const shared = createSharedBuffer();
  const control = shared ? new Int32Array(shared, STATE, 2) : null;
  const sharedText = shared ? new Uint8Array(shared, TEXT) : null;

  let instance = null;
  let response = "";
  let responsePointer = 0;
  let responseCapacity = 0;
  let linePointer = 0;
  let lineCapacity = 0;

  /** Copies `value` into a reusable block of module memory. */
  function write(pointer, capacity, value) {
    const bytes = encoder.encode(value);
    if (bytes.length > capacity) {
      if (pointer) instance.exports.tiger_free(pointer, capacity);
      capacity = Math.max(bytes.length, 256);
      pointer = instance.exports.tiger_alloc(capacity);
    }
    new Uint8Array(instance.exports.memory.buffer, pointer, bytes.length).set(bytes);
    return { pointer, capacity, length: bytes.length };
  }

  const imports = {
    [IMPORT_MODULE]: {
      tiger_output(pointer, length) {
        if (output) output(decoder.decode(new Uint8Array(instance.exports.memory.buffer, pointer, length)));
      },
      tiger_has_input: () => (control && requestInput ? 1 : 0),
      tiger_has_loader: () => (resolveModule && loadModule ? 1 : 0),
      tiger_input() {
        Atomics.store(control, STATE, 0);
        requestInput();
        Atomics.wait(control, STATE, 0);
        if (Atomics.load(control, STATE) < 0) return -1;
        // `slice` copies out of the shared buffer rather than aliasing it, because
        // a browser rejects a `TextDecoder` input backed by a `SharedArrayBuffer`.
        const line = decoder.decode(sharedText.slice(0, Atomics.load(control, LENGTH)));
        const written = write(linePointer, lineCapacity, line);
        linePointer = written.pointer;
        lineCapacity = written.capacity;
        return written.length;
      },
      tiger_input_ptr: () => linePointer,
      tiger_resolve(importerPointer, importerLength, requestedPointer, requestedLength) {
        const memory = new Uint8Array(instance.exports.memory.buffer);
        const importer = decoder.decode(memory.subarray(importerPointer, importerPointer + importerLength));
        const requested = decoder.decode(memory.subarray(requestedPointer, requestedPointer + requestedLength));
        const result = resolveModule(importer, requested) ?? {};
        response = result.path ?? "";
        return result.error ? 1 : 0;
      },
      tiger_load(filenamePointer, filenameLength) {
        const memory = new Uint8Array(instance.exports.memory.buffer);
        const filename = decoder.decode(memory.subarray(filenamePointer, filenamePointer + filenameLength));
        const result = loadModule(filename) ?? {};
        response = result.source ?? "";
        return result.error ? 1 : 0;
      },
      tiger_response_ptr() {
        const written = write(responsePointer, responseCapacity, response);
        responsePointer = written.pointer;
        responseCapacity = written.capacity;
        return written.pointer;
      },
      tiger_response_len: () => encoder.encode(response).length,
    },
  };

  const module = await WebAssembly.compile(await fetchModule(url));
  instance = await WebAssembly.instantiate(module, imports);

  return { run: (source, filename) => runProgram(instance, encoder, decoder, source, filename), shared, input: control !== null };
}

/** Runs a program, returning `{ output, error }`. */
function runProgram(instance, encoder, decoder, source, filename = "examples/playground.tg") {
  const sourceBytes = encoder.encode(source);
  const filenameBytes = encoder.encode(filename);
  const sourcePointer = instance.exports.tiger_alloc(Math.max(sourceBytes.length, 1));
  const filenamePointer = instance.exports.tiger_alloc(Math.max(filenameBytes.length, 1));
  const memory = new Uint8Array(instance.exports.memory.buffer);
  memory.set(sourceBytes, sourcePointer);
  memory.set(filenameBytes, filenamePointer);
  try {
    instance.exports.tiger_run(sourcePointer, sourceBytes.length, filenamePointer, filenameBytes.length);
    const length = instance.exports.tiger_result_len();
    const result = new Uint8Array(instance.exports.memory.buffer, instance.exports.tiger_result_ptr(), length);
    const view = new DataView(result.buffer, result.byteOffset, result.byteLength);
    const outputLength = view.getUint32(0, true);
    const errorLength = view.getUint32(4 + outputLength, true);
    return {
      output: decoder.decode(result.subarray(4, 4 + outputLength)),
      error: decoder.decode(result.subarray(8 + outputLength, 8 + outputLength + errorLength)),
    };
  } finally {
    instance.exports.tiger_free(sourcePointer, Math.max(sourceBytes.length, 1));
    instance.exports.tiger_free(filenamePointer, Math.max(filenameBytes.length, 1));
  }
}

/**
 * The page's half of the input channel: writes a line into the shared buffer and
 * wakes the worker. `end()` reports end of input, as Ctrl+D does.
 */
export function createInputChannel(shared) {
  if (!shared) return null;
  const encoder = new TextEncoder();
  const control = new Int32Array(shared, STATE, 2);
  const text = new Uint8Array(shared, TEXT);
  return {
    write(value) {
      const bytes = encoder.encode(value).subarray(0, text.length);
      text.set(bytes);
      Atomics.store(control, LENGTH, bytes.length);
      Atomics.store(control, STATE, 1);
      Atomics.notify(control, STATE);
    },
    end() {
      Atomics.store(control, STATE, -1);
      Atomics.notify(control, STATE);
    },
  };
}

function createSharedBuffer() {
  try {
    return new SharedArrayBuffer(TEXT + INPUT_BYTES);
  } catch {
    return null;
  }
}

async function fetchModule(url) {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`Runtime download failed (${response.status})`);
  return response.arrayBuffer();
}
