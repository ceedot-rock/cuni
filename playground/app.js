const $ = (id) => document.getElementById(id);

const els = {
  source: $("source"),
  example: $("example"),
  run: $("run"),
  emit: $("emit"),
  check: $("check"),
  publish: $("publish"),
  status: $("status"),
  error: $("error"),
  summary: $("summary"),
  health: $("health"),
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
    lang: $("out-lang"),
    stdout: $("out-stdout"),
  },
  langPick: $("lang-pick"),
};

let lastLangs = {};
let catalog = [];

let examples = [];
let running = false;
let mode = "play";
let lastProposeSource = "";

const DEFAULT_SOURCE = "def greet(name: str) -> str do\n    ret `hello ${name}`\nend\n\nsay(greet(\"CuNi\"))\nsay(1 + 2 * 3)\n";


function setStatus(kind, text) {
  els.status.className = `badge ${kind}`;
  els.status.textContent = text;
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
      `<div class="book-empty">No registered contracts yet. Run exactness → <strong>Publish</strong> to register into Rider (local stub always; remote when healthy).</div>`;
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
        `<div class="contract-row" title="${hash}">` +
        `<span class="cid">${id}</span>` +
        `<span class="hash">${hash}</span>` +
        `<span class="when">${when}</span>` +
        `<span class="st">${st}</span>` +
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
  for (const row of rows) {
    const o = document.createElement("option");
    o.value = row.file;
    const gate = row.exactness ? " · exactness" : "";
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
  if (els.out.lang) els.out.lang.textContent = text || "(emit to fill this language)";
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

function setOutputs(data) {
  els.out.py.textContent = data.py || "(no emit)";
  els.out.go.textContent = data.go || "(no emit)";
  els.out.js.textContent = data.js || "(no emit)";
  fillLangPick(data.langs || {});
  if (!data.langs || !Object.keys(data.langs).length) {
    if (els.out.lang) els.out.lang.textContent = data.py || "(no emit)";
  }

  const parts = [];
  const stdout = data.stdout || {};
  for (const k of ["py", "go", "js"]) {
    parts.push(`--- ${k} ---`);
    if (stdout[k] !== undefined) {
      parts.push(stdout[k] === "" ? "(empty)" : stdout[k]);
    } else if (data.run_errors && data.run_errors[k]) {
      parts.push(`ERROR: ${data.run_errors[k]}`);
    } else {
      parts.push("(n/a — use Run for stdout)");
    }
    parts.push("");
  }
  if (data.check_log) {
    parts.push("--- cuni check ---");
    parts.push(data.check_log.trim());
  }
  els.out.stdout.textContent = parts.join("\n");
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
    .replace(/&/g, "&")
    .replace(/</g, "<")
    .replace(/>/g, ">")
    .replace(/"/g, "\"");
}

function renderNotelog(entries) {
  const list = [...(entries || [])].reverse();
  if (!list.length) {
    els.notelogList.innerHTML = `<div class="book-empty">No notes yet. Run exactness or add a note.</div>`;
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
    const parts = [
      `cuni: ok`,
      `py: ${j.python ? "ok" : "missing"}`,
      `go: ${j.go ? "ok" : "missing"}`,
      `node: ${j.node ? "ok" : "missing"}`,
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
  // Prefer flagship spend-control for immediate exactness demo
  const preferred = examples.find((e) => e.id === "spend-control") || examples.find((e) => e.id === "full");
  if (preferred) {
    els.example.value = preferred.id;
    els.source.value = preferred.source;
  } else {
    els.source.value = DEFAULT_SOURCE;
  }
}

async function invoke(path, label) {
  if (running) return;
  running = true;
  [els.run, els.emit, els.check].forEach((b) => {
    b.disabled = true;
  });
  setStatus("run", `${label}…`);
  showError("");
  els.summary.textContent = "";
  els.summary.className = "summary mono";

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
    await refreshBooks();
    void loadHealth();

    if (data.phase === "emit" && data.ok) {
      setStatus("pass", "emit ok");
      els.summary.textContent = data.summary || "emit: ok";
      els.summary.classList.add("pass");
    } else if (data.phase === "emit" || data.phase === "compile") {
      setStatus("fail", "emit error");
      showError(data.error || data.summary || "emit failed");
      els.summary.textContent = data.summary || "emit failed";
      els.summary.classList.add("fail");
      selectTab("stdout");
      selectBook("critic");
    } else if (data.exactness === "PASS" || data.ok) {
      setStatus("pass", "exactness PASS");
      showError("");
      els.summary.textContent = data.summary || "exactness: PASS";
      els.summary.classList.add("pass");
    } else {
      setStatus("fail", "exactness FAIL");
      showError(data.error || data.summary || "exactness failed");
      els.summary.textContent = data.summary || "exactness: FAIL";
      els.summary.classList.add("fail");
      selectTab("stdout");
      selectBook("critic");
    }
  } catch (e) {
    setStatus("fail", "error");
    showError(String(e));
    els.summary.textContent = "request failed";
    els.summary.classList.add("fail");
  } finally {
    running = false;
    [els.run, els.emit, els.check].forEach((b) => {
      b.disabled = false;
    });
  }
}

function setMode(next) {
  mode = next;
  document.querySelectorAll(".mode-tab").forEach((t) => {
    t.classList.toggle("active", t.dataset.mode === next);
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
      py: data.py,
      go: data.go,
      js: data.js,
      stdout: { py: data.stdout, go: data.stdout, js: data.stdout },
      check_log:
        (data.check_log || "") +
        (data.host_tool
          ? "\n--- host tool ---\n" + JSON.stringify(data.host_tool, null, 2)
          : "") +
        (data.run_error ? "\n--- run error ---\n" + data.run_error : ""),
    });
    await refreshBooks();
    if (data.ok && data.exactness === "PASS") {
      setStatus("pass", `agent ${data.skill}`);
      els.summary.textContent = data.summary || "skill PASS";
      els.summary.className = "summary mono pass";
      selectTab("stdout");
    } else {
      setStatus("fail", "agent refused");
      {
        const raw = data.error || data.summary || "agent refused";
        const hint =
          /busy|concurrent/i.test(String(raw))
            ? " — server busy (soft limit: max concurrent runs); retry shortly"
            : /timeout/i.test(String(raw))
              ? " — timed out; try a shorter speech or retry"
              : /exactness|FAIL/i.test(String(raw))
                ? " — exactness gate refused the generated law"
                : "";
        showError(String(raw) + hint);
      }
      els.summary.textContent = data.summary || "FAIL";
      els.summary.className = "summary mono fail";
      selectTab("stdout");
      selectBook("critic");
    }
  } catch (e) {
    setStatus("fail", "error");
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
      py: "",
      go: "",
      js: "",
      stdout: {},
      check_log: data.check_log || data.error || "",
    });
    if (data.ok) {
      setStatus("pass", "propose PASS");
      els.summary.textContent = `quarantine ${data.quarantine_id} — exactness PASS (adopt optional)`;
      els.summary.className = "summary mono pass";
    } else {
      setStatus("fail", "propose FAIL");
      showError(data.error || data.summary || "exactness FAIL — not a citizen");
      els.summary.textContent = data.summary || "refuse";
      els.summary.className = "summary mono fail";
      selectBook("critic");
    }
    selectTab("stdout");
  } catch (e) {
    setStatus("fail", "error");
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
      setStatus("pass", "adopted");
      els.summary.textContent = `adopted ${data.meta?.name} — citizen`;
      els.summary.className = "summary mono pass";
    } else {
      setStatus("fail", "not adopted");
      showError(data.error || "exactness FAIL — refuse adopt");
      els.summary.className = "summary mono fail";
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
  setStatus("busy", "publishing");
  showError("");
  try {
    const r = await fetch("/api/publish", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ source: els.source.value }),
    });
    const data = await r.json();
    if (!r.ok || !data.ok) {
      setStatus("fail", "publish refused");
      showError(data.error || data.exactness || "publish failed");
      els.summary.textContent = data.exactness || data.error || "FAIL";
      els.summary.className = "summary mono fail";
      return;
    }
    setStatus("ok", "published");
    const h = (data.meta && data.meta.sourceHash) || "";
    const reg = data.registration || {};
    const regBit = reg.id
      ? ` · registered ${reg.id}${reg.idempotent ? " (idempotent)" : ""}`
      : "";
    els.summary.textContent = `publish OK · ${h.slice(0, 12)}… · ${data.stored || "meta"}${regBit} · download started`;
    els.summary.className = "summary mono pass";
    showError("");
    // Client-side download of .publish.json (server already stores under /data/published)
    if (data.meta) downloadPublishJson(data.meta, data.stored);
    setOutputs({
      py: data.meta ? JSON.stringify(data.meta, null, 2) : "",
      go: data.registration
        ? JSON.stringify(data.registration, null, 2)
        : data.next || "",
      js: data.docs || "",
      stdout: {
        py: "publish metadata JSON (downloaded as .publish.json + Python tab)",
        go: reg.id
          ? `rider stub registered id=${reg.id} — GET /api/rider/registered`
          : data.next || "",
        js: "",
      },
    });
    selectTab("lang");
    await refreshBooks();
  } catch (e) {
    setStatus("fail", "error");
    showError(String(e));
  } finally {
    running = false;
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

  els.run.addEventListener("click", () => void invoke("/api/run", "running"));
  els.emit.addEventListener("click", () => void invoke("/api/emit", "emitting"));
  els.check.addEventListener("click", () => void invoke("/api/check", "checking"));
  if (els.publish) {
    els.publish.addEventListener("click", () => void publishToRider());
  }
  els.source.addEventListener("keydown", (ev) => {
    if ((ev.metaKey || ev.ctrlKey) && ev.key === "Enter") {
      ev.preventDefault();
      if (mode === "agent") void agentRun();
      else void invoke("/api/run", "running");
    }
  });
  els.example.addEventListener("change", () => {
    const id = els.example.value;
    if (!id) {
      els.source.value = DEFAULT_SOURCE;
      return;
    }
    const ex = examples.find((e) => e.id === id);
    if (ex) els.source.value = ex.source;
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

wire();
void loadHealth();
void loadExamples();
void loadLangCatalog();
void refreshBooks();
void loadAgentSkills();
