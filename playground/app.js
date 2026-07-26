const $ = (id) => document.getElementById(id);

const els = {
  source: $("source"),
  example: $("example"),
  run: $("run"),
  emit: $("emit"),
  check: $("check"),
  status: $("status"),
  error: $("error"),
  summary: $("summary"),
  health: $("health"),
  notelogList: $("notelog-list"),
  criticList: $("critic-list"),
  notelogForm: $("notelog-form"),
  notelogInput: $("notelog-input"),
  criticForm: $("critic-form"),
  criticInput: $("critic-input"),
  criticSeverity: $("critic-severity"),
  criticCategory: $("critic-category"),
  bookRefresh: $("book-refresh"),
  out: {
    py: $("out-py"),
    go: $("out-go"),
    js: $("out-js"),
    stdout: $("out-stdout"),
  },
};

let examples = [];
let running = false;

const DEFAULT_SOURCE = `def greet(name: str) -> str do
    ret \`hello \${name}\`
end

say(greet("CuNi"))
say(1 + 2 * 3)
`;

function setStatus(kind, text) {
  els.status.className = `badge ${kind}`;
  els.status.textContent = text;
}

function showError(msg) {
  if (!msg) {
    els.error.classList.add("hidden");
    els.error.textContent = "";
    return;
  }
  els.error.classList.remove("hidden");
  els.error.textContent = msg;
}

function setOutputs(data) {
  els.out.py.textContent = data.py || "(no emit)";
  els.out.go.textContent = data.go || "(no emit)";
  els.out.js.textContent = data.js || "(no emit)";

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
  const map = { py: "out-py", go: "out-go", js: "out-js", stdout: "out-stdout" };
  $(map[name]).classList.add("active");
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
    const r = await fetch("/api/health");
    const j = await r.json();
    if (!j.ok) {
      els.health.textContent = `toolchain: cuni missing — ${j.error || "build with cargo"}`;
      return;
    }
    els.health.textContent = [
      `cuni: ok`,
      `py: ${j.python ? "ok" : "missing"}`,
      `go: ${j.go ? "ok" : "missing"}`,
      `node: ${j.node ? "ok" : "missing"}`,
      `notes: ${j.books?.notelog ?? 0}`,
      `critiques: ${j.books?.critic ?? 0}`,
    ].join(" · ");
  } catch (e) {
    els.health.textContent = `health check failed: ${e}`;
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
  const full = examples.find((e) => e.id === "full");
  if (full) {
    els.example.value = "full";
    els.source.value = full.source;
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
      els.summary.textContent = data.summary || "exactness: PASS (py/go/js)";
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

function wire() {
  els.run.addEventListener("click", () => void invoke("/api/run", "running"));
  els.emit.addEventListener("click", () => void invoke("/api/emit", "emitting"));
  els.check.addEventListener("click", () => void invoke("/api/check", "checking"));
  els.source.addEventListener("keydown", (ev) => {
    if ((ev.metaKey || ev.ctrlKey) && ev.key === "Enter") {
      ev.preventDefault();
      void invoke("/api/run", "running");
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
  document.querySelectorAll(".book-tab").forEach((t) => {
    t.addEventListener("click", () => selectBook(t.dataset.book));
  });
  els.bookRefresh.addEventListener("click", () => void refreshBooks());

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
void refreshBooks();
