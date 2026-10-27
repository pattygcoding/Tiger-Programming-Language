// The dashboard's page.
//
// It opens one event stream and draws whatever arrives, so a run is watchable while
// it happens: each Tiger section and each benchmark program is pushed the moment it
// has been measured. The report that closes a run is authoritative and redraws
// everything, so a page that connected late still ends up with the same picture.

const $ = (selector) => document.querySelector(selector);

// A bar at full width is 2.00x, which puts the 1.00x line at the middle of the
// track: a section whose bar stops short of it is faster than CPython.
const PROTOCOL_SCALE = 2;

const state = {
  report: null,
  running: false,
  plan: { iterations: 100, corpus: true, engines: false },
  // Live numbers, keyed by name, until the report replaces them.
  tiger: new Map(),
  python: new Map(),
  corpus: new Map(),
  tree: new Map(),
};

function setStatus(text, kind = "") {
  const status = $("#status");
  status.textContent = text;
  status.className = kind;
}

function log(text, kind = "") {
  const line = document.createElement("div");
  if (kind) line.className = kind;
  line.textContent = text;
  $("#log").append(line);
  $("#log").scrollTop = $("#log").scrollHeight;
}

function clearLog() {
  $("#log").replaceChildren();
}

// Builds one row: a name, a track with an optional bar and 1.00x rule, and a value.
function buildRow(name, { width = 0, kind = "", rule = null, value = "", note = "", rowClass = "" }) {
  const row = document.createElement("div");
  row.className = `row ${rowClass}`.trim();

  const label = document.createElement("span");
  label.className = "name";
  label.textContent = name;
  label.title = name;

  const track = document.createElement("div");
  track.className = "track";
  const bar = document.createElement("div");
  bar.className = `bar ${kind}`.trim();
  bar.style.width = `${Math.max(0, Math.min(width, 100))}%`;
  track.append(bar);
  if (rule !== null) {
    const line = document.createElement("div");
    line.className = "rule";
    line.style.left = `${rule}%`;
    track.append(line);
  }

  const measurement = document.createElement("span");
  measurement.className = "value";
  measurement.textContent = value;
  if (note) {
    const detail = document.createElement("small");
    detail.textContent = note;
    measurement.append(detail);
  }

  row.append(label, track, measurement);
  return row;
}

function fill(container, rows, emptySelector) {
  container.replaceChildren(...rows);
  const empty = emptySelector ? $(emptySelector) : null;
  if (empty) empty.hidden = rows.length > 0;
}

// --- The protocol panel -------------------------------------------------

// What the protocol panel draws: the report once a run has finished, and otherwise
// whatever the event stream has delivered so far, so the rows appear as they are
// measured rather than all at once.
function protocolData() {
  const measured = state.report?.protocol;
  if (measured?.available) {
    return measured.sections.map((section) => ({
      name: section.name,
      ratio: section.ratio,
      probe: section.probe,
      agrees: section.agrees,
      note: `tiger ${section.tigerWork.toFixed(3)}ms · python ${section.pythonExec.toFixed(3)}ms`,
    }));
  }
  return [...state.tiger.entries()].map(([name, tiger]) => {
    const python = state.python.get(name);
    return {
      name,
      ratio: python ? tiger.work / python.exec : null,
      probe: name === "overhead",
      agrees: true,
      note: python
        ? `tiger ${tiger.work.toFixed(3)}ms · python ${python.exec.toFixed(3)}ms`
        : `tiger ${tiger.work.toFixed(3)}ms · python pending`,
    };
  });
}

function renderProtocol() {
  const rows = protocolData().map((entry) => {
    const pending = entry.ratio === null || !Number.isFinite(entry.ratio);
    return buildRow(entry.name, {
      width: pending ? 0 : (entry.ratio / PROTOCOL_SCALE) * 100,
      kind: entry.probe ? "python" : entry.ratio > 1 ? "slow" : "",
      rule: 100 / PROTOCOL_SCALE,
      value: pending ? "\u2026" : `${entry.ratio.toFixed(2)}x`,
      note: entry.note,
      rowClass: entry.probe ? "probe" : entry.agrees ? "" : "failed",
    });
  });
  fill($("#protocol-rows"), rows, "#protocol-empty");
  renderSummary();
}

// --- The corpus panel ---------------------------------------------------

function corpusData() {
  const measured = state.report?.corpus;
  if (measured) return measured.programs;
  return [...state.corpus.entries()].map(([name, min]) => ({ name, min, mean: null, lines: null, error: null }));
}

function renderCorpus() {
  const data = corpusData();
  const timed = data.filter((program) => Number.isFinite(program.min));
  const heaviest = Math.max(...timed.map((program) => program.min), 0.001);
  const rows = [...timed]
    .sort((left, right) => right.min - left.min)
    .map((program) =>
      buildRow(program.name, {
        width: (program.min / heaviest) * 100,
        value: `${program.min.toFixed(3)} ms`,
        note: program.mean === null ? "" : `mean ${program.mean.toFixed(3)} ms`,
      })
    );
  // A program that could not be measured is a gap rather than a zero: a bar of no
  // length would read as the fastest program in the corpus.
  for (const program of data.filter((program) => program.error)) {
    rows.push(buildRow(program.name, { value: "failed", note: program.error, rowClass: "failed" }));
  }
  fill($("#corpus-rows"), rows, "#corpus-empty");
  renderSummary();
}

// --- The engine panel ---------------------------------------------------

// Each program's best run on each engine, from the report once it exists and from
// the two live maps before that, so the second pass fills in as it is measured.
function engineData() {
  const measured = state.report?.tree;
  if (measured) {
    const byName = new Map((state.report?.corpus?.programs ?? []).map((program) => [program.name, program.min]));
    return measured.programs
      .filter((program) => Number.isFinite(program.min) && Number.isFinite(byName.get(program.name)))
      .map((program) => ({ name: program.name, tree: program.min, auto: byName.get(program.name) }));
  }
  return [...state.tree.entries()]
    .filter(([name, tree]) => state.corpus.has(name) && Number.isFinite(tree))
    .map(([name, tree]) => ({ name, tree, auto: state.corpus.get(name) }));
}

// The second pass is opt-in because it doubles the wait, so this panel appears only
// when a run has been asked for it.
function renderEngines() {
  const panel = $("#engines-panel");
  const compared = engineData()
    .map((entry) => ({ ...entry, speedup: entry.tree / entry.auto }))
    .sort((left, right) => right.speedup - left.speedup);
  if (compared.length === 0) {
    panel.hidden = true;
    return;
  }
  panel.hidden = false;
  const heaviest = Math.max(...compared.map((entry) => entry.tree), 0.001);
  const rows = compared.map((entry) =>
    buildRow(entry.name, {
      width: (entry.tree / heaviest) * 100,
      kind: "tree",
      value: `${entry.speedup.toFixed(2)}x`,
      note: `tree ${entry.tree.toFixed(3)}ms · default ${entry.auto.toFixed(3)}ms`,
    })
  );
  fill($("#engines-rows"), rows, null);
  renderSummary();
}

// --- The headline tiles -------------------------------------------------

function setTile(id, value, note, kind = "") {
  const strong = $(id);
  strong.textContent = value;
  strong.className = kind;
  $(`${id}-note`).textContent = note;
}

function renderSummary() {
  const sections = protocolData().filter((entry) => !entry.probe && Number.isFinite(entry.ratio));
  const under = sections.filter((entry) => entry.ratio <= 1);
  setTile(
    "#stat-sections",
    sections.length ? `${under.length} / ${sections.length}` : "\u2014",
    sections.length ? "sections past the CPython line" : "nothing measured yet",
    sections.length ? (under.length === sections.length ? "good" : "bad") : ""
  );

  const fastest = sections.reduce((best, entry) => (best === null || entry.ratio < best.ratio ? entry : best), null);
  setTile("#stat-fastest", fastest ? `${fastest.ratio.toFixed(2)}x` : "\u2014", fastest ? fastest.name : "\u2014", fastest ? "good" : "");

  const corpus = state.report?.corpus;
  setTile(
    "#stat-corpus",
    corpus ? `${corpus.totalMin.toFixed(2)} ms` : state.corpus.size ? `${state.corpus.size} timed` : "\u2014",
    corpus ? `${corpus.programs.length} programs, best run each` : state.corpus.size ? "programs so far" : "\u2014"
  );

  const seconds = state.report?.seconds;
  setTile("#stat-time", seconds === undefined ? "\u2014" : `${seconds.toFixed(2)} s`, `${state.plan.iterations} iterations`);
}

// --- The run ------------------------------------------------------------

// Puts the server's plan into the controls, adding an option when the server was
// started with an iteration count the list does not offer.
function applyPlan(plan) {
  state.plan = plan;
  const select = $("#iterations");
  if (![...select.options].some((option) => Number(option.value) === plan.iterations)) {
    const option = document.createElement("option");
    option.value = String(plan.iterations);
    option.textContent = String(plan.iterations);
    select.append(option);
  }
  select.value = String(plan.iterations);
  $("#corpus").checked = plan.corpus;
  $("#engines").checked = plan.engines;
}

function setRunning(running) {
  state.running = running;
  const play = $("#play");
  play.disabled = running;
  play.classList.toggle("busy", running);
  play.textContent = running ? "Measuring\u2026" : "Play";
  setStatus(running ? "Measuring" : "Ready", running ? "running" : "ready");
}

function resetPanels() {
  clearLog();
  state.tiger.clear();
  state.python.clear();
  state.corpus.clear();
  state.tree.clear();
  state.report = null;
  renderProtocol();
  renderCorpus();
  renderEngines();
}

function handleEvent(event) {
  switch (event.type) {
    case "start":
      applyPlan(event.plan);
      resetPanels();
      setRunning(true);
      log(`measuring ${event.plan.iterations} iterations`);
      break;
    case "phase":
      log(`phase: ${event.phase}`, "ok");
      break;
    case "tiger":
      state.tiger.set(event.name, event);
      renderProtocol();
      break;
    case "python":
      for (const section of event.sections) state.python.set(section.name, section);
      log(`CPython measured ${event.sections.length} sections`);
      renderProtocol();
      break;
    case "program":
      // The second pass reports the same programs on the other engine, so the engine
      // it names is what decides which panel the number belongs to.
      if (event.engine === "tree") state.tree.set(event.name, event.min);
      else state.corpus.set(event.name, event.min);
      renderCorpus();
      renderEngines();
      break;
    case "note":
      log(event.text, "warn");
      break;
    case "report":
      state.report = event.report;
      applyPlan(event.report.plan);
      renderProtocol();
      renderCorpus();
      renderEngines();
      renderSummary();
      setRunning(false);
      log(`done in ${event.report.seconds.toFixed(2)} s`, "ok");
      break;
    default:
      break;
  }
}

async function play() {
  if (state.running) return;
  const body = {
    iterations: Number($("#iterations").value) || state.plan.iterations,
    corpus: $("#corpus").checked,
    engines: $("#engines").checked,
  };
  const response = await fetch("/api/play", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!response.ok) {
    const detail = await response.json().catch(() => ({}));
    log(detail.reason ?? `the server refused the run (${response.status})`, "warn");
  }
}

function connect() {
  const events = new EventSource("/api/events");
  events.onopen = () => setStatus(state.running ? "Measuring" : "Ready", state.running ? "running" : "ready");
  events.onerror = () => setStatus("Reconnecting", "error");
  events.onmessage = ({ data }) => {
    try {
      handleEvent(JSON.parse(data));
    } catch (error) {
      log(`could not read an event: ${error.message}`, "warn");
    }
  };
}

async function boot() {
  try {
    const initial = await (await fetch("/api/state")).json();
    applyPlan(initial.plan);
    if (initial.report) {
      state.report = initial.report;
      log("showing the last run");
    } else {
      log("press Play to measure");
    }
    setRunning(initial.running);
    renderProtocol();
    renderCorpus();
    renderEngines();
  } catch (error) {
    log(`could not reach the server: ${error.message}`, "warn");
    setStatus("No server", "error");
  }
  connect();
}

$("#play").addEventListener("click", play);
$("#copy").addEventListener("click", async () => {
  if (!state.report) {
    log("nothing to copy yet", "warn");
    return;
  }
  await navigator.clipboard.writeText(JSON.stringify(state.report, null, 2));
  log("report copied as JSON", "ok");
});

boot();

