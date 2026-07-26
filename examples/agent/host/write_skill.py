#!/usr/bin/env python3
"""Skill writer: propose CuNi law → exactness gate → optional adopt.

  python3 examples/agent/host/write_skill.py propose path/to/draft.cuni
  python3 examples/agent/host/write_skill.py adopt my_skill path/to/draft.cuni
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
AGENT = Path(__file__).resolve().parents[1]
ADOPTED = AGENT / "adopted"


def find_cuni() -> Path:
    for p in (ROOT / "target/release/cuni", ROOT / "target/debug/cuni"):
        if p.is_file():
            return p
    w = shutil.which("cuni")
    if w:
        return Path(w)
    raise SystemExit("cuni not found")


def check_file(cuni: Path, path: Path) -> tuple[bool, str]:
    # stage uses if needed next to file
    r = subprocess.run(
        [str(cuni), "check", str(path), "--timeout", "45"],
        capture_output=True,
        text=True,
        timeout=60,
    )
    out = (r.stdout or "") + (r.stderr or "")
    ok = r.returncode == 0 and "exactness: PASS" in out
    return ok, out


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=("propose", "adopt"))
    ap.add_argument("name_or_path")
    ap.add_argument("path", nargs="?")
    args = ap.parse_args()

    cuni = find_cuni()
    if args.cmd == "propose":
        path = Path(args.name_or_path)
        if not path.is_file():
            raise SystemExit(f"missing {path}")
        # copy modules for use
        with tempfile.TemporaryDirectory() as td:
            work = Path(td)
            src = path.read_text(encoding="utf-8")
            main = work / "main.cuni"
            main.write_text(src, encoding="utf-8")
            import re

            for m in re.findall(r"(?m)^\s*use\s+([A-Za-z0-9_]+)\s*$", src):
                mod = AGENT / f"{m}.cuni"
                if mod.is_file():
                    shutil.copy(mod, work / f"{m}.cuni")
            ok, out = check_file(cuni, main)
        print(out)
        print("CITIZEN" if ok else "REFUSE — not adopted")
        sys.exit(0 if ok else 1)

    # adopt
    name = args.name_or_path
    path = Path(args.path or "")
    if not path.is_file():
        raise SystemExit("adopt requires path to .cuni")
    src = path.read_text(encoding="utf-8")
    with tempfile.TemporaryDirectory() as td:
        work = Path(td)
        main = work / "main.cuni"
        main.write_text(src, encoding="utf-8")
        import re

        for m in re.findall(r"(?m)^\s*use\s+([A-Za-z0-9_]+)\s*$", src):
            mod = AGENT / f"{m}.cuni"
            if mod.is_file():
                shutil.copy(mod, work / f"{m}.cuni")
        ok, out = check_file(cuni, main)
    print(out)
    if not ok:
        print("REFUSE adopt")
        sys.exit(1)
    ADOPTED.mkdir(parents=True, exist_ok=True)
    dest = ADOPTED / f"{name}.cuni"
    dest.write_text(src, encoding="utf-8")
    meta = {"name": name, "path": str(dest), "exactness": "PASS"}
    (ADOPTED / f"{name}.json").write_text(json.dumps(meta, indent=2), encoding="utf-8")
    print(f"ADOPTED {dest}")


if __name__ == "__main__":
    main()
