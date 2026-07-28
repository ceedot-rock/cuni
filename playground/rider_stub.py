"""Studio-side Rider registration stub.

Until a real Agent-Rider service exists, Studio can accept publish metadata
and store it under DATA/registered/. Wire into server.py:

  from rider_stub import handle_register, handle_list_registered

  # in do_POST:
  if path == "/api/rider/register":
      return self._json(*handle_register(data, DATA, append_note))

  # in do_GET:
  if path == "/api/rider/registered":
      return self._json(*handle_list_registered(DATA))
"""

from __future__ import annotations

import json
import time
import uuid
from pathlib import Path
from typing import Any, Callable


def handle_register(
    data: dict,
    data_dir: Path,
    append_note: Callable[..., dict] | None = None,
) -> tuple[int, dict]:
    """Accept publish metadata; require exactness.passed. Returns (status, body)."""
    meta = data.get("meta") if isinstance(data.get("meta"), dict) else data
    if not isinstance(meta, dict):
        return 400, {"ok": False, "error": "expected publish metadata object"}

    exact = meta.get("exactness") or {}
    if not exact.get("passed"):
        return 400, {
            "ok": False,
            "error": "exactness.passed must be true — refuse register",
        }

    source_hash = meta.get("sourceHash") or "unknown"
    reg_dir = data_dir / "registered"
    reg_dir.mkdir(parents=True, exist_ok=True)

    # Idempotent on sourceHash if already present
    for existing in reg_dir.glob("*.json"):
        try:
            prev = json.loads(existing.read_text(encoding="utf-8"))
            if (prev.get("meta") or {}).get("sourceHash") == source_hash:
                return 200, {
                    "ok": True,
                    "id": prev.get("id"),
                    "status": "registered_stub",
                    "idempotent": True,
                    "stored": existing.name,
                }
        except (json.JSONDecodeError, OSError):
            continue

    rid = str(uuid.uuid4())[:12]
    record = {
        "id": rid,
        "registeredAt": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "status": "registered_stub",
        "meta": meta,
    }
    out = reg_dir / f"{rid}-{source_hash[:8]}.json"
    out.write_text(json.dumps(record, indent=2), encoding="utf-8")

    if append_note:
        append_note(
            f"[rider/register] stub accepted id={rid} hash={source_hash[:12]}…",
            kind="system",
            meta={"id": rid, "sourceHash": source_hash},
        )

    return 200, {
        "ok": True,
        "id": rid,
        "status": "registered_stub",
        "stored": out.name,
        "next": "GET /api/rider/registered to list",
    }


def handle_list_registered(data_dir: Path) -> tuple[int, dict]:
    """List all stub-registered contracts. Returns (status, body)."""
    reg_dir = data_dir / "registered"
    items: list[dict[str, Any]] = []
    if reg_dir.is_dir():
        for p in sorted(reg_dir.glob("*.json"), reverse=True):
            try:
                rec = json.loads(p.read_text(encoding="utf-8"))
                items.append(
                    {
                        "id": rec.get("id"),
                        "registeredAt": rec.get("registeredAt"),
                        "status": rec.get("status"),
                        "sourceHash": (rec.get("meta") or {}).get("sourceHash"),
                        "file": p.name,
                    }
                )
            except (json.JSONDecodeError, OSError):
                continue
    return 200, {"ok": True, "count": len(items), "contracts": items}
