#!/usr/bin/env python3
"""CuNi Hosted Playground — emit + exactness (cuni check) + Notelog + Critic Book.

Pipeline (same as CLI):
  1. cuni <file> --emit-py/--emit-go/--emit-js
  2. cuni check <file>  → exactness: PASS|FAIL

Books:
  Notelog  — chronological lab notes (auto + manual)
  Critic   — structured critiques (auto from failures + manual)

Usage:
  cargo build --release
  CUNI_PLAYGROUND_HOST=0.0.0.0 python3 playground/server.py
"""

from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import tempfile
import threading
import time
import uuid
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
PLAY = Path(__file__).resolve().parent
EXAMPLES = ROOT / "examples"
AGENT = EXAMPLES / "agent"
DATA = Path(os.environ.get("CUNI_PLAYGROUND_DATA", str(PLAY / "data")))
DEFAULT_PORT = int(os.environ.get("CUNI_PLAYGROUND_PORT", "8787"))
TIMEOUT = int(os.environ.get("CUNI_PLAYGROUND_TIMEOUT", "45"))
MAX_SOURCE = int(os.environ.get("CUNI_PLAYGROUND_MAX_SOURCE", "200000"))
MAX_CONCURRENT = int(os.environ.get("CUNI_PLAYGROUND_MAX_CONCURRENT", "2"))
# Hosted default: bind all interfaces. Local-only: set CUNI_PLAYGROUND_HOST=127.0.0.1
DEFAULT_HOST = os.environ.get("CUNI_PLAYGROUND_HOST", "0.0.0.0")
HTTP_BASE = os.environ.get("CUNI_AGENT_HTTP_BASE", "https://cuni-studio.fly.dev")

_run_sem = threading.Semaphore(MAX_CONCURRENT)
_store_lock = threading.Lock()

# Agent pack (examples/agent) — optional if tree incomplete
sys_path_agent = str(PLAY)
if sys_path_agent not in __import__("sys").path:
    __import__("sys").path.insert(0, sys_path_agent)
try:
    import agent_lib
except ImportError:
    agent_lib = None  # type: ignore

try:
    from rider_stub import handle_list_registered, handle_register
except ImportError:
    handle_register = None  # type: ignore
    handle_list_registered = None  # type: ignore

try:
    from rider_client import register_remote
except ImportError:
    register_remote = None  # type: ignore


def list_remote_contracts(timeout: float = 8.0) -> dict:
    """GET Rider contracts list when CUNI_RIDER_URL is set."""
    base = (os.environ.get("CUNI_RIDER_URL") or "").rstrip("/")
    if not base:
        return {"ok": False, "error": "CUNI_RIDER_URL not set"}
    url = f"{base}/api/v0/contracts"
    req = urllib.request.Request(
        url,
        method="GET",
        headers={"Accept": "application/json", "User-Agent": "cuni-studio-health/0.1"},
    )
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            data = json.loads(resp.read().decode() or "{}")
            items = (
                data.get("contracts")
                or data.get("items")
                or data.get("data")
                or (data if isinstance(data, list) else [])
            )
            if not isinstance(items, list):
                items = []
            last = items[0] if items else None
            last_id = None
            if isinstance(last, dict):
                last_id = last.get("contractId") or last.get("id")
            return {
                "ok": True,
                "count": data.get("count", len(items)),
                "last_id": last_id,
                "url": url,
            }
    except Exception as e:  # noqa: BLE001
        return {"ok": False, "error": str(e), "url": url}


def find_cuni() -> Path:
    env = os.environ.get("CUNI_BIN")
    if env and Path(env).is_file():
        return Path(env)
    for rel in (
        ROOT / "target" / "release" / "cuni",
        ROOT / "target" / "debug" / "cuni",
    ):
        if rel.is_file():
            return rel
    which = shutil.which("cuni")
    if which:
        return Path(which)
    raise FileNotFoundError(
        "cuni binary not found — run `cargo build --release` or set CUNI_BIN"
    )


def list_examples() -> list[dict]:
    out = []
    if not EXAMPLES.is_dir():
        return out
    for p in sorted(EXAMPLES.glob("*.cuni")):
        out.append(
            {
                "id": p.stem,
                "name": p.name,
                "source": p.read_text(encoding="utf-8"),
            }
        )
    return out


def stage_source(workdir: Path, source: str) -> Path:
    main = workdir / "main.cuni"
    main.write_text(source, encoding="utf-8")
    for m in re.findall(r"(?m)^\s*use\s+([A-Za-z_][A-Za-z0-9_]*)\s*$", source):
        for base in (AGENT, EXAMPLES):
            cand = base / f"{m}.cuni"
            if cand.is_file():
                shutil.copy(cand, workdir / f"{m}.cuni")
                break
    return main


def run_cmd(args: list[str], cwd: Path | None = None) -> subprocess.CompletedProcess:
    return subprocess.run(
        args,
        cwd=str(cwd) if cwd else None,
        capture_output=True,
        text=True,
        timeout=TIMEOUT,
    )


def _emit_only(cuni: Path, work: Path, main: Path) -> dict:
    py_out, go_out, js_out = work / "out.py", work / "out.go", work / "out.js"
    emit = run_cmd(
        [
            str(cuni),
            str(main),
            "--emit-py",
            str(py_out),
            "--emit-go",
            str(go_out),
            "--emit-js",
            str(js_out),
        ]
    )
    if emit.returncode != 0:
        err = (emit.stderr or emit.stdout or "compile failed").strip()
        err = err.replace(str(main), "main.cuni")
        return {
            "ok": False,
            "phase": "emit",
            "error": err,
            "py": None,
            "go": None,
            "js": None,
            "summary": err.splitlines()[-1] if err else "emit failed",
            "critiques": _critiques_from_compile(err),
        }
    return {
        "ok": True,
        "phase": "emit",
        "error": None,
        "py": py_out.read_text(encoding="utf-8") if py_out.is_file() else "",
        "go": go_out.read_text(encoding="utf-8") if go_out.is_file() else "",
        "js": js_out.read_text(encoding="utf-8") if js_out.is_file() else "",
        "summary": "emit: ok (py/go/js)",
        "critiques": [],
        "_paths": {"py": py_out, "go": go_out, "js": js_out, "main": main, "work": work},
    }


def _critiques_from_compile(err: str) -> list[dict]:
    out = []
    for line in err.splitlines():
        line = line.strip()
        if not line:
            continue
        # main.cuni:1:9: type error: ...
        m = re.match(
            r"^(?:main\.cuni|[^:]+):(\d+):(\d+):\s*(.+)$",
            line,
        )
        if m:
            out.append(
                {
                    "severity": "error",
                    "category": "typeck" if "type" in m.group(3).lower() else "compile",
                    "line": int(m.group(1)),
                    "col": int(m.group(2)),
                    "body": m.group(3),
                    "source": "auto",
                }
            )
        else:
            out.append(
                {
                    "severity": "error",
                    "category": "compile",
                    "line": None,
                    "col": None,
                    "body": line,
                    "source": "auto",
                }
            )
    return out


def _critiques_from_exactness(check_out: str, stdout: dict[str, str]) -> list[dict]:
    critiques = []
    if "exactness: PASS" in check_out:
        return critiques
    critiques.append(
        {
            "severity": "error",
            "category": "exactness",
            "line": None,
            "col": None,
            "body": "Exactness failed: py/go/js stdout are not identical.",
            "source": "auto",
        }
    )
    # Highlight first differing pair for the critic book
    keys = ["py", "go", "js"]
    present = {k: stdout.get(k) for k in keys if k in stdout}
    if len(present) >= 2:
        items = list(present.items())
        a_k, a_v = items[0]
        for b_k, b_v in items[1:]:
            if a_v != b_v:
                critiques.append(
                    {
                        "severity": "warn",
                        "category": "exactness",
                        "line": None,
                        "col": None,
                        "body": f"Divergence {a_k} vs {b_k}: "
                        f"{a_k!r} len={len(a_v or '')}, {b_k!r} len={len(b_v or '')}. "
                        f"Inspect Stdout tab; prefer portable constructs (no ext, same say paths).",
                        "source": "auto",
                    }
                )
                break
    for ln in check_out.splitlines():
        if ln.strip() and "exactness:" not in ln.lower():
            if "error" in ln.lower() or "fail" in ln.lower() or "timeout" in ln.lower():
                critiques.append(
                    {
                        "severity": "warn",
                        "category": "runtime",
                        "line": None,
                        "col": None,
                        "body": ln.strip(),
                        "source": "auto",
                    }
                )
    return critiques


def compile_and_check(source: str, mode: str = "run") -> dict:
    """mode: emit | check | run (emit + check + per-target stdout)."""
    cuni = find_cuni()
    with tempfile.TemporaryDirectory(prefix="cuni_play_") as td:
        work = Path(td)
        main = stage_source(work, source)

        emit_res = _emit_only(cuni, work, main)
        if not emit_res["ok"]:
            return {
                **{k: v for k, v in emit_res.items() if k != "_paths"},
                "stdout": {},
                "run_errors": {},
                "exactness": "FAIL",
                "check_log": "",
            }

        py_src, go_src, js_src = emit_res["py"], emit_res["go"], emit_res["js"]
        paths = emit_res["_paths"]

        if mode == "emit":
            return {
                "ok": True,
                "phase": "emit",
                "error": None,
                "py": py_src,
                "go": go_src,
                "js": js_src,
                "stdout": {},
                "run_errors": {},
                "exactness": "n/a",
                "summary": "emit: ok (py/go/js)",
                "check_log": "",
                "critiques": [],
            }

        # Exactness via official cuni check path
        check = run_cmd([str(cuni), "check", str(main), "--timeout", str(TIMEOUT)])
        check_out = (check.stdout or "") + (check.stderr or "")
        exact_pass = check.returncode == 0 and "exactness: PASS" in check_out

        stdout: dict[str, str] = {}
        run_errs: dict[str, str] = {}
        if mode == "run":
            for label, cmd, art in (
                ("py", ["python3", str(paths["py"])], paths["py"]),
                ("go", ["go", "run", str(paths["go"])], paths["go"]),
                ("js", ["node", str(paths["js"])], paths["js"]),
            ):
                if not art.is_file():
                    run_errs[label] = "no artifact"
                    continue
                try:
                    r = run_cmd(cmd, cwd=work)
                    if r.returncode == 0:
                        stdout[label] = r.stdout
                    else:
                        run_errs[label] = (
                            r.stderr or r.stdout or f"exit {r.returncode}"
                        ).strip()
                except subprocess.TimeoutExpired:
                    run_errs[label] = f"timeout after {TIMEOUT}s"
                except FileNotFoundError as e:
                    run_errs[label] = str(e)

        summary_line = next(
            (ln for ln in check_out.splitlines() if "exactness:" in ln),
            "exactness: FAIL" if not exact_pass else "exactness: PASS (py/go/js)",
        )
        critiques = (
            []
            if exact_pass
            else _critiques_from_exactness(check_out, stdout)
        )

        return {
            "ok": exact_pass,
            "phase": "check",
            "error": None if exact_pass else check_out.strip() or "exactness failed",
            "py": py_src,
            "go": go_src,
            "js": js_src,
            "stdout": stdout,
            "run_errors": run_errs,
            "exactness": "PASS" if exact_pass else "FAIL",
            "summary": summary_line.strip(),
            "check_log": check_out,
            "critiques": critiques,
        }


# ── Notelog + Critic Book persistence ──────────────────────────────────────

def _ensure_data() -> None:
    DATA.mkdir(parents=True, exist_ok=True)
    for name, default in (
        ("notelog.json", {"entries": []}),
        ("criticbook.json", {"entries": []}),
    ):
        p = DATA / name
        if not p.is_file():
            p.write_text(json.dumps(default, indent=2), encoding="utf-8")


def _load_book(name: str) -> dict:
    _ensure_data()
    p = DATA / name
    with _store_lock:
        try:
            return json.loads(p.read_text(encoding="utf-8"))
        except (json.JSONDecodeError, OSError):
            return {"entries": []}


def _save_book(name: str, data: dict) -> None:
    _ensure_data()
    p = DATA / name
    with _store_lock:
        tmp = p.with_suffix(".tmp")
        tmp.write_text(json.dumps(data, indent=2), encoding="utf-8")
        tmp.replace(p)


def append_note(body: str, kind: str = "manual", meta: dict | None = None) -> dict:
    book = _load_book("notelog.json")
    entry = {
        "id": str(uuid.uuid4()),
        "ts": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "kind": kind,  # manual | run | system
        "body": body.strip(),
        "meta": meta or {},
    }
    book.setdefault("entries", []).append(entry)
    # cap
    book["entries"] = book["entries"][-500:]
    _save_book("notelog.json", book)
    return entry


def append_critique(
    body: str,
    *,
    severity: str = "note",
    category: str = "design",
    line: int | None = None,
    col: int | None = None,
    source: str = "manual",
    meta: dict | None = None,
) -> dict:
    book = _load_book("criticbook.json")
    entry = {
        "id": str(uuid.uuid4()),
        "ts": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "severity": severity,  # error | warn | note
        "category": category,  # typeck | exactness | runtime | design | style
        "line": line,
        "col": col,
        "body": body.strip(),
        "source": source,  # auto | manual
        "meta": meta or {},
    }
    book.setdefault("entries", []).append(entry)
    book["entries"] = book["entries"][-500:]
    _save_book("criticbook.json", book)
    return entry


class Handler(SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(PLAY), **kwargs)

    def log_message(self, fmt: str, *args) -> None:
        print(f"[play] {self.address_string()} {fmt % args}")

    def _json(self, code: int, payload: dict) -> None:
        body = json.dumps(payload).encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.send_header("Access-Control-Allow-Origin", "*")
        self.end_headers()
        self.wfile.write(body)

    def do_OPTIONS(self) -> None:  # noqa: N802
        self.send_response(204)
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "Content-Type")
        self.end_headers()

    def do_GET(self) -> None:  # noqa: N802
        path = urlparse(self.path).path
        if path == "/api/health":
            try:
                bin_path = str(find_cuni())
                ok = True
                err = None
            except Exception as e:  # noqa: BLE001
                bin_path = None
                ok = False
                err = str(e)
            return self._json(
                200,
                {
                    "ok": ok,
                    "cuni": bin_path,
                    "error": err,
                    "python": shutil.which("python3"),
                    "go": shutil.which("go"),
                    "node": shutil.which("node"),
                    "host": DEFAULT_HOST,
                    "timeout": TIMEOUT,
                    "max_concurrent": MAX_CONCURRENT,
                    "books": {
                        "notelog": len(_load_book("notelog.json").get("entries", [])),
                        "critic": len(_load_book("criticbook.json").get("entries", [])),
                    },
                    "agent": bool(agent_lib and agent_lib.agent_available()),
                    "studio": "https://cuni-studio.fly.dev/",
                    "rider": {
                        "register": bool(handle_register),
                        "list": bool(handle_list_registered),
                        "remote": bool(
                            register_remote
                            and (os.environ.get("CUNI_RIDER_URL") or "").strip()
                        ),
                        "remote_url": (os.environ.get("CUNI_RIDER_URL") or "").rstrip("/")
                        or None,
                        "client": bool(register_remote),
                        "contracts": list_remote_contracts(),
                    },
                },
            )
        if path == "/api/examples":
            return self._json(200, {"examples": list_examples()})
        if path == "/api/agent/skills":
            if not agent_lib or not agent_lib.agent_available():
                return self._json(503, {"ok": False, "error": "agent pack not available"})
            return self._json(
                200,
                {
                    "ok": True,
                    "thesis": "Speech routes; law is CuNi; exactness is citizenship.",
                    "skills": agent_lib.list_skills(),
                    "manifest": agent_lib.load_manifest(),
                },
            )
        if path == "/api/notelog":
            return self._json(200, _load_book("notelog.json"))
        if path == "/api/criticbook":
            return self._json(200, _load_book("criticbook.json"))
        # Studio-side Rider registration stub (list)
        if path == "/api/rider/contracts":
            return self._json(200, list_remote_contracts())
        if path == "/api/rider/registered":
            if not handle_list_registered:
                return self._json(503, {"ok": False, "error": "rider_stub not available"})
            return self._json(*handle_list_registered(DATA))
        if path in ("/", ""):
            self.path = "/index.html"
            return super().do_GET()
        # do not serve data/ directory
        if path.startswith("/data"):
            return self._json(404, {"ok": False, "error": "not found"})
        return super().do_GET()

    def do_POST(self) -> None:  # noqa: N802
        path = urlparse(self.path).path
        length = int(self.headers.get("Content-Length", "0") or 0)
        raw = self.rfile.read(length) if length else b"{}"
        try:
            data = json.loads(raw.decode("utf-8") or "{}")
        except json.JSONDecodeError:
            return self._json(400, {"ok": False, "error": "invalid JSON body"})

        if path in ("/api/run", "/api/emit", "/api/check"):
            source = data.get("source")
            if not isinstance(source, str) or not source.strip():
                return self._json(400, {"ok": False, "error": "missing source"})
            if len(source) > MAX_SOURCE:
                return self._json(400, {"ok": False, "error": "source too large"})
            mode = {"/api/emit": "emit", "/api/check": "check", "/api/run": "run"}[path]
            if not _run_sem.acquire(blocking=False):
                return self._json(
                    503,
                    {
                        "ok": False,
                        "error": f"server busy (max {MAX_CONCURRENT} concurrent runs)",
                    },
                )
            try:
                result = compile_and_check(source, mode=mode)
            except FileNotFoundError as e:
                _run_sem.release()
                return self._json(503, {"ok": False, "error": str(e)})
            except subprocess.TimeoutExpired:
                _run_sem.release()
                return self._json(
                    504, {"ok": False, "error": f"timeout after {TIMEOUT}s"}
                )
            except Exception as e:  # noqa: BLE001
                _run_sem.release()
                return self._json(500, {"ok": False, "error": f"internal: {e}"})
            _run_sem.release()

            # Auto-append notelog + critic book
            note_body = f"[{mode}] {result.get('summary') or result.get('exactness')}"
            if result.get("error") and mode != "emit":
                note_body += f"\n{(result.get('error') or '')[:400]}"
            append_note(
                note_body,
                kind="run",
                meta={
                    "mode": mode,
                    "exactness": result.get("exactness"),
                    "ok": result.get("ok"),
                },
            )
            for c in result.get("critiques") or []:
                append_critique(
                    c.get("body", ""),
                    severity=c.get("severity", "error"),
                    category=c.get("category", "compile"),
                    line=c.get("line"),
                    col=c.get("col"),
                    source="auto",
                    meta={"mode": mode},
                )
            result["notelog_count"] = len(
                _load_book("notelog.json").get("entries", [])
            )
            result["critic_count"] = len(
                _load_book("criticbook.json").get("entries", [])
            )
            return self._json(200, result)

        if path == "/api/agent/run":
            if not agent_lib or not agent_lib.agent_available():
                return self._json(503, {"ok": False, "error": "agent pack not available"})
            if not _run_sem.acquire(blocking=False):
                return self._json(503, {"ok": False, "error": "server busy"})
            try:
                skill = data.get("skill")
                message = data.get("message") or ""
                host = data.get("host") or "py"
                if host not in ("py", "go", "js"):
                    host = "py"
                params = data.get("params") if isinstance(data.get("params"), dict) else {}
                if message and not skill:
                    hint, parsed = agent_lib.parse_message_params(message)
                    skill = hint or "mind"
                    params = {**parsed, **params}
                if not skill:
                    skill = "mind"
                cuni = find_cuni()
                result = agent_lib.run_skill(
                    cuni,
                    skill,
                    params=params,
                    host=host,
                    timeout=TIMEOUT,
                    http_base=HTTP_BASE,
                )
                append_note(
                    f"[agent:{skill}] {result.get('summary') or result.get('exactness')}",
                    kind="run",
                    meta={"skill": skill, "ok": result.get("ok")},
                )
                if not result.get("ok"):
                    append_critique(
                        result.get("error") or result.get("summary") or "agent fail",
                        severity="error",
                        category="exactness",
                        source="auto",
                        meta={"skill": skill},
                    )
                return self._json(200, result)
            except FileNotFoundError as e:
                return self._json(503, {"ok": False, "error": str(e)})
            except subprocess.TimeoutExpired:
                return self._json(504, {"ok": False, "error": f"timeout after {TIMEOUT}s"})
            except Exception as e:  # noqa: BLE001
                return self._json(500, {"ok": False, "error": f"internal: {e}"})
            finally:
                _run_sem.release()

        if path == "/api/agent/propose":
            # Skill writer: check proposed law; do not adopt
            if not agent_lib:
                return self._json(503, {"ok": False, "error": "agent_lib missing"})
            source = data.get("source")
            if not isinstance(source, str) or not source.strip():
                return self._json(400, {"ok": False, "error": "missing source"})
            if len(source) > MAX_SOURCE:
                return self._json(400, {"ok": False, "error": "source too large"})
            if not _run_sem.acquire(blocking=False):
                return self._json(503, {"ok": False, "error": "server busy"})
            try:
                cuni = find_cuni()
                result = agent_lib.check_source(cuni, source, TIMEOUT)
                append_note(
                    f"[propose] {result.get('summary')}",
                    kind="run",
                    meta={"ok": result.get("ok")},
                )
                if not result.get("ok"):
                    append_critique(
                        result.get("summary") or "propose exactness FAIL",
                        severity="error",
                        category="exactness",
                        source="auto",
                    )
                # quarantine raw proposals
                qdir = DATA / "quarantine"
                qdir.mkdir(parents=True, exist_ok=True)
                qid = str(uuid.uuid4())[:8]
                (qdir / f"{qid}.cuni").write_text(source, encoding="utf-8")
                (qdir / f"{qid}.json").write_text(
                    json.dumps(
                        {
                            "id": qid,
                            "ok": result.get("ok"),
                            "summary": result.get("summary"),
                            "ts": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                        },
                        indent=2,
                    ),
                    encoding="utf-8",
                )
                result["quarantine_id"] = qid
                result["adopted"] = False
                return self._json(200, result)
            except Exception as e:  # noqa: BLE001
                return self._json(500, {"ok": False, "error": str(e)})
            finally:
                _run_sem.release()

        if path == "/api/agent/adopt":
            if not agent_lib:
                return self._json(503, {"ok": False, "error": "agent_lib missing"})
            source = data.get("source")
            name = data.get("name") or "skill"
            if not isinstance(source, str) or not source.strip():
                return self._json(400, {"ok": False, "error": "missing source"})
            if not _run_sem.acquire(blocking=False):
                return self._json(503, {"ok": False, "error": "server busy"})
            try:
                cuni = find_cuni()
                result = agent_lib.adopt_skill(DATA, str(name), source, cuni, TIMEOUT)
                append_note(
                    f"[adopt:{name}] {result.get('summary')} adopted={result.get('adopted')}",
                    kind="system",
                )
                return self._json(200, result)
            except Exception as e:  # noqa: BLE001
                return self._json(500, {"ok": False, "error": str(e)})
            finally:
                _run_sem.release()

        if path == "/api/notelog":
            body = data.get("body")
            if not isinstance(body, str) or not body.strip():
                return self._json(400, {"ok": False, "error": "missing body"})
            entry = append_note(body, kind=data.get("kind") or "manual")
            return self._json(200, {"ok": True, "entry": entry})

        if path == "/api/criticbook":
            body = data.get("body")
            if not isinstance(body, str) or not body.strip():
                return self._json(400, {"ok": False, "error": "missing body"})
            entry = append_critique(
                body,
                severity=data.get("severity") or "note",
                category=data.get("category") or "design",
                line=data.get("line"),
                col=data.get("col"),
                source="manual",
            )
            return self._json(200, {"ok": True, "entry": entry})

        if path == "/api/notelog/clear":
            _save_book("notelog.json", {"entries": []})
            return self._json(200, {"ok": True})

        if path == "/api/criticbook/clear":
            _save_book("criticbook.json", {"entries": []})
            return self._json(200, {"ok": True})

        # Studio → Rider publish prototype: exactness gate, then metadata JSON
        if path == "/api/publish":
            import hashlib
            from datetime import datetime, timezone

            source = data.get("source")
            if not isinstance(source, str) or not source.strip():
                return self._json(400, {"ok": False, "error": "missing source"})
            if len(source) > MAX_SOURCE:
                return self._json(400, {"ok": False, "error": "source too large"})
            if not _run_sem.acquire(blocking=False):
                return self._json(503, {"ok": False, "error": "server busy"})
            try:
                result = compile_and_check(source, mode="check")
                exact = (result.get("exactness") or "").upper()
                passed = result.get("ok") is True or exact.startswith("PASS")
                if not passed:
                    append_note(
                        f"[publish] exactness FAIL — refuse publish",
                        kind="run",
                        meta={"ok": False},
                    )
                    return self._json(
                        400,
                        {
                            "ok": False,
                            "error": "Exactness FAILED – refusing to publish",
                            "exactness": result.get("exactness") or result.get("summary"),
                            "result": result,
                        },
                    )
                ts = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
                source_hash = hashlib.sha256(source.encode("utf-8")).hexdigest()
                meta = {
                    "version": "0.1",
                    "source": source,
                    "sourceHash": source_hash,
                    "exactness": {
                        "passed": True,
                        "checkedAt": ts,
                        "targets": ["py", "go", "js"],
                        "stdoutMatch": True,
                    },
                    "publishedAt": ts,
                    "publisher": "studio",
                }
                pub_dir = DATA / "published"
                pub_dir.mkdir(parents=True, exist_ok=True)
                out_path = pub_dir / f"{source_hash[:16]}.publish.json"
                out_path.write_text(json.dumps(meta, indent=2), encoding="utf-8")
                append_note(
                    f"[publish] exactness PASS hash={source_hash[:12]}… → Rider metadata",
                    kind="system",
                    meta={"sourceHash": source_hash, "ok": True},
                )
                # Auto-register into Studio-side Rider stub when available
                registration = None
                if handle_register:
                    _code, registration = handle_register(
                        {"meta": meta}, DATA, append_note
                    )
                # Real Agent-Rider (when CUNI_RIDER_URL set)
                rider = None
                if register_remote:
                    try:
                        rider = register_remote(meta)
                        if rider and rider.get("ok") is not False:
                            append_note(
                                f"[publish] Rider remote ok "
                                f"id={(rider.get('contractId') or rider.get('id') or '')!s}",
                                kind="system",
                                meta={"rider": True, "ok": True},
                            )
                        elif rider:
                            append_note(
                                f"[publish] Rider remote: {rider.get('error', 'fail')}",
                                kind="system",
                                meta={"rider": True, "ok": False},
                            )
                    except Exception as e:  # noqa: BLE001
                        rider = {"ok": False, "error": str(e)}
                        append_note(
                            f"[publish] Rider remote exception: {e}",
                            kind="system",
                            meta={"rider": True, "ok": False},
                        )
                return self._json(
                    200,
                    {
                        "ok": True,
                        "meta": meta,
                        "stored": str(out_path.name),
                        "registration": registration,
                        "rider": rider,
                        "next": "Remote Rider when CUNI_RIDER_URL set; local stub still available",
                        "docs": "docs/RIDER_CUTOVER.md + docs/PUBLISH_FLOW.md",
                    },
                )
            except FileNotFoundError as e:
                return self._json(503, {"ok": False, "error": str(e)})
            except subprocess.TimeoutExpired:
                return self._json(504, {"ok": False, "error": f"timeout after {TIMEOUT}s"})
            except Exception as e:  # noqa: BLE001
                return self._json(500, {"ok": False, "error": f"internal: {e}"})
            finally:
                _run_sem.release()

        # Studio-side Rider registration stub (register)
        if path == "/api/rider/register":
            if not handle_register:
                return self._json(503, {"ok": False, "error": "rider_stub not available"})
            return self._json(*handle_register(data, DATA, append_note))

        return self._json(404, {"ok": False, "error": "not found"})


def main() -> None:
    _ensure_data()
    try:
        cuni = find_cuni()
        print(f"cuni binary: {cuni}")
    except FileNotFoundError as e:
        print(f"WARNING: {e}")

    host = DEFAULT_HOST
    port = DEFAULT_PORT
    httpd = ThreadingHTTPServer((host, port), Handler)
    # Prefer dual-stack display
    display = "localhost" if host in ("0.0.0.0", "::") else host
    print(f"CuNi Playground (hosted) → http://{display}:{port}/")
    print(f"  bind {host}:{port}  timeout={TIMEOUT}s  concurrent={MAX_CONCURRENT}")
    print("  POST /api/run    emit + cuni check + stdout")
    print("  POST /api/emit   emit only")
    print("  POST /api/check  emit + cuni check")
    print("  POST /api/publish  exactness gate → Rider metadata (+ remote if CUNI_RIDER_URL)")
    print("  POST /api/rider/register  |  GET /api/rider/registered  (Studio Rider stub)")
    print("  GET/POST /api/notelog   |  /api/criticbook")
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        print("\nshutting down")
        httpd.shutdown()


if __name__ == "__main__":
    main()
