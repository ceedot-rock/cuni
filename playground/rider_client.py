"""Real Agent-Rider registration client (Studio → Rider cutover).

When CUNI_RIDER_URL is set (e.g. https://agentrider.fly.dev), publish will
POST exactness-gated metadata **and** an explicit citizen_receipt to
POST /api/v0/contracts on that host.

Body shape (PASS only — refuse never pushes):
  {
    "meta": { ...publish meta... },
    "citizen_receipt": {
      "source_hash": "<sha256 hex>",
      "exactness": { "passed": true, ... }
    },
    "studio": "called"
  }

Falls back gracefully if the remote is unreachable (local stub still runs).
Fund path remains Rider settle / XPay — Studio/PCC never funds.
Never log ar_ / JWT values.
"""

from __future__ import annotations

import json
import os
import urllib.error
import urllib.request
from typing import Any


def build_citizen_receipt(
    source_hash: str,
    *,
    checked_at: str | None = None,
    targets: list[str] | None = None,
    stdout_match: bool = True,
) -> dict[str, Any]:
    """PASS citizen_receipt envelope Rider gate expects (source_hash + exactness.passed)."""
    exact: dict[str, Any] = {"passed": True, "stdoutMatch": bool(stdout_match)}
    if checked_at:
        exact["checkedAt"] = checked_at
    if targets is not None:
        exact["targets"] = list(targets)
    return {
        "source_hash": source_hash,
        # Alias kept for docs/clients that prefer camelCase on the hash field.
        "sourceHash": source_hash,
        "exactness": exact,
    }


def citizen_receipt_from_meta(meta: dict[str, Any]) -> dict[str, Any] | None:
    """Derive citizen_receipt from publish meta when exactness.passed is true."""
    if not isinstance(meta, dict):
        return None
    src_hash = meta.get("sourceHash") or meta.get("source_hash")
    if not isinstance(src_hash, str) or not src_hash.strip():
        return None
    exact = meta.get("exactness") if isinstance(meta.get("exactness"), dict) else {}
    if exact.get("passed") is not True:
        return None
    return build_citizen_receipt(
        src_hash.strip(),
        checked_at=exact.get("checkedAt") if isinstance(exact.get("checkedAt"), str) else None,
        targets=exact.get("targets") if isinstance(exact.get("targets"), list) else None,
        stdout_match=exact.get("stdoutMatch", True) is True,
    )


def register_remote(
    meta: dict,
    timeout: float = 12.0,
    citizen_receipt: dict[str, Any] | None = None,
) -> dict[str, Any] | None:
    """POST publish meta + citizen_receipt to real Rider. Returns body or None on skip."""
    base = (os.environ.get("CUNI_RIDER_URL") or "").rstrip("/")
    if not base:
        return None

    receipt = citizen_receipt or citizen_receipt_from_meta(meta)
    if receipt is None:
        # Refuse sacred: never push a non-PASS receipt. Caller should not reach here
        # after Studio exactness gate, but guard anyway.
        return {
            "ok": False,
            "error": "citizen_receipt_missing_or_not_pass — refuse remote push",
            "_url": f"{base}/api/v0/contracts",
        }

    url = f"{base}/api/v0/contracts"
    payload = {
        "meta": meta,
        "citizen_receipt": receipt,
        "studio": "called",
    }
    body = json.dumps(payload).encode("utf-8")
    req = urllib.request.Request(
        url,
        data=body,
        method="POST",
        headers={
            "Content-Type": "application/json",
            "Accept": "application/json",
            "User-Agent": "cuni-studio-publish/0.2",
            "X-Cuni-Studio": "called",
        },
    )
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            raw = resp.read().decode("utf-8")
            data = json.loads(raw) if raw else {}
            data["_http_status"] = resp.status
            data["_url"] = url
            data.setdefault("studio", "called")
            data.setdefault("citizen_receipt_pushed", True)
            return data
    except urllib.error.HTTPError as e:
        try:
            err_body = e.read().decode("utf-8")
            parsed = json.loads(err_body) if err_body else {}
        except Exception:
            parsed = {"error": err_body if "err_body" in dir() else str(e)}
        return {
            "ok": False,
            "error": parsed.get("error") or str(e),
            "_http_status": e.code,
            "_url": url,
            "studio": "called",
            "citizen_receipt_pushed": False,
        }
    except Exception as e:  # noqa: BLE001
        return {
            "ok": False,
            "error": f"rider_unreachable: {e}",
            "_url": url,
            "studio": "called",
            "citizen_receipt_pushed": False,
        }
