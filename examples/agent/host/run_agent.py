#!/usr/bin/env python3
"""Thin host: speech → CuNi skill (generated entry + modules) → exactness → effect.

  cargo build --release
  python3 examples/agent/host/run_agent.py --check-all
  python3 examples/agent/host/run_agent.py --message "spend 4 cap 5" --host go
  python3 examples/agent/host/run_agent.py --repl
  python3 examples/agent/host/run_agent.py --loop --message "echo hi" --message "score 3 9"
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
TIMEOUT = int(os.environ.get("CUNI_AGENT_TIMEOUT", "45"))


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
    """Extract simple args from speech: spend 4, cap 5, score 3 9, echo hi, get /path"""
    p: dict = {}
    m = message.strip()

    # spend N  /  budget N
    mo = re.search(r"\b(?:spend|budget|usd|allow)\s+(-?\d+)\b", m, re.I)
    if mo:
        p["usd"] = int(mo.group(1))

    mo = re.search(r"\bcap\s+(-?\d+)\b", m, re.I)
    if mo:
        p["cap"] = int(mo.group(1))
    # "spend 12 cap 5" already handled; "clamp 12 5"
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

    return p


def speech_stub(message: str) -> str:
    m = message.lower()
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


def speech_llm(message: str, skill_ids: list[str]) -> str:
    url = os.environ.get(
        "CUNI_AGENT_LLM_URL", "https://api.openai.com/v1/chat/completions"
    )
    key = os.environ.get("CUNI_AGENT_LLM_KEY") or os.environ.get("OPENAI_API_KEY")
    model = os.environ.get("CUNI_AGENT_LLM_MODEL", "gpt-4o-mini")
    if not key:
        return speech_stub(message)

    system = (
        "Route user intent to ONE CuNi skill id. "
        f"Allowed: {', '.join(skill_ids)}. "
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
        return speech_stub(message)
    except Exception as e:  # noqa: BLE001
        print(f"(LLM fallback: {e})", file=sys.stderr)
        return speech_stub(message)


def build_entry_source(skill_id: str, params: dict, skill: dict) -> str:
    """Prefer static entry file when no params; else generate from lawgen."""
    gen = lawgen.GENERATORS.get(skill_id)
    if gen and (params or skill_id != "mind"):
        # always generate for parametric skills when we have a generator
        if skill_id == "mind" and not params:
            return (AGENT / skill["entry"]).read_text(encoding="utf-8")
        try:
            return gen(**params)
        except TypeError:
            # filter unexpected keys
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
        # also copy module files referenced by generators (use X)
        for extra in ("budget.cuni", "text.cuni", "score.cuni"):
            src = AGENT / extra
            if src.is_file() and not (work / extra).exists():
                # only if entry needs it
                pass
        for mod in skill.get("modules") or []:
            pass  # already copied

        entry_src = build_entry_source(skill_id, params, skill)
        # ensure modules for generated use lines
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
    # parse first line GET /path
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
) -> dict:
    skill_ids = [s["id"] for s in manifest["skills"]]
    params = parse_params(message)
    skill_id = skill_force or (
        speech_llm(message, skill_ids) if use_llm else speech_stub(message)
    )
    skill = skill_by_id(manifest, skill_id)

    print("== speech ==")
    print(f"user:   {message}")
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
    # static entries on disk
    for s in manifest["skills"]:
        entry = AGENT / s["entry"]
        if entry.is_file():
            print(f"\n--- static {s['id']} ({s['entry']}) ---")
            out = exactness_gate(cuni, entry)
            for ln in out.splitlines():
                if "exactness:" in ln:
                    print(f"  {ln.strip()}")
    # generated default params for parametric skills
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
    print("CuNi agent REPL — law is CuNi. Commands: skills | host py|go|js | quit")
    print("Examples: spend 4 cap 5 | echo hi | score 3 9 80 | tag Agent | get /health\n")
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
        try:
            run_turn(
                cuni,
                manifest,
                line,
                host,
                skill_force=None,
                use_llm=use_llm,
                skip_check=False,
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
    args = ap.parse_args()

    if args.list:
        print(json.dumps(manifest, indent=2))
        return

    cuni = find_cuni()
    print("AI runs off CuNi — speech routes; law is exact CuNi\n")
    print(f"cuni: {cuni}")
    print(f"host: {args.host}")
    print(f"skills: {', '.join(skill_ids)}\n")

    if args.check_all:
        check_all(cuni, manifest)
        return

    if args.repl:
        repl(cuni, manifest, args.host, args.llm)
        return

    messages = args.messages or ["run the full agent mind"]
    if not args.loop and len(messages) > 1:
        # still allow multi without flag
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
        )

    print("\n== done ==")
    print("The agent is CuNi that passed exactness — not the chat.")


if __name__ == "__main__":
    main()
