const $ = (id) => document.getElementById(id);

const els = {
  source: $("source"),
  example: $("example"),
  run: $("run"),
  status: $("status"),
  error: $("error"),
  summary: $("summary"),
  health: $("health"),
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
      parts.push("(n/a)");
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

async function loadHealth() {
  try {
    const r = await fetch("/api/health");
    const j = await r.json();
    if (!j.ok) {
      els.health.textContent = `toolchain: cuni missing — ${j.error || "build with cargo"}`;
      return;
    }
    const bits = [
      `cuni: ${j.cuni}`,
      `py: ${j.python ? "ok" : "missing"}`,
      `go: ${j.go ? "ok" : "missing"}`,
      `node: ${j.node ? "ok" : "missing"}`,
    ];
    els.health.textContent = bits.join(" · ");
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
  // Prefer full.cuni if present
  const full = examples.find((e) => e.id === "full");
  if (full) {
    els.example.value = "full";
    els.source.value = full.source;
  } else {
    els.source.value = DEFAULT_SOURCE;
  }
}

async function run() {
  if (running) return;
  running = true;
  els.run.disabled = true;
  setStatus("run", "running…");
  showError("");
  els.summary.textContent = "";
  els.summary.className = "summary mono";

  try {
    const r = await fetch("/api/run", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ source: els.source.value }),
    });
    const data = await r.json();
    if (!r.ok && !data.py && !data.error) {
      throw new Error(data.error || `HTTP ${r.status}`);
    }

    setOutputs(data);

    if (data.phase === "compile" || (data.error && !data.py && !data.go)) {
      setStatus("fail", "compile error");
      showError(data.error || data.summary || "compile failed");
      els.summary.textContent = data.summary || "compile failed";
      els.summary.classList.add("fail");
      selectTab("stdout");
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
    }
  } catch (e) {
    setStatus("fail", "error");
    showError(String(e));
    els.summary.textContent = "request failed";
    els.summary.classList.add("fail");
  } finally {
    running = false;
    els.run.disabled = false;
  }
}

function wire() {
  els.run.addEventListener("click", () => void run());
  els.source.addEventListener("keydown", (ev) => {
    if ((ev.metaKey || ev.ctrlKey) && ev.key === "Enter") {
      ev.preventDefault();
      void run();
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
}

wire();
void loadHealth();
void loadExamples();
