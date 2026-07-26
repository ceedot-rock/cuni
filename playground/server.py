#!/usr/bin/env python3
"""CuNi Playground v0 — local HTTP server.

Serves static UI and POST /api/run which:
  1. typechecks / emits py+go+js via `cuni`
  2. runs all three targets
  3. reports exactness PASS/FAIL

Usage:
  cargo build --release
  python3 playground/server.py
  # open http://127.0.0.1:8787
"""

from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import tempfile
import threading
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlparse

ROOT = Path(__file__).resolve().parents[1]
PLAY = Path(__file__).resolve().parent
EXAMPLES = ROOT / "examples"
DEFAULT_PORT = int(os.environ.get("CUNI_PLAYGROUND_PORT", "8787"))
TIMEOUT = int(os.environ.get("CUNI_PLAYGROUND_TIMEOUT", "45"))


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
        "cuni binary not found — run `cargo build --release` in the repo root "
        "or set CUNI_BIN=/path/to/cuni"
    )


def list_examples() -> list[dict]:
    out = []
    if not EXAMPLES.is_dir():
        return out
    for p in sorted(EXAMPLES.glob("*.cuni")):
        # skip pure typeck fixtures that need modules-heavy / non-portable cases by default? keep all
        out.append(
            {
                "id": p.stem,
                "name": p.name,
                "source": p.read_text(encoding="utf-8"),
            }
        )
    return out


def stage_source(workdir: Path, source: str) -> Path:
    """Write main.cu ni + any `use X` modules from examples/ if present."""
    main = workdir / "main.cuni"
    main.write_text(source, encoding="utf-8")
    for m in re.findall(r"(?m)^\s*use\s+([A-Za-z_][A-Za-z0-9_]*)\s*$", source):
        cand = EXAMPLES / f"{m}.cuni"
        if cand.is_file():
            shutil.copy(cand, workdir / f"{m}.cuni")
    return main


def run_cmd(args: list[str], cwd: Path | None = None) -> subprocess.CompletedProcess:
    return subprocess.run(
        args,
        cwd=str(cwd) if cwd else None,
        capture_output=True,
        text=True,
        timeout=TIMEOUT,
    )


def compile_and_check(source: str) -> dict:
    cuni = find_cuni()
    with tempfile.TemporaryDirectory(prefix="cuni_play_") as td:
        work = Path(td)
        main = stage_source(work, source)
        py_out = work / "out.py"
        go_out = work / "out.go"
        js_out = work / "out.js"

        # Front-end + emit (captures type errors with line:col)
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
            # Rewrite temp path so UI shows a stable name + keeps line:col
            err = err.replace(str(main), "main.cuni")
            return {
                "ok": False,
                "phase": "compile",
                "error": err,
                "py": None,
                "go": None,
                "js": None,
                "stdout": {},
                "exactness": "FAIL",
                "summary": err.splitlines()[-1] if err else "compile failed",
            }

        py_src = py_out.read_text(encoding="utf-8") if py_out.is_file() else ""
        go_src = go_out.read_text(encoding="utf-8") if go_out.is_file() else ""
        js_src = js_out.read_text(encoding="utf-8") if js_out.is_file() else ""

        # Exactness via cuni check
        check = run_cmd([str(cuni), "check", str(main), "--timeout", str(TIMEOUT)])
        check_out = (check.stdout or "") + (check.stderr or "")
        exact_pass = check.returncode == 0 and "exactness: PASS" in check_out

        # Also capture per-target stdout for the UI (best-effort)
        stdout: dict[str, str] = {}
        run_errs: dict[str, str] = {}
        for label, cmd, art in (
            ("py", ["python3", str(py_out)], py_out),
            ("go", ["go", "run", str(go_out)], go_out),
            ("js", ["node", str(js_out)], js_out),
        ):
            if not art.is_file():
                run_errs[label] = "no artifact"
                continue
            try:
                r = run_cmd(cmd, cwd=work)
                if r.returncode == 0:
                    stdout[label] = r.stdout
                else:
                    run_errs[label] = (r.stderr or r.stdout or f"exit {r.returncode}").strip()
            except subprocess.TimeoutExpired:
                run_errs[label] = f"timeout after {TIMEOUT}s"
            except FileNotFoundError as e:
                run_errs[label] = str(e)

        summary_line = next(
            (ln for ln in check_out.splitlines() if "exactness:" in ln),
            "exactness: FAIL" if not exact_pass else "exactness: PASS (py/go/js)",
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
        }


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
        self.end_headers()
        self.wfile.write(body)

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
                },
            )
        if path == "/api/examples":
            return self._json(200, {"examples": list_examples()})
        if path in ("/", ""):
            self.path = "/index.html"
        return super().do_GET()

    def do_POST(self) -> None:  # noqa: N802
        path = urlparse(self.path).path
        length = int(self.headers.get("Content-Length", "0") or 0)
        raw = self.rfile.read(length) if length else b"{}"
        try:
            data = json.loads(raw.decode("utf-8") or "{}")
        except json.JSONDecodeError:
            return self._json(400, {"ok": False, "error": "invalid JSON body"})

        if path == "/api/run":
            source = data.get("source")
            if not isinstance(source, str) or not source.strip():
                return self._json(400, {"ok": False, "error": "missing source"})
            if len(source) > 200_000:
                return self._json(400, {"ok": False, "error": "source too large"})
            try:
                result = compile_and_check(source)
            except FileNotFoundError as e:
                return self._json(503, {"ok": False, "error": str(e)})
            except subprocess.TimeoutExpired:
                return self._json(504, {"ok": False, "error": f"timeout after {TIMEOUT}s"})
            except Exception as e:  # noqa: BLE001
                return self._json(500, {"ok": False, "error": f"internal: {e}"})
            return self._json(200, result)

        return self._json(404, {"ok": False, "error": "not found"})


def main() -> None:
    try:
        cuni = find_cuni()
        print(f"cuni binary: {cuni}")
    except FileNotFoundError as e:
        print(f"WARNING: {e}")

    host = os.environ.get("CUNI_PLAYGROUND_HOST", "127.0.0.1")
    port = DEFAULT_PORT
    httpd = ThreadingHTTPServer((host, port), Handler)
    url = f"http://{host}:{port}/"
    print(f"CuNi Playground v0 → {url}")
    print("  POST /api/run   {\"source\": \"...\"}")
    print("  GET  /api/examples")
    print("  GET  /api/health")
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        print("\nshutting down")
        httpd.shutdown()


if __name__ == "__main__":
    main()
