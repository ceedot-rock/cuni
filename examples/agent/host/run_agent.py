#!/usr/bin/env python3
"""Thin host: speech → CuNi skill (generated entry + modules) → exactness → effect.

  cargo build --release
  python3 examples/agent/host/run_agent.py --check-all
  python3 examples/agent/host/run_agent.py --message "spend 4 cap 5" --host go
  python3 examples/agent/host/run_agent.py --repl
  python3 examples/agent/host/run_agent.py --loop --message "echo hi" --message "score 3 9"
  python3 examples/agent/host/run_agent.py --plan --message "spend 3; echo ok; explain exactness"
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path

# allow `import lawgen` when run as script
sys.path.insert(0, str(Path(__file__).resolve().parent))
import lawgen  # noqa: E402

ROOT = Path(__file__).resolve().parents[3]
AGENT = Path(__file__).resolve().parents[1]
MANIFEST_PATH = AGENT / "manifest.json"
SESSION_DIR = Path(
    os.environ.get("CUNI_AGENT_SESSION_DIR", str(AGENT / "sessions"))
)
QUARANTINE_DIR = Path(
    os.environ.get("CUNI_AGENT_QUARANTINE", str(AGENT / "quarantine"))
)
TIMEOUT = int(os.environ.get("CUNI_AGENT_TIMEOUT", "45"))
MEMORY_TURNS = int(os.environ.get("CUNI_AGENT_MEMORY_TURNS", "6"))


def find_cuni() -> Path:
    env = os.environ.get("CUNI_BIN")
    if env and Path(env).is_file():
        return Path(env)
    for p in (ROOT / "target/release/cuni", ROOT / "target/debug/cuni"):
        if p.is_file():
            return p
    w = shutil.which("cuni")
    if w:
        return Path(w)
    raise SystemExit("cuni not found — cargo build --release or set CUNI_BIN")


def load_manifest() -> dict:
    return json.loads(MANIFEST_PATH.read_text(encoding="utf-8"))


def skill_by_id(manifest: dict, skill_id: str) -> dict:
    for s in manifest.get("skills", []):
        if s["id"] == skill_id:
            return s
    raise SystemExit(
        f"unknown skill `{skill_id}` — known: "
        + ", ".join(s["id"] for s in manifest.get("skills", []))
    )


def run(cmd: list[str], cwd: Path | None = None) -> subprocess.CompletedProcess:
    return subprocess.run(
        cmd,
        cwd=str(cwd) if cwd else None,
        capture_output=True,
        text=True,
        timeout=TIMEOUT,
    )


def exactness_gate(cuni: Path, entry: Path) -> str:
    r = run([str(cuni), "check", str(entry), "--timeout", str(TIMEOUT)])
    out = (r.stdout or "") + (r.stderr or "")
    if r.returncode != 0 or "exactness: PASS" not in out:
        raise SystemExit(
            f"REFUSE: `{entry.name}` failed exactness\n{out}"
        )
    return out


def parse_params(message: str) -> dict:
    """Extract simple args from speech."""
    p: dict = {}
    m = message.strip()

    mo = re.search(r"\b(?:spend|budget|usd|allow)\s+(-?\d+)\b", m, re.I)
    if mo:
        p["usd"] = int(mo.group(1))

    mo = re.search(r"\bcap\s+(-?\d+)\b", m, re.I)
    if mo:
        p["cap"] = int(mo.group(1))
    mo = re.search(r"\bclamp\s+(-?\d+)\s+(-?\d+)\b", m, re.I)
    if mo:
        p["usd"] = int(mo.group(1))
        p["cap"] = int(mo.group(2))

    mo = re.search(r"\bscore\s+(-?\d+)(?:\s+(-?\d+))?(?:\s+(-?\d+))?\b", m, re.I)
    if mo:
        p["a"] = int(mo.group(1))
        if mo.group(2) is not None:
            p["b"] = int(mo.group(2))
        if mo.group(3) is not None:
            p["n"] = int(mo.group(3))

    mo = re.search(r"\b(?:echo|ping)\s+(\S+)", m, re.I)
    if mo:
        p["msg"] = mo.group(1)
    elif re.search(r"\becho\b|\bping\b", m, re.I):
        p["msg"] = "ping"

    mo = re.search(r"\b(?:get|fetch|path)\s+(\S+)", m, re.I)
    if mo:
        p["path"] = mo.group(1)

    mo = re.search(r"\btag\s+(\S+)", m, re.I)
    if mo:
        p["name"] = mo.group(1)

    mo = re.search(r"\bjoin\s+(\S+)\s+(\S+)", m, re.I)
    if mo:
        p["a"] = mo.group(1)
        p["b"] = mo.group(2)

    # learn topics
    mo = re.search(
        r"\b(?:explain|what is|whats|what's|teach|lesson|learn)\s+(\w+)",
        m,
        re.I,
    )
    if mo:
        p["topic"] = mo.group(1).lower()
    else:
        for t in (
            "exactness",
            "let",
            "mut",
            "def",
            "link",
            "typ",
            "fail",
            "do",
            "enum",
            "iface",
        ):
            if re.search(rf"\b{t}\b", m, re.I) and any(
                w in m.lower()
                for w in ("explain", "what", "teach", "learn", "lesson", "mean")
            ):
                p["topic"] = t
                break

    return p


def speech_stub(message: str, mode: str = "execute") -> str:
    m = message.lower()
    if mode == "learn" or any(
        w in m for w in ("explain", "what is", "whats", "teach", "lesson", "learn")
    ):
        return "learn"
    if any(w in m for w in ("get ", "fetch ", "path ", "plan_get", "http")):
        return "tool_plan_get"
    if any(w in m for w in ("echo", "ping", "tool")):
        return "tool_echo"
    if any(w in m for w in ("spend", "budget", "money", "cap", "usd", "allow", "clamp")):
        return "budget"
    if any(w in m for w in ("join", "tag", "text", "speak", "string")):
        return "text"
    if any(w in m for w in ("score", "rank", "prefer")):
        return "score"
    if any(w in m for w in ("full", "mind", "all", "identity")):
        return "mind"
    return "mind"


def speech_llm(
    message: str,
    skill_ids: list[str],
    *,
    mode: str = "execute",
    memory: list[dict] | None = None,
) -> str:
    url = os.environ.get(
        "CUNI_AGENT_LLM_URL", "https://api.openai.com/v1/chat/completions"
    )
    key = os.environ.get("CUNI_AGENT_LLM_KEY") or os.environ.get("OPENAI_API_KEY")
    model = os.environ.get("CUNI_AGENT_LLM_MODEL", "gpt-4o-mini")
    if not key:
        return speech_stub(message, mode=mode)

    mem_lines = []
    for rec in (memory or [])[-MEMORY_TURNS:]:
        mem_lines.append(
            f"user:{rec.get('message', '')} → skill:{rec.get('skill', '')}"
        )
    mem_block = "\n".join(mem_lines) if mem_lines else "(none)"

    system = (
        "Route user intent to ONE CuNi skill id. "
        f"Allowed: {', '.join(skill_ids)}. "
        f"Chat mode: {mode}. If mode is learn, prefer skill `learn`. "
        "Recent session memory:\n"
        f"{mem_block}\n"
        "Reply with only the id."
    )
    body = {
        "model": model,
        "temperature": 0,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": message},
        ],
    }
    req = urllib.request.Request(
        url,
        data=json.dumps(body).encode(),
        headers={
            "Content-Type": "application/json",
            "Authorization": f"Bearer {key}",
        },
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
            data = json.loads(resp.read().decode())
        text = data["choices"][0]["message"]["content"].strip().lower()
        for name in skill_ids:
            if text == name or name in text.replace("`", " ").split():
                return name
        return speech_stub(message, mode=mode)
    except Exception as e:  # noqa: BLE001
        print(f"(LLM fallback: {e})", file=sys.stderr)
        return speech_stub(message, mode=mode)


def plan_messages(message: str) -> list[str]:
    """Split a goal into ordered speech steps.

    Stub planner: split on `;` or ` then ` / ` and then `.
    LLM path (optional): if CUNI_AGENT_LLM_KEY set, ask for JSON list of steps.
    """
    key = os.environ.get("CUNI_AGENT_LLM_KEY") or os.environ.get("OPENAI_API_KEY")
    if key and len(message) > 12 and ";" not in message:
        url = os.environ.get(
            "CUNI_AGENT_LLM_URL", "https://api.openai.com/v1/chat/completions"
        )
        model = os.environ.get("CUNI_AGENT_LLM_MODEL", "gpt-4o-mini")
        system = (
            "You are a planner for CuNi agent skills. "
            "Break the user goal into 1-5 short executable speech steps. "
            "Each step should map to one skill (budget/text/score/learn/echo/get). "
            "Reply ONLY a JSON array of strings. Example: "
            '["spend 3 cap 5", "echo ok", "explain exactness"]'
        )
        body = {
            "model": model,
            "temperature": 0,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": message},
            ],
        }
        req = urllib.request.Request(
            url,
            data=json.dumps(body).encode(),
            headers={
                "Content-Type": "application/json",
                "Authorization": f"Bearer {key}",
            },
            method="POST",
        )
        try:
            with urllib.request.urlopen(req, timeout=60) as resp:
                data = json.loads(resp.read().decode())
            text = data["choices"][0]["message"]["content"].strip()
            # extract JSON array
            mo = re.search(r"\[.*\]", text, re.S)
            if mo:
                steps = json.loads(mo.group(0))
                if isinstance(steps, list) and steps:
                    return [str(s).strip() for s in steps if str(s).strip()][:5]
        except Exception as e:  # noqa: BLE001
            print(f"(plan LLM fallback: {e})", file=sys.stderr)

    # deterministic split
    parts = re.split(r"\s*;\s*|\s+then\s+|\s+and then\s+", message, flags=re.I)
    steps = [p.strip() for p in parts if p.strip()]
    return steps if steps else [message]


def build_entry_source(skill_id: str, params: dict, skill: dict) -> str:
    """Prefer static entry file when no params; else generate from lawgen."""
    gen = lawgen.GENERATORS.get(skill_id)
    if gen and (params or skill_id != "mind"):
        if skill_id == "mind" and not params:
            return (AGENT / skill["entry"]).read_text(encoding="utf-8")
        try:
            return gen(**params)
        except TypeError:
            import inspect

            sig = inspect.signature(gen)
            filtered = {k: v for k, v in params.items() if k in sig.parameters}
            return gen(**filtered)
    path = AGENT / skill["entry"]
    return path.read_text(encoding="utf-8")


def stage_and_run(
    cuni: Path,
    skill: dict,
    skill_id: str,
    params: dict,
    host: str,
    *,
    skip_check: bool,
) -> tuple[str, str, Path]:
    """Returns (stdout, check_log, workdir_kept_if_any)."""
    work = Path(tempfile.mkdtemp(prefix="cuni_agent_"))
    try:
        for mod in skill.get("modules") or []:
            shutil.copy(AGENT / mod, work / mod)

        entry_src = build_entry_source(skill_id, params, skill)
        for name in re.findall(r"(?m)^\s*use\s+([A-Za-z_][A-Za-z0-9_]*)\s*$", entry_src):
            mod = AGENT / f"{name}.cuni"
            if mod.is_file():
                shutil.copy(mod, work / f"{name}.cuni")

        main = work / "entry.cuni"
        main.write_text(entry_src, encoding="utf-8")

        check_log = ""
        if not skip_check:
            check_log = exactness_gate(cuni, main)

        py, go, js = work / "out.py", work / "out.go", work / "out.js"
        em = run(
            [
                str(cuni),
                str(main),
                "--emit-py",
                str(py),
                "--emit-go",
                str(go),
                "--emit-js",
                str(js),
            ]
        )
        if em.returncode != 0:
            raise SystemExit((em.stderr or em.stdout or "emit failed").strip())

        cmd = {
            "py": ["python3", str(py)],
            "go": ["go", "run", str(go)],
            "js": ["node", str(js)],
        }[host]
        r = run(cmd, cwd=work)
        if r.returncode != 0:
            raise SystemExit((r.stderr or r.stdout or f"{host} failed").strip())
        return r.stdout, check_log, work
    except Exception:
        shutil.rmtree(work, ignore_errors=True)
        raise


def host_side_effects(skill_id: str, stdout: str, params: dict) -> str | None:
    """Optional non-exact host tools after law (e.g. real HTTP)."""
    if skill_id != "tool_plan_get":
        return None
    line = (stdout.strip().splitlines() or [""])[0].strip()
    mo = re.match(r"GET\s+(\S+)", line)
    if not mo:
        return None
    path = mo.group(1)
    base = os.environ.get("CUNI_AGENT_HTTP_BASE", "").rstrip("/")
    if not base:
        return f"(host) plan only — set CUNI_AGENT_HTTP_BASE to fetch {path}"
    url = base + (path if path.startswith("/") else "/" + path)
    try:
        req = urllib.request.Request(url, method="GET")
        with urllib.request.urlopen(req, timeout=15) as resp:
            body = resp.read()[:500].decode("utf-8", errors="replace")
        return f"(host) GET {url} → {resp.status}\n{body}"
    except Exception as e:  # noqa: BLE001
        return f"(host) GET {url} failed: {e}"


def append_session(record: dict) -> Path:
    SESSION_DIR.mkdir(parents=True, exist_ok=True)
    day = time.strftime("%Y-%m-%d")
    path = SESSION_DIR / f"session-{day}.jsonl"
    with path.open("a", encoding="utf-8") as f:
        f.write(json.dumps(record) + "\n")
    return path


def load_recent_memory(limit: int | None = None) -> list[dict]:
    """Load recent session turns for speech context."""
    limit = limit or MEMORY_TURNS
    SESSION_DIR.mkdir(parents=True, exist_ok=True)
    files = sorted(SESSION_DIR.glob("session-*.jsonl"))
    rows: list[dict] = []
    for path in files[-3:]:
        try:
            for line in path.read_text(encoding="utf-8").splitlines():
                if line.strip():
                    rows.append(json.loads(line))
        except (OSError, json.JSONDecodeError):
            continue
    return rows[-limit:]


def quarantine_source(source: str, label: str = "propose") -> Path:
    """Write proposed law into quarantine dir (not live skills)."""
    QUARANTINE_DIR.mkdir(parents=True, exist_ok=True)
    ts = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
    safe = re.sub(r"[^a-zA-Z0-9_]", "_", label)[:32] or "propose"
    path = QUARANTINE_DIR / f"{ts}_{safe}.cuni"
    path.write_text(source, encoding="utf-8")
    meta = {
        "path": str(path),
        "ts": ts,
        "label": label,
        "status": "quarantine",
    }
    path.with_suffix(".json").write_text(json.dumps(meta, indent=2), encoding="utf-8")
    return path


def run_turn(
    cuni: Path,
    manifest: dict,
    message: str,
    host: str,
    *,
    skill_force: str | None,
    use_llm: bool,
    skip_check: bool,
    quiet_check: bool = False,
    mode: str = "execute",
) -> dict:
    skill_ids = [s["id"] for s in manifest["skills"]]
    memory = load_recent_memory()
    params = parse_params(message)
    skill_id = skill_force or (
        speech_llm(message, skill_ids, mode=mode, memory=memory)
        if use_llm
        else speech_stub(message, mode=mode)
    )
    skill = skill_by_id(manifest, skill_id)

    print("== speech ==")
    print(f"user:   {message}")
    print(f"mode:   {mode}")
    print(f"route:  {skill_id}")
    print(f"params: {params or '{}'}")
    print(f"law:    {skill.get('description', skill_id)}\n")

    if not skip_check and not quiet_check:
        print("== law gate ==")
    stdout, check_log, work = stage_and_run(
        cuni, skill, skill_id, params, host, skip_check=skip_check
    )
    shutil.rmtree(work, ignore_errors=True)

    if not skip_check:
        for ln in check_log.splitlines():
            if "exactness:" in ln:
                print(f"  {ln.strip()}")
        print("OK: citizen\n")

    print(f"== law effect ({host}) ==")
    print(stdout.rstrip())

    side = host_side_effects(skill_id, stdout, params)
    if side:
        print("\n== host tool ==")
        print(side)

    rec = {
        "ts": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "message": message,
        "mode": mode,
        "skill": skill_id,
        "params": params,
        "host": host,
        "stdout": stdout,
        "host_tool": side,
        "exactness": "PASS" if not skip_check else "skipped",
    }
    sp = append_session(rec)
    print(f"\n(session log: {sp})")
    return rec


def check_all(cuni: Path, manifest: dict) -> None:
    print("== check-all (static entries + generated defaults) ==")
    for s in manifest["skills"]:
        entry = AGENT / s["entry"]
        if entry.is_file():
            print(f"\n--- static {s['id']} ({s['entry']}) ---")
            out = exactness_gate(cuni, entry)
            for ln in out.splitlines():
                if "exactness:" in ln:
                    print(f"  {ln.strip()}")
    print("\n--- generated defaults ---")
    for skill_id, gen in lawgen.GENERATORS.items():
        if skill_id == "mind":
            continue
        skill = skill_by_id(manifest, skill_id)
        work = Path(tempfile.mkdtemp(prefix="cuni_chk_"))
        try:
            src = gen()
            for name in re.findall(r"(?m)^\s*use\s+([A-Za-z_][A-Za-z0-9_]*)\s*$", src):
                mod = AGENT / f"{name}.cuni"
                if mod.is_file():
                    shutil.copy(mod, work / f"{name}.cuni")
            main = work / "entry.cuni"
            main.write_text(src, encoding="utf-8")
            print(f"\n--- gen {skill_id} ---")
            out = exactness_gate(cuni, main)
            for ln in out.splitlines():
                if "exactness:" in ln:
                    print(f"  {ln.strip()}")
        finally:
            shutil.rmtree(work, ignore_errors=True)
    print("\nOK: all skills citizens")


def repl(cuni: Path, manifest: dict, host: str, use_llm: bool) -> None:
    print("CuNi agent REPL — law is CuNi. Commands: skills | host py|go|js | mode learn|execute|code | quit")
    print("Examples: spend 4 cap 5 | echo hi | explain exactness | score 3 9 80\n")
    mode = "execute"
    while True:
        try:
            line = input("cuni> ").strip()
        except (EOFError, KeyboardInterrupt):
            print()
            break
        if not line:
            continue
        if line in ("quit", "exit", "q"):
            break
        if line == "skills":
            for s in manifest["skills"]:
                print(f"  {s['id']:12} {s.get('description', '')}")
            continue
        if line.startswith("host "):
            h = line.split(None, 1)[1].strip()
            if h in ("py", "go", "js"):
                host = h
                print(f"host = {host}")
            else:
                print("host must be py|go|js")
            continue
        if line.startswith("mode "):
            m = line.split(None, 1)[1].strip()
            if m in ("learn", "execute", "code"):
                mode = m
                print(f"mode = {mode}")
            else:
                print("mode must be learn|execute|code")
            continue
        if mode == "code":
            # treat free text as proposed law body → quarantine after check
            qpath = quarantine_source(line if "def " in line or "say(" in line else f"say(`{line}`)\n")
            print(f"quarantine draft: {qpath}")
            print("(use write_skill.py propose/adopt to promote)")
            continue
        try:
            run_turn(
                cuni,
                manifest,
                line,
                host,
                skill_force=None,
                use_llm=use_llm,
                skip_check=False,
                mode=mode,
            )
            print()
        except SystemExit as e:
            print(e, file=sys.stderr)


def main() -> None:
    manifest = load_manifest()
    skill_ids = [s["id"] for s in manifest["skills"]]

    ap = argparse.ArgumentParser(description="CuNi agent host — speech routes, law is CuNi")
    ap.add_argument("--host", choices=("py", "go", "js"), default="py")
    ap.add_argument(
        "--message",
        action="append",
        dest="messages",
        help="user message (repeat for multi-step loop)",
    )
    ap.add_argument("--skill", choices=skill_ids, default=None)
    ap.add_argument("--llm", action="store_true")
    ap.add_argument("--check-all", action="store_true")
    ap.add_argument("--skip-check", action="store_true")
    ap.add_argument("--list", action="store_true")
    ap.add_argument("--repl", action="store_true")
    ap.add_argument(
        "--loop",
        action="store_true",
        help="run all --message steps in sequence (session log)",
    )
    ap.add_argument(
        "--plan",
        action="store_true",
        help="expand each --message into a multi-step plan then execute",
    )
    ap.add_argument(
        "--mode",
        choices=("execute", "learn", "code"),
        default="execute",
        help="chat mode bias for speech routing",
    )
    args = ap.parse_args()

    if args.list:
        print(json.dumps(manifest, indent=2))
        return

    cuni = find_cuni()
    print("AI runs off CuNi — speech routes; law is exact CuNi\n")
    print(f"cuni: {cuni}")
    print(f"host: {args.host}")
    print(f"mode: {args.mode}")
    print(f"skills: {', '.join(skill_ids)}\n")

    if args.check_all:
        check_all(cuni, manifest)
        return

    if args.repl:
        repl(cuni, manifest, args.host, args.llm)
        return

    messages = args.messages or ["run the full agent mind"]
    if args.plan:
        expanded: list[str] = []
        for msg in messages:
            steps = plan_messages(msg)
            print(f"== plan for: {msg!r} ==")
            for i, s in enumerate(steps, 1):
                print(f"  {i}. {s}")
            print()
            expanded.extend(steps)
        messages = expanded
        args.loop = True

    if not args.loop and len(messages) > 1:
        args.loop = True

    for i, msg in enumerate(messages):
        if len(messages) > 1:
            print(f"\n######## step {i + 1}/{len(messages)} ########\n")
        run_turn(
            cuni,
            manifest,
            msg,
            args.host,
            skill_force=args.skill,
            use_llm=args.llm,
            skip_check=args.skip_check,
            mode=args.mode,
        )

    print("\n== done ==")
    print("The agent is CuNi that passed exactness — not the chat.")


if __name__ == "__main__":
    main()
