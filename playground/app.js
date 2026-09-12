const $ = (id) => document.getElementById(id);

const els = {
  source: $("source"),
  example: $("example"),
  run: $("run"),
  exec: $("exec"),
  emit: $("emit"),
  check: $("check"),
  publish: $("publish"),
  status: $("status"),
  stamp: $("stamp"),
  error: $("error"),
  summary: $("summary"),
  health: $("health"),
  sourceHash: $("source-hash"),
  hashKind: $("hash-kind"),
  hashCopy: $("hash-copy"),
  seats: $("seats"),
  contractsCount: $("contracts-count"),
  contractsList: $("contracts-list"),
  contractsRefresh: $("contracts-refresh"),
  riderLink: $("rider-link"),
  notelogList: $("notelog-list"),
  criticList: $("critic-list"),
  notelogForm: $("notelog-form"),
  notelogInput: $("notelog-input"),
  criticForm: $("critic-form"),
  criticInput: $("critic-input"),
  criticSeverity: $("critic-severity"),
  criticCategory: $("critic-category"),
  bookRefresh: $("book-refresh"),
  agentSkill: $("agent-skill"),
  agentMessage: $("agent-message"),
  agentHost: $("agent-host"),
  agentRun: $("agent-run"),
  agentPropose: $("agent-propose"),
  agentAdopt: $("agent-adopt"),
  modePlay: $("mode-play"),
  modeAgent: $("mode-agent"),
  out: {
    py: $("out-py"),
    go: $("out-go"),
    js: $("out-js"),
    interp: $("out-interp"),
    lang: $("out-lang"),
    stdout: $("out-stdout"),
  },
  seatState: {
    py: $("seat-py-state"),
    go: $("seat-go-state"),
    js: $("seat-js-state"),
    interp: $("seat-interp-state"),
  },
  langPick: $("lang-pick"),
};

const GATE = ["py", "go", "js"];
const EMPTY = {
  py: "n/a — Run exactness to fill this seat",
  go: "n/a — Run exactness to fill this seat",
  js: "n/a — Run exactness to fill this seat",
  interp: "Run for instant stdout. Same answer as the gate — not a bypass.",
  lang: "Emit or Run exactness to fill the catalog artifact.",
  log: "Check log appears after Check or Run exactness.",
};

let lastLangs = {};
let catalog = [];
let examples = [];
let running = false;
let mode = "play";
let lastProposeSource = "";
let liveHash = "";
let checkedHash = "";
let hashTimer = 0;
let lastStdout = { py: undefined, go: undefined, js: undefined, interp: undefined };
let lastErrors = {};
let lastExactness = "";

const DEFAULT_SOURCE =
  "def greet(name: str) -> str do\n    ret `hello ${name}`\nend\n\nsay(greet(\"CuNi\"))\nsay(1 + 2 * 3)\n";

function actionButtons() {
  return [els.run, els.emit, els.check, els.exec, els.publish].filter(Boolean);
}

function setStatus(kind, text) {
  els.status.className = `badge ${kind}`;
  els.status.textContent = text;
}

function setStamp(kind, k, v) {
  if (!els.stamp) return;
  const prev = els.stamp.dataset.k;
  els.stamp.className = `stamp ${kind}`;
  els.stamp.dataset.k = k;
  els.stamp.innerHTML =
    `<span class="stamp-k">${esc(k)}</span>` +
    `<span class="stamp-v">${esc(v || "")}</span>`;
  if (prev !== k && (kind === "pass" || kind === "fail")) {
    els.stamp.classList.add("ink");
    window.setTimeout(() => els.stamp && els.stamp.classList.remove("ink"), 320);
  }
}

function setSummary(text, kind) {
  els.summary.textContent = text || "";
  els.summary.className = `summary mono${kind ? " " + kind : ""}`;
}

/** Concrete fix-its for type + exactness refusals. Never suggests approximate mode. */
function polishFixIt(msg) {
  const s = String(msg || "");
  if (!s) return s;
  if (/fix-it:/i.test(s)) return s;
  const hints = [];
  if (/undefined variable/i.test(s)) {
    hints.push("fix-it: declare it with `let name = …` or `mut name = …` before use (SPEC.md §6)");
  } else if (/undefined function/i.test(s)) {
    hints.push("fix-it: define `def name(...) -> T do … end` above the call, or check spelling");
  } else if (/unknown type/i.test(s)) {
    hints.push("fix-it: use a known type (`int`, `str`, `bool`, `float`, `list<T>`, `map<K,V>`, `opt<T>`) or a declared `typ`/`enum`");
  } else if (/expects \d+ argument/i.test(s)) {
    hints.push("fix-it: pass exactly the declared arity — no extra/missing args (exactness refuses silent coercion)");
  } else if (/declares `->|ret` value has type/i.test(s)) {
    hints.push("fix-it: change the `ret` expression or the `-> T` annotation so they match");
  } else if (/fallible/i.test(s) && /\?\?/.test(s) === false && /unwrap/i.test(s)) {
    hints.push("fix-it: unwrap with `??` or handle failure explicitly — bare fallible results are refused");
  } else if (/fallible/i.test(s)) {
    hints.push("fix-it: unwrap with `let x = f(…) ?? fallback` (SPEC.md §12) — bare fallible results are refused");
  } else if (/immutable|let`-bound|cannot assign/i.test(s)) {
    hints.push("fix-it: declare the binding `mut` if mutation is intended");
  } else if (/stdout diverged|exactness:\s*FAIL|exactness failed|catalog language/i.test(s)) {
    hints.push(
      "fix-it: remove `ext` host differences, avoid non-portable float printing, and keep integer/`say` paths identical — CuNi has no approximate mode"
    );
  } else if (/type error/i.test(s)) {
    hints.push("fix-it: resolve the type error above; exactness never runs on a refused program");
  }
  if (!hints.length) return s;
  return s.replace(/\s*$/, "") + "\n\n" + hints.join("\n");
}

function showError(msg) {
  if (!msg) {
    els.error.classList.add("hidden");
    els.error.textContent = "";
    return;
  }
  els.error.classList.remove("hidden");
  els.error.textContent = polishFixIt(msg);
}

function shortHash(h) {
  const s = (h || "").toString();
  return s.length > 16 ? s.slice(0, 12) + "…" : s || "—";
}

function hexHash(buf) {
  return Array.from(new Uint8Array(buf))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

async function sha256(text) {
  if (!window.crypto || !crypto.subtle) return "";
  const buf = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
  return hexHash(buf);
}

function renderHash(h, kind) {
  liveHash = h || liveHash;
  if (els.sourceHash) {
    els.sourceHash.textContent = h || "—";
    els.sourceHash.title = h || "";
  }
  if (els.hashKind) {
    els.hashKind.textContent = kind || "live";
    els.hashKind.dataset.kind = kind || "live";
  }
}

async function refreshLiveHash() {
  const h = await sha256(els.source.value || "");
  if (!h) {
    renderHash("—", "live");
    return;
  }
  const kind =
    checkedHash && h === checkedHash
      ? "checked"
      : checkedHash
        ? "edited"
        : "live";
  renderHash(h, kind);
  if (checkedHash && h !== checkedHash && !running) {
    const k = (els.stamp && els.stamp.dataset.k) || "";
    if (k === "PASS" || k === "FAIL") {
      setStamp("stale", "STALE", "source edited");
      setStatus("idle", "edited");
    }
  }
}

function riderUrlFromHealth(j) {
  const u =
    (j && j.rider && (j.rider.remote_url || (j.rider.contracts && j.rider.contracts.url))) ||
    "";
  if (typeof u === "string" && u.startsWith("http")) {
    return u.replace(/\/api\/v0\/contracts\/?$/, "") || "https://agentrider.fly.dev";
  }
  return "https://agentrider.fly.dev";
}

function renderContracts(reg, health) {
  const countEl = els.contractsCount;
  const listEl = els.contractsList;
  const linkEl = els.riderLink;
  if (!listEl) return;
  const riderBase = riderUrlFromHealth(health);
  if (linkEl) {
    linkEl.href = riderBase;
    linkEl.title = `Open Agent-Rider (${riderBase})`;
  }
  const count = reg && typeof reg.count === "number" ? reg.count : 0;
  const contracts = (reg && Array.isArray(reg.contracts) && reg.contracts) || [];
  if (countEl) countEl.textContent = `(${count})`;
  if (!count || contracts.length === 0) {
    listEl.innerHTML =
      `<div class="book-empty">No registered contracts. Run exactness → <strong>Publish</strong> to register into Rider (local stub always; remote when healthy).</div>`;
    return;
  }
  const recent = contracts.slice(0, 8);
  listEl.innerHTML = recent
    .map((c) => {
      const id = (c && c.id) || "—";
      const hash = shortHash(c && c.sourceHash);
      const when = (c && c.registeredAt) || "—";
      const st = (c && c.status) || "registered";
      return (
        `<div class="contract-row" title="${esc(hash)}">` +
        `<span class="cid">${esc(id)}</span>` +
        `<span class="hash">${esc(hash)}</span>` +
        `<span class="when">${esc(when)}</span>` +
        `<span class="st">${esc(st)}</span>` +
        `</div>`
      );
    })
    .join("");
}

function fillLangPick(langs) {
  lastLangs = langs || {};
  const pick = els.langPick;
  if (!pick) return;
  const prev = pick.value;
  const emitted = Object.keys(lastLangs);
  const rows =
    catalog.length > 0
      ? catalog
      : emitted.sort().map((file) => ({ file, name: file, id: file.split(".")[0] }));
  if (rows.length === 0) return;
  pick.innerHTML = "";
  const gateIds = new Set(GATE);
  for (const row of rows) {
    const o = document.createElement("option");
    o.value = row.file;
    const gate = gateIds.has(row.id) ? " · gate" : "";
    o.textContent = `${row.name} (${row.id})${gate}`;
    pick.appendChild(o);
  }
  const files = rows.map((r) => r.file);
  pick.value = files.includes(prev)
    ? prev
    : files.find((n) => n === "py.py" || n.endsWith(".py")) || files[0];
  showPickedLang();
}

function showPickedLang() {
  const key = els.langPick && els.langPick.value;
  const text =
    (key && lastLangs[key]) || lastLangs["py.py"] || lastLangs["out.py"] || "";
  if (els.out.lang) {
    els.out.lang.textContent = text || "";
    els.out.lang.dataset.empty = EMPTY.lang;
  }
}

async function loadLangCatalog() {
  try {
    const r = await fetch("/api/langs");
    const j = await r.json();
    catalog = j.langs || [];
    fillLangPick(lastLangs);
  } catch (e) {
    console.warn("langs catalog", e);
  }
}

function seatEl(id) {
  return document.getElementById(`seat-${id}`);
}

function fillSeatBody(id, text, fallback) {
  const el = els.out[id];
  if (!el) return;
  if (text === undefined || text === null) {
    el.textContent = "";
    el.dataset.empty = fallback;
    return;
  }
  el.textContent = text === "" ? "(empty)" : text;
}

function setSeatState(id, state, label) {
  const art = seatEl(id);
  if (art) art.dataset.state = state || "";
  if (els.seatState[id]) els.seatState[id].textContent = label;
}

function renderSeats(data) {
  const stdout = lastStdout;
  const errs = lastErrors;
  const present = GATE.map((k) => stdout[k]).filter((v) => v !== undefined);
  const allMatch =
    present.length === 3 && present.every((v) => v === present[0]);
  const anyDiverge =
    present.length >= 2 && present.some((v) => v !== present[0]);

  if (els.seats) {
    els.seats.classList.toggle("match", allMatch);
    els.seats.classList.toggle("diverge", Boolean(anyDiverge && !allMatch));
  }

  for (const k of GATE) {
    if (errs[k]) {
      fillSeatBody(k, `ERROR: ${errs[k]}`, EMPTY[k]);
      setSeatState(k, "error", "error");
    } else if (stdout[k] !== undefined) {
      fillSeatBody(k, stdout[k], EMPTY[k]);
      if (allMatch) setSeatState(k, "match", "match");
      else if (anyDiverge) setSeatState(k, "diverge", "diverge");
      else setSeatState(k, "ok", "stdout");
    } else {
      fillSeatBody(k, undefined, EMPTY[k]);
      setSeatState(k, "", "n/a");
    }
  }

  if (errs.interp) {
    fillSeatBody("interp", `ERROR: ${errs.interp}`, EMPTY.interp);
    setSeatState("interp", "error", "fail");
  } else if (stdout.interp !== undefined) {
    fillSeatBody("interp", stdout.interp, EMPTY.interp);
    const matchesGate =
      allMatch && present[0] !== undefined && stdout.interp === present[0];
    setSeatState(
      "interp",
      "ok",
      matchesGate ? "matches seats" : "same answer, instant"
    );
  } else {
    fillSeatBody("interp", undefined, EMPTY.interp);
    setSeatState("interp", "", "same answer, instant");
  }

  if (els.out.stdout) {
    const parts = [];
    if (data && data.check_log) {
      parts.push("--- cuni check ---");
      parts.push(String(data.check_log).trim());
    } else if (data && data.phase === "exec" && data.check_log) {
      parts.push("--- cuni run ---");
      parts.push(String(data.check_log).trim());
    }
    els.out.stdout.textContent = parts.join("\n");
    els.out.stdout.dataset.empty = EMPTY.log;
  }
}

function setOutputs(data) {
  const phase = data.phase;
  if (phase !== "exec") {
    // Keep last emit artifacts when interpreter-only.
  }
  fillLangPick(data.langs || lastLangs);
  if (!data.langs || !Object.keys(data.langs).length) {
    if (els.out.lang && data.py) els.out.lang.textContent = data.py;
  }

  const stdout = data.stdout || {};
  const runErrs = data.run_errors || {};

  if (phase === "exec") {
    const interpOut =
      stdout.interp !== undefined
        ? stdout.interp
        : stdout.py !== undefined
          ? stdout.py
          : undefined;
    if (interpOut !== undefined) lastStdout.interp = interpOut;
    lastErrors.interp = runErrs.interp || runErrs.py || (data.ok ? "" : data.error || "");
    if (!lastErrors.interp) delete lastErrors.interp;
  } else if (phase === "run" || phase === "check" || stdout.py !== undefined || stdout.go !== undefined || stdout.js !== undefined) {
    for (const k of GATE) {
      if (stdout[k] !== undefined) lastStdout[k] = stdout[k];
      if (runErrs[k]) lastErrors[k] = runErrs[k];
      else if (stdout[k] !== undefined) delete lastErrors[k];
    }
    if (stdout.interp !== undefined) lastStdout.interp = stdout.interp;
  }

  if (data.source_hash) {
    checkedHash = data.source_hash;
    renderHash(data.source_hash, data.exactness === "PASS" || data.exactness === "FAIL" ? "checked" : "live");
  }

  lastExactness = data.exactness || lastExactness;
  renderSeats(data);
  showPickedLang();
}

function selectTab(name) {
  document.querySelectorAll(".tab").forEach((t) => {
    t.classList.toggle("active", t.dataset.tab === name);
  });
  document.querySelectorAll(".code").forEach((p) => p.classList.remove("active"));
  if (name === "stdout") {
    $("out-stdout").classList.add("active");
  } else {
    $("out-lang").classList.add("active");
  }
}

function selectBook(name) {
  document.querySelectorAll(".book-tab").forEach((t) => {
    t.classList.toggle("active", t.dataset.book === name);
  });
  document.querySelectorAll(".book-panel").forEach((p) => p.classList.remove("active"));
  $(name === "notelog" ? "panel-notelog" : "panel-critic").classList.add("active");
}

function esc(s) {
  return String(s)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function renderNotelog(entries) {
  const list = [...(entries || [])].reverse();
  if (!list.length) {
    els.notelogList.innerHTML = `<div class="book-empty">No notes yet. Run exactness, Run, or add a note.</div>`;
    return;
  }
  els.notelogList.innerHTML = list
    .map(
      (e) => `
    <article class="book-entry kind-${esc(e.kind || "manual")}">
      <header>
        <time>${esc(e.ts || "")}</time>
        <span class="pill">${esc(e.kind || "manual")}</span>
      </header>
      <pre class="book-body">${esc(e.body || "")}</pre>
    </article>`
    )
    .join("");
}

function renderCritic(entries) {
  const list = [...(entries || [])].reverse();
  if (!list.length) {
    els.criticList.innerHTML = `<div class="book-empty">No critiques yet. Failures auto-log; add design notes anytime.</div>`;
    return;
  }
  els.criticList.innerHTML = list
    .map((e) => {
      const loc =
        e.line != null
          ? `main.cuni:${e.line}${e.col != null ? ":" + e.col : ""}`
          : "";
      return `
    <article class="book-entry sev-${esc(e.severity || "note")}">
      <header>
        <time>${esc(e.ts || "")}</time>
        <span class="pill sev">${esc(e.severity || "note")}</span>
        <span class="pill cat">${esc(e.category || "")}</span>
        <span class="pill src">${esc(e.source || "")}</span>
        ${loc ? `<span class="pill loc mono">${esc(loc)}</span>` : ""}
      </header>
      <pre class="book-body">${esc(e.body || "")}</pre>
    </article>`;
    })
    .join("");
}

async function refreshBooks() {
  try {
    const [n, c] = await Promise.all([
      fetch("/api/notelog").then((r) => r.json()),
      fetch("/api/criticbook").then((r) => r.json()),
    ]);
    renderNotelog(n.entries);
    renderCritic(c.entries);
  } catch (e) {
    console.warn("books refresh", e);
  }
}

async function loadHealth() {
  try {
    const [r, regR] = await Promise.all([
      fetch("/api/health"),
      fetch("/api/rider/registered").catch(() => null),
    ]);
    const j = await r.json();
    let reg = null;
    let regCount = null;
    if (regR && regR.ok) {
      try {
        reg = await regR.json();
        if (reg && typeof reg.count === "number") regCount = reg.count;
      } catch (_) {
        /* ignore */
      }
    }
    if (!j.ok) {
      els.health.textContent = `toolchain: cuni missing — ${j.error || "build with cargo"}`;
      renderContracts(reg || { count: 0, contracts: [] }, j);
      return;
    }
    const riderBase = riderUrlFromHealth(j);
    const remoteBit =
      j.rider && j.rider.remote
        ? `rider remote: on`
        : j.rider
          ? `rider remote: off`
          : null;
    const gate = Array.isArray(j.exactness_gate) ? j.exactness_gate.join("/") : "py/go/js";
    const parts = [
      `cuni: ok`,
      `gate: ${gate}`,
      `py: ${j.python ? "ok" : "missing"}`,
      `go: ${j.go ? "ok" : "missing"}`,
      `js: ${j.node ? "ok" : "missing"}`,
      `notes: ${j.books?.notelog ?? 0}`,
      `critiques: ${j.books?.critic ?? 0}`,
    ];
    if (regCount != null) parts.push(`registered: ${regCount}`);
    else if (j.rider) parts.push(`rider: ${j.rider.register ? "ok" : "off"}`);
    if (remoteBit) parts.push(remoteBit);
    parts.push(`langs: ${j.lang_count ?? 119}`);
    els.health.textContent = parts.join(" · ");
    if (els.riderLink) {
      els.riderLink.href = riderBase;
    }
    renderContracts(reg || { count: 0, contracts: [] }, j);
  } catch (e) {
    els.health.textContent = `health check failed: ${e}`;
    renderContracts({ count: 0, contracts: [] }, null);
  }
}

async function loadExamples() {
  const r = await fetch("/api/examples");
  const j = await r.json();
  examples = j.examples || [];
  els.example.innerHTML = "";
  const blank = document.createElement("option");
  blank.value = "";
  blank.textContent = "— starter —";
  els.example.appendChild(blank);
  for (const ex of examples) {
    const o = document.createElement("option");
    o.value = ex.id;
    o.textContent = ex.name;
    els.example.appendChild(o);
  }
  const preferred =
    examples.find((e) => e.id === "spend-control") ||
    examples.find((e) => e.id === "full");
  if (preferred) {
    els.example.value = preferred.id;
    els.source.value = preferred.source;
  } else {
    els.source.value = DEFAULT_SOURCE;
  }
  void refreshLiveHash();
}

function applyVerdict(data) {
  const phase = data.phase;
  const hashBit = data.source_hash ? ` · ${shortHash(data.source_hash)}` : "";

  if (phase === "emit" && data.ok) {
    setStamp("pass", "EMIT", "catalog");
    setStatus("pass", "emit ok");
    showError("");
    setSummary((data.summary || "emit: ok") + hashBit, "pass");
    selectTab("lang");
    return;
  }
  if (phase === "exec") {
    if (data.ok) {
      setStamp("run", "RUN", "interpreter");
      setStatus("pass", "run ok");
      showError("");
      setSummary(
        (data.summary || "run: ok (interpreter, same stdout)") +
          " — not a substitute for exactness" +
          hashBit,
        "run"
      );
      selectTab("stdout");
    } else {
      setStamp("fail", "RUN", "fail");
      setStatus("fail", "run fail");
      showError(data.error || data.summary || "cuni run failed");
      setSummary((data.summary || "run: fail (interpreter)") + hashBit, "fail");
      selectTab("stdout");
      selectBook("critic");
    }
    return;
  }
  if (phase === "emit" || phase === "compile") {
    setStamp("fail", "FAIL", "emit");
    setStatus("fail", "emit error");
    showError(data.error || data.summary || "emit failed");
    setSummary((data.summary || "emit failed") + hashBit, "fail");
    selectTab("stdout");
    selectBook("critic");
    return;
  }
  if (data.exactness === "PASS") {
    setStamp("pass", "PASS", "exactness");
    setStatus("pass", "exactness PASS");
    showError("");
    setSummary((data.summary || "exactness: PASS") + hashBit, "pass");
    return;
  }
  if (data.exactness === "FAIL") {
    setStamp("fail", "FAIL", "exactness");
    setStatus("fail", "exactness FAIL");
    showError(data.error || data.summary || "exactness failed");
    setSummary((data.summary || "exactness: FAIL") + hashBit, "fail");
    selectTab("stdout");
    selectBook("critic");
    return;
  }
  setStamp("idle", "IDLE", "no verdict");
  setStatus(data.ok ? "pass" : "fail", data.ok ? "ok" : "error");
  setSummary(data.summary || "", data.ok ? "pass" : "fail");
}

async function invoke(path, label) {
  if (running) return;
  running = true;
  actionButtons().forEach((b) => {
    b.disabled = true;
  });
  setStatus("run", `${label}…`);
  setStamp("run", "…", label);
  showError("");
  setSummary("");
  if (path === "/api/run" || path === "/api/check") {
    lastStdout.py = lastStdout.go = lastStdout.js = undefined;
    lastErrors = { ...lastErrors };
    delete lastErrors.py;
    delete lastErrors.go;
    delete lastErrors.js;
    renderSeats({});
  } else if (path === "/api/exec") {
    lastStdout.interp = undefined;
    delete lastErrors.interp;
    renderSeats({});
  }

  try {
    const r = await fetch(path, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ source: els.source.value }),
    });
    const data = await r.json();
    if (!r.ok && !data.py && !data.error) {
      throw new Error(data.error || `HTTP ${r.status}`);
    }

    setOutputs(data);
    applyVerdict(data);
    await refreshBooks();
    void loadHealth();
  } catch (e) {
    setStatus("fail", "error");
    setStamp("fail", "FAIL", "request");
    showError(String(e));
    setSummary("request failed", "fail");
  } finally {
    running = false;
    actionButtons().forEach((b) => {
      b.disabled = false;
    });
    void refreshLiveHash();
  }
}

function setMode(next) {
  mode = next;
  document.querySelectorAll(".mode-tab").forEach((t) => {
    t.classList.toggle("active", t.dataset.mode === next);
    t.setAttribute("aria-selected", t.dataset.mode === next ? "true" : "false");
  });
  els.modePlay.classList.toggle("hidden", next !== "play");
  els.modeAgent.classList.toggle("hidden", next !== "agent");
}

async function loadAgentSkills() {
  try {
    const r = await fetch("/api/agent/skills");
    const j = await r.json();
    if (!j.ok && !j.skills) {
      els.agentSkill.innerHTML = `<option value="">(agent pack offline)</option>`;
      return;
    }
    els.agentSkill.innerHTML = "";
    for (const s of j.skills || []) {
      const o = document.createElement("option");
      o.value = s.id;
      o.textContent = `${s.id} — ${s.description || s.entry}`;
      els.agentSkill.appendChild(o);
    }
  } catch (e) {
    els.agentSkill.innerHTML = `<option value="">(failed to load)</option>`;
  }
}

async function agentRun() {
  if (running) return;
  running = true;
  els.agentRun.disabled = true;
  setStatus("run", "agent…");
  setStamp("run", "…", "agent");
  showError("");
  try {
    const r = await fetch("/api/agent/run", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        skill: els.agentSkill.value || undefined,
        message: els.agentMessage.value || "",
        host: els.agentHost.value || "py",
      }),
    });
    const data = await r.json();
    if (data.source) els.source.value = data.source;
    setOutputs({
      phase: "run",
      py: data.py,
      go: data.go,
      js: data.js,
      stdout: { py: data.stdout, go: data.stdout, js: data.stdout },
      exactness: data.exactness,
      source_hash: data.source_hash,
      check_log:
        (data.check_log || "") +
        (data.host_tool
          ? "\n--- host tool ---\n" + JSON.stringify(data.host_tool, null, 2)
          : "") +
        (data.run_error ? "\n--- run error ---\n" + data.run_error : ""),
    });
    await refreshBooks();
    void refreshLiveHash();
    if (data.ok && data.exactness === "PASS") {
      setStamp("pass", "PASS", `skill ${data.skill || ""}`.trim());
      setStatus("pass", `agent ${data.skill}`);
      setSummary(data.summary || "skill PASS", "pass");
      selectTab("stdout");
    } else {
      setStamp("fail", "FAIL", "agent");
      setStatus("fail", "agent refused");
      {
        const raw = data.error || data.summary || "agent refused";
        const hint = /busy|concurrent/i.test(String(raw))
          ? " — server busy (soft limit: max concurrent runs); retry shortly"
          : /timeout/i.test(String(raw))
            ? " — timed out; try a shorter speech or retry"
            : /exactness|FAIL/i.test(String(raw))
              ? " — exactness gate refused the generated law"
              : "";
        showError(String(raw) + hint);
      }
      setSummary(data.summary || "FAIL", "fail");
      selectTab("stdout");
      selectBook("critic");
    }
  } catch (e) {
    setStatus("fail", "error");
    setStamp("fail", "FAIL", "request");
    showError(String(e));
  } finally {
    running = false;
    els.agentRun.disabled = false;
    void loadHealth();
  }
}

async function agentPropose() {
  if (running) return;
  running = true;
  setStatus("run", "propose…");
  setStamp("run", "…", "propose");
  showError("");
  lastProposeSource = els.source.value;
  try {
    const r = await fetch("/api/agent/propose", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ source: els.source.value }),
    });
    const data = await r.json();
    await refreshBooks();
    setOutputs({
      phase: "check",
      py: "",
      go: "",
      js: "",
      stdout: {},
      exactness: data.ok ? "PASS" : "FAIL",
      check_log: data.check_log || data.error || "",
    });
    if (data.ok) {
      setStamp("pass", "PASS", "propose");
      setStatus("pass", "propose PASS");
      setSummary(
        `quarantine ${data.quarantine_id} — exactness PASS (adopt optional)`,
        "pass"
      );
    } else {
      setStamp("fail", "FAIL", "propose");
      setStatus("fail", "propose FAIL");
      showError(data.error || data.summary || "exactness FAIL — not a citizen");
      setSummary(data.summary || "refuse", "fail");
      selectBook("critic");
    }
    selectTab("stdout");
  } catch (e) {
    setStatus("fail", "error");
    setStamp("fail", "FAIL", "request");
    showError(String(e));
  } finally {
    running = false;
    void loadHealth();
  }
}

async function agentAdopt() {
  if (running) return;
  const source = els.source.value || lastProposeSource;
  const name = prompt("Adopt skill name (alnum/_):", "my_skill");
  if (!name) return;
  running = true;
  setStatus("run", "adopt…");
  try {
    const r = await fetch("/api/agent/adopt", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ source, name }),
    });
    const data = await r.json();
    await refreshBooks();
    if (data.adopted) {
      setStamp("pass", "PASS", "adopted");
      setStatus("pass", "adopted");
      setSummary(`adopted ${data.meta?.name} — citizen`, "pass");
    } else {
      setStamp("fail", "FAIL", "not adopted");
      setStatus("fail", "not adopted");
      showError(data.error || "exactness FAIL — refuse adopt");
      setSummary(data.error || "refuse adopt", "fail");
    }
  } catch (e) {
    showError(String(e));
  } finally {
    running = false;
    void loadHealth();
  }
}

/** Trigger browser download of publish metadata (.publish.json). */
function downloadPublishJson(meta, storedName) {
  if (!meta || typeof meta !== "object") return;
  const name =
    (typeof storedName === "string" && storedName.endsWith(".publish.json") && storedName) ||
    `${(meta.sourceHash || "meta").toString().slice(0, 16)}.publish.json`;
  const blob = new Blob([JSON.stringify(meta, null, 2)], {
    type: "application/json",
  });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.rel = "noopener";
  document.body.appendChild(a);
  a.click();
  a.remove();
  URL.revokeObjectURL(url);
}

async function publishToRider() {
  if (running) return;
  running = true;
  actionButtons().forEach((b) => {
    b.disabled = true;
  });
  setStatus("run", "publishing");
  setStamp("run", "…", "publish");
  showError("");
  try {
    const r = await fetch("/api/publish", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ source: els.source.value }),
    });
    const data = await r.json();
    if (!r.ok || !data.ok) {
      setStamp("fail", "FAIL", "publish");
      setStatus("fail", "publish refused");
      showError(data.error || data.exactness || "publish failed");
      setSummary(data.exactness || data.error || "FAIL", "fail");
      return;
    }
    setStamp("pass", "PASS", "published");
    setStatus("pass", "published");
    const h = (data.meta && data.meta.sourceHash) || "";
    if (h) {
      checkedHash = h;
      renderHash(h, "checked");
    }
    const reg = data.registration || {};
    const regBit = reg.id
      ? ` · registered ${reg.id}${reg.idempotent ? " (idempotent)" : ""}`
      : "";
    setSummary(
      `publish OK · ${shortHash(h)} · ${data.stored || "meta"}${regBit} · download started`,
      "pass"
    );
    showError("");
    if (data.meta) downloadPublishJson(data.meta, data.stored);
    if (els.out.lang) {
      els.out.lang.textContent = data.meta ? JSON.stringify(data.meta, null, 2) : "";
    }
    if (els.out.stdout) {
      els.out.stdout.textContent =
        (h ? `source_hash ${h}\n` : "") +
        (reg.id
          ? `rider stub registered id=${reg.id} — GET /api/rider/registered`
          : data.next || "publish metadata downloaded");
    }
    selectTab("lang");
    await refreshBooks();
  } catch (e) {
    setStatus("fail", "error");
    setStamp("fail", "FAIL", "request");
    showError(String(e));
  } finally {
    running = false;
    actionButtons().forEach((b) => {
      b.disabled = false;
    });
    void loadHealth();
  }
}

function wire() {
  document.querySelectorAll(".mode-tab").forEach((t) => {
    t.addEventListener("click", () => {
      setMode(t.dataset.mode);
      if (t.dataset.mode === "agent") void loadAgentSkills();
    });
  });

  els.run.addEventListener("click", () => void invoke("/api/run", "exactness"));
  if (els.exec) {
    els.exec.addEventListener("click", () => void invoke("/api/exec", "interpreter"));
  }
  els.emit.addEventListener("click", () => void invoke("/api/emit", "emitting"));
  els.check.addEventListener("click", () => void invoke("/api/check", "checking"));
  if (els.publish) {
    els.publish.addEventListener("click", () => void publishToRider());
  }
  els.source.addEventListener("keydown", (ev) => {
    if ((ev.metaKey || ev.ctrlKey) && ev.key === "Enter") {
      ev.preventDefault();
      if (mode === "agent") void agentRun();
      else void invoke("/api/run", "exactness");
    }
  });
  els.source.addEventListener("input", () => {
    window.clearTimeout(hashTimer);
    hashTimer = window.setTimeout(() => void refreshLiveHash(), 280);
  });
  els.example.addEventListener("change", () => {
    const id = els.example.value;
    if (!id) {
      els.source.value = DEFAULT_SOURCE;
    } else {
      const ex = examples.find((e) => e.id === id);
      if (ex) els.source.value = ex.source;
    }
    void refreshLiveHash();
  });
  document.querySelectorAll(".tab").forEach((t) => {
    t.addEventListener("click", () => selectTab(t.dataset.tab));
  });
  if (els.langPick) {
    els.langPick.addEventListener("change", () => {
      showPickedLang();
      selectTab("lang");
    });
  }
  document.querySelectorAll(".book-tab").forEach((t) => {
    t.addEventListener("click", () => selectBook(t.dataset.book));
  });
  els.bookRefresh.addEventListener("click", () => void refreshBooks());
  if (els.contractsRefresh) {
    els.contractsRefresh.addEventListener("click", () => void loadHealth());
  }
  if (els.hashCopy) {
    els.hashCopy.addEventListener("click", async () => {
      const h = liveHash || (els.sourceHash && els.sourceHash.textContent) || "";
      if (!h || h === "—") return;
      try {
        await navigator.clipboard.writeText(h);
        els.hashCopy.textContent = "Copied";
        window.setTimeout(() => {
          els.hashCopy.textContent = "Copy";
        }, 1200);
      } catch (_) {
        els.hashCopy.textContent = "—";
      }
    });
  }

  els.agentRun.addEventListener("click", () => void agentRun());
  els.agentPropose.addEventListener("click", () => void agentPropose());
  els.agentAdopt.addEventListener("click", () => void agentAdopt());
  els.agentMessage.addEventListener("keydown", (ev) => {
    if (ev.key === "Enter") {
      ev.preventDefault();
      void agentRun();
    }
  });

  els.notelogForm.addEventListener("submit", async (ev) => {
    ev.preventDefault();
    const body = els.notelogInput.value.trim();
    if (!body) return;
    await fetch("/api/notelog", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ body }),
    });
    els.notelogInput.value = "";
    await refreshBooks();
    void loadHealth();
  });

  els.criticForm.addEventListener("submit", async (ev) => {
    ev.preventDefault();
    const body = els.criticInput.value.trim();
    if (!body) return;
    await fetch("/api/criticbook", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        body,
        severity: els.criticSeverity.value,
        category: els.criticCategory.value,
      }),
    });
    els.criticInput.value = "";
    await refreshBooks();
    void loadHealth();
    selectBook("critic");
  });
}

function initEmpty() {
  fillSeatBody("py", undefined, EMPTY.py);
  fillSeatBody("go", undefined, EMPTY.go);
  fillSeatBody("js", undefined, EMPTY.js);
  fillSeatBody("interp", undefined, EMPTY.interp);
  if (els.out.lang) els.out.lang.dataset.empty = EMPTY.lang;
  if (els.out.stdout) els.out.stdout.dataset.empty = EMPTY.log;
  setStamp("idle", "IDLE", "no check");
}

wire();
initEmpty();
void loadHealth();
void loadExamples();
void loadLangCatalog();
void refreshBooks();
void loadAgentSkills();
void refreshLiveHash();
