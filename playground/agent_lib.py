"""Agent pack integration for CuNi Studio.

Speech routes to skills; law is CuNi; exactness is citizenship.
Skill writer: propose → quarantine until PASS → optional adopt.
Chat modes: execute | learn | code.
"""

from __future__ import annotations

import json
import re
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
AGENT = ROOT / "examples" / "agent"
HOST_DIR = AGENT / "host"

if str(HOST_DIR) not in sys.path:
    sys.path.insert(0, str(HOST_DIR))
try:
    import lawgen  # type: ignore
except ImportError:
    lawgen = None  # type: ignore


def agent_available() -> bool:
    return (AGENT / "manifest.json").is_file()


def load_manifest() -> dict:
    p = AGENT / "manifest.json"
    if not p.is_file():
        return {"skills": [], "error": "agent pack missing"}
    return json.loads(p.read_text(encoding="utf-8"))


def list_skills() -> list[dict]:
    m = load_manifest()
    out = []
    for s in m.get("skills", []):
        entry = AGENT / s["entry"]
        out.append(
            {
                "id": s["id"],
                "entry": s["entry"],
                "description": s.get("description", ""),
                "modules": s.get("modules") or [],
                "has_entry": entry.is_file(),
            }
        )
    return out


def _run(cmd: list[str], cwd: Path | None, timeout: int) -> subprocess.CompletedProcess:
    return subprocess.run(
        cmd,
        cwd=str(cwd) if cwd else None,
        capture_output=True,
        text=True,
        timeout=timeout,
    )


def _stage_modules(work: Path, source: str) -> None:
    for name in re.findall(r"(?m)^\s*use\s+([A-Za-z_][A-Za-z0-9_]*)\s*$", source):
        for base in (AGENT, ROOT / "examples"):
            mod = base / f"{name}.cuni"
            if mod.is_file():
                shutil.copy(mod, work / f"{name}.cuni")
                break


def check_source(cuni: Path, source: str, timeout: int) -> dict:
    with tempfile.TemporaryDirectory(prefix="cuni_prop_") as td:
        work = Path(td)
        main = work / "main.cuni"
        main.write_text(source, encoding="utf-8")
        _stage_modules(work, source)
        r = _run([str(cuni), "check", str(main), "--timeout", str(timeout)], work, timeout)
        out = (r.stdout or "") + (r.stderr or "")
        out = out.replace(str(main), "main.cuni")
        ok = r.returncode == 0 and "exactness: PASS" in out
        return {
            "ok": ok,
            "exactness": "PASS" if ok else "FAIL",
            "check_log": out,
            "summary": next(
                (ln for ln in out.splitlines() if "exactness:" in ln),
                "exactness: FAIL" if not ok else "exactness: PASS",
            ),
        }


def run_skill(
    cuni: Path,
    skill_id: str,
    *,
    params: dict | None = None,
    host: str = "py",
    timeout: int = 45,
    http_base: str | None = None,
) -> dict:
    params = params or {}
    manifest = load_manifest()
    skill = None
    for s in manifest.get("skills", []):
        if s["id"] == skill_id:
            skill = s
            break
    if not skill:
        return {"ok": False, "error": f"unknown skill {skill_id}"}

    if lawgen and skill_id in getattr(lawgen, "GENERATORS", {}):
        gen = lawgen.GENERATORS[skill_id]
        try:
            import inspect

            sig = inspect.signature(gen)
            filtered = {k: v for k, v in params.items() if k in sig.parameters}
            source = gen(**filtered) if filtered or skill_id != "mind" else gen()
            if skill_id == "mind" and not params:
                source = (AGENT / skill["entry"]).read_text(encoding="utf-8")
            gen_err = None
        except Exception as e:  # noqa: BLE001
            source = (AGENT / skill["entry"]).read_text(encoding="utf-8")
            gen_err = str(e)
    else:
        source = (AGENT / skill["entry"]).read_text(encoding="utf-8")
        gen_err = None

    with tempfile.TemporaryDirectory(prefix="cuni_ag_") as td:
        work = Path(td)
        for mod in skill.get("modules") or []:
            src = AGENT / mod
            if src.is_file():
                shutil.copy(src, work / mod)
        _stage_modules(work, source)
        main = work / "entry.cuni"
        main.write_text(source, encoding="utf-8")

        chk = _run(
            [str(cuni), "check", str(main), "--timeout", str(timeout)], work, timeout
        )
        check_log = ((chk.stdout or "") + (chk.stderr or "")).replace(
            str(main), "entry.cuni"
        )
        if chk.returncode != 0 or "exactness: PASS" not in check_log:
            return {
                "ok": False,
                "phase": "check",
                "skill": skill_id,
                "params": params,
                "source": source,
                "exactness": "FAIL",
                "check_log": check_log,
                "error": "skill failed exactness — refuse",
                "summary": next(
                    (ln for ln in check_log.splitlines() if "exactness:" in ln),
                    "exactness: FAIL",
                ),
            }

        py, go, js = work / "out.py", work / "out.go", work / "out.js"
        em = _run(
            [
                str(cuni),
                str(main),
                "--emit-py",
                str(py),
                "--emit-go",
                str(go),
                "--emit-js",
                str(js),
            ],
            work,
            timeout,
        )
        if em.returncode != 0:
            return {
                "ok": False,
                "phase": "emit",
                "skill": skill_id,
                "error": (em.stderr or em.stdout or "emit failed").strip(),
                "check_log": check_log,
            }

        cmd = {
            "py": ["python3", str(py)],
            "go": ["go", "run", str(go)],
            "js": ["node", str(js)],
        }.get(host, ["python3", str(py)])
        rr = _run(cmd, work, timeout)
        stdout = rr.stdout if rr.returncode == 0 else ""
        run_err = None if rr.returncode == 0 else (rr.stderr or rr.stdout or "run fail")

        host_tool = None
        if skill_id == "tool_plan_get" and http_base and stdout:
            line = stdout.strip().splitlines()[0] if stdout.strip() else ""
            mo = re.match(r"GET\s+(\S+)", line.strip())
            if mo:
                path = mo.group(1)
                url = http_base.rstrip("/") + (
                    path if path.startswith("/") else "/" + path
                )
                try:
                    req = urllib.request.Request(url, method="GET")
                    with urllib.request.urlopen(req, timeout=15) as resp:
                        body = resp.read()[:800].decode("utf-8", errors="replace")
                    host_tool = {
                        "url": url,
                        "status": resp.status,
                        "body": body,
                    }
                except Exception as e:  # noqa: BLE001
                    host_tool = {"url": url, "error": str(e)}

        return {
            "ok": run_err is None,
            "phase": "run",
            "skill": skill_id,
            "params": params,
            "source": source,
            "exactness": "PASS",
            "check_log": check_log,
            "stdout": stdout,
            "run_error": run_err,
            "host": host,
            "host_tool": host_tool,
            "py": py.read_text(encoding="utf-8") if py.is_file() else "",
            "go": go.read_text(encoding="utf-8") if go.is_file() else "",
            "js": js.read_text(encoding="utf-8") if js.is_file() else "",
            "summary": f"skill `{skill_id}` exactness PASS on {host}",
            "gen_error": gen_err,
        }


def parse_message_params(message: str, mode: str = "execute") -> tuple[str | None, dict]:
    """Return (hint_skill, params) from free text."""
    p: dict = {}
    m = message.strip()
    mo = re.search(r"\b(?:spend|budget|usd|allow)\s+(-?\d+)\b", m, re.I)
    if mo:
        p["usd"] = int(mo.group(1))
    mo = re.search(r"\bcap\s+(-?\d+)\b", m, re.I)
    if mo:
        p["cap"] = int(mo.group(1))
    mo = re.search(r"\bscore\s+(-?\d+)(?:\s+(-?\d+))?(?:\s+(-?\d+))?\b", m, re.I)
    if mo:
        p["a"] = int(mo.group(1))
        if mo.group(2):
            p["b"] = int(mo.group(2))
        if mo.group(3):
            p["n"] = int(mo.group(3))
    mo = re.search(r"\b(?:echo|ping)\s+(\S+)", m, re.I)
    if mo:
        p["msg"] = mo.group(1)
    mo = re.search(r"\b(?:get|fetch|path)\s+(\S+)", m, re.I)
    if mo:
        p["path"] = mo.group(1)
    mo = re.search(r"\btag\s+(\S+)", m, re.I)
    if mo:
        p["name"] = mo.group(1)

    mo = re.search(
        r"\b(?:explain|what is|whats|what's|teach|lesson|learn)\s+(\w+)",
        m,
        re.I,
    )
    if mo:
        p["topic"] = mo.group(1).lower()

    ml = m.lower()
    skill = None
    if mode == "learn" or any(
        w in ml for w in ("explain", "what is", "teach", "lesson", "learn ")
    ):
        skill = "learn"
        if "topic" not in p:
            for t in (
                "exactness",
                "let",
                "mut",
                "def",
                "link",
                "typ",
                "fail",
                "do",
            ):
                if re.search(rf"\b{t}\b", ml):
                    p["topic"] = t
                    break
            p.setdefault("topic", "exactness")
    elif any(w in ml for w in ("get ", "fetch ", "path ")):
        skill = "tool_plan_get"
    elif any(w in ml for w in ("echo", "ping")):
        skill = "tool_echo"
    elif any(w in ml for w in ("spend", "budget", "cap", "usd")):
        skill = "budget"
    elif any(w in ml for w in ("tag", "join", "text", "speak")):
        skill = "text"
    elif "score" in ml or "prefer" in ml:
        skill = "score"
    elif "mind" in ml or "full" in ml:
        skill = "mind"
    return skill, p


def plan_steps(message: str) -> list[str]:
    parts = re.split(r"\s*;\s*|\s+then\s+|\s+and then\s+", message, flags=re.I)
    steps = [p.strip() for p in parts if p.strip()]
    return steps if steps else [message]


def chat_turn(
    cuni: Path,
    message: str,
    *,
    mode: str = "execute",
    host: str = "py",
    timeout: int = 45,
    http_base: str | None = None,
    plan: bool = False,
) -> dict:
    """High-level chat: optional multi-step plan → run each skill."""
    steps = plan_steps(message) if plan or ";" in message else [message]
    results = []
    for step in steps:
        hint, params = parse_message_params(step, mode=mode)
        skill = hint or ("learn" if mode == "learn" else "mind")
        if mode == "code":
            # propose path: treat message as source draft
            chk = check_source(cuni, step if "def " in step or "say(" in step else f'say(`{step}`)\n', timeout)
            results.append(
                {
                    "step": step,
                    "mode": "code",
                    "ok": chk.get("ok"),
                    "exactness": chk.get("exactness"),
                    "check_log": chk.get("check_log"),
                    "summary": chk.get("summary"),
                    "note": "quarantine — adopt only after PASS",
                }
            )
            continue
        r = run_skill(
            cuni,
            skill,
            params=params,
            host=host,
            timeout=timeout,
            http_base=http_base,
        )
        r["step"] = step
        r["mode"] = mode
        results.append(r)

    ok = all(x.get("ok") for x in results)
    last = results[-1] if results else {}
    return {
        "ok": ok,
        "mode": mode,
        "steps": results,
        "count": len(results),
        "skill": last.get("skill"),
        "source": last.get("source"),
        "stdout": last.get("stdout"),
        "py": last.get("py"),
        "go": last.get("go"),
        "js": last.get("js"),
        "check_log": last.get("check_log"),
        "exactness": last.get("exactness"),
        "summary": f"chat {mode}: {len(results)} step(s), ok={ok}",
        "host_tool": last.get("host_tool"),
        "error": None if ok else (last.get("error") or "one or more steps failed"),
    }


def adopt_skill(data_dir: Path, name: str, source: str, cuni: Path, timeout: int) -> dict:
    """Save proposed law only if exactness PASS."""
    name = re.sub(r"[^a-zA-Z0-9_]", "_", name.strip())[:40] or "skill"
    chk = check_source(cuni, source, timeout)
    if not chk["ok"]:
        return {
            "ok": False,
            "adopted": False,
            "error": "refuse adopt — exactness FAIL",
            **chk,
        }
    dest_dir = data_dir / "adopted_skills"
    dest_dir.mkdir(parents=True, exist_ok=True)
    path = dest_dir / f"{name}.cuni"
    path.write_text(source, encoding="utf-8")
    meta = {
        "name": name,
        "path": str(path),
        "exactness": "PASS",
        "ts": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    }
    (dest_dir / f"{name}.json").write_text(json.dumps(meta, indent=2), encoding="utf-8")
    return {"ok": True, "adopted": True, "meta": meta, **chk}
