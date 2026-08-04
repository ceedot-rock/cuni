"""Real Agent-Rider registration client (Studio → Rider cutover).

When CUNI_RIDER_URL is set (e.g. https://agentrider.vercel.app), publish will
POST the exactness-gated metadata to POST /api/v0/contracts on that host.
Falls back gracefully if the remote is unreachable (local stub still runs).
"""

from __future__ import annotations

import json
import os
import urllib.error
import urllib.request
from typing import Any


def register_remote(meta: dict, timeout: float = 12.0) -> dict[str, Any] | None:
    """POST publish meta to real Rider. Returns response body or None on skip/error."""
    base = (os.environ.get("CUNI_RIDER_URL") or "").rstrip("/")
    if not base:
        return None

    url = f"{base}/api/v0/contracts"
    body = json.dumps({"meta": meta}).encode("utf-8")
    req = urllib.request.Request(
        url,
        data=body,
        method="POST",
        headers={
            "Content-Type": "application/json",
            "Accept": "application/json",
            "User-Agent": "cuni-studio-publish/0.1",
        },
    )
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            raw = resp.read().decode("utf-8")
            data = json.loads(raw) if raw else {}
            data["_http_status"] = resp.status
            data["_url"] = url
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
        }
    except Exception as e:  # noqa: BLE001
        return {"ok": False, "error": f"rider_unreachable: {e}", "_url": url}
