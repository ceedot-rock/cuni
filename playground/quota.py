"""Per-IP daily quotas for CuNi Studio — keep Fly spend bounded.

Env:
  CUNI_PLAYGROUND_FREE_DAILY   default 40  (run/check/emit)
  CUNI_PLAYGROUND_AGENT_DAILY  default 20  (agent/run/chat/propose)
  CUNI_PLAYGROUND_DATA         quota store directory

Local CLI is unlimited; only the hosted Studio is gated.
"""

from __future__ import annotations

import json
import os
import threading
import time
from pathlib import Path

DATA = Path(os.environ.get("CUNI_PLAYGROUND_DATA", str(Path(__file__).resolve().parent / "data")))
FREE_DAILY = int(os.environ.get("CUNI_PLAYGROUND_FREE_DAILY", "40"))
AGENT_DAILY = int(os.environ.get("CUNI_PLAYGROUND_AGENT_DAILY", "20"))
# 0 = unlimited (e.g. local dev)
QUOTA_ENABLED = os.environ.get("CUNI_PLAYGROUND_QUOTA", "1") not in ("0", "false", "no")

_lock = threading.Lock()
_QUOTA_FILE = "quota.json"

INSTALL_HINT = (
    "Free daily limit reached on hosted Studio. "
    "Unlimited on your machine: "
    "cargo install --git https://github.com/ceedot-rock/cuni --tag v0.1.6"
)


def _today() -> str:
    return time.strftime("%Y-%m-%d", time.gmtime())


def _path() -> Path:
    DATA.mkdir(parents=True, exist_ok=True)
    return DATA / _QUOTA_FILE


def _load() -> dict:
    p = _path()
    with _lock:
        if not p.is_file():
            return {"day": _today(), "ips": {}}
        try:
            data = json.loads(p.read_text(encoding="utf-8"))
        except (json.JSONDecodeError, OSError):
            return {"day": _today(), "ips": {}}
        if data.get("day") != _today():
            return {"day": _today(), "ips": {}}
        return data


def _save(data: dict) -> None:
    p = _path()
    with _lock:
        tmp = p.with_suffix(".tmp")
        tmp.write_text(json.dumps(data, indent=2), encoding="utf-8")
        tmp.replace(p)


def _bucket(ip: str) -> dict:
    data = _load()
    ips = data.setdefault("ips", {})
    b = ips.get(ip) or {"play": 0, "agent": 0}
    return data, ips, b


def status(ip: str) -> dict:
    """Remaining quota for an IP (does not increment)."""
    if not QUOTA_ENABLED:
        return {
            "enabled": False,
            "ip": ip,
            "play_limit": FREE_DAILY,
            "agent_limit": AGENT_DAILY,
            "play_used": 0,
            "agent_used": 0,
            "play_remaining": FREE_DAILY,
            "agent_remaining": AGENT_DAILY,
            "day": _today(),
        }
    data, ips, b = _bucket(ip)
    play_used = int(b.get("play", 0))
    agent_used = int(b.get("agent", 0))
    return {
        "enabled": True,
        "ip": ip,
        "play_limit": FREE_DAILY,
        "agent_limit": AGENT_DAILY,
        "play_used": play_used,
        "agent_used": agent_used,
        "play_remaining": max(0, FREE_DAILY - play_used),
        "agent_remaining": max(0, AGENT_DAILY - agent_used),
        "day": data.get("day", _today()),
    }


def check_and_consume(ip: str, kind: str) -> tuple[bool, dict]:
    """kind: 'play' | 'agent'. Returns (allowed, status_dict)."""
    if not QUOTA_ENABLED:
        return True, status(ip)

    limit = FREE_DAILY if kind == "play" else AGENT_DAILY
    key = "play" if kind == "play" else "agent"

    with _lock:
        p = _path()
        if p.is_file():
            try:
                data = json.loads(p.read_text(encoding="utf-8"))
            except (json.JSONDecodeError, OSError):
                data = {"day": _today(), "ips": {}}
        else:
            data = {"day": _today(), "ips": {}}
        if data.get("day") != _today():
            data = {"day": _today(), "ips": {}}

        ips = data.setdefault("ips", {})
        b = ips.get(ip) or {"play": 0, "agent": 0}
        used = int(b.get(key, 0))
        if used >= limit:
            st = {
                "enabled": True,
                "ip": ip,
                "play_limit": FREE_DAILY,
                "agent_limit": AGENT_DAILY,
                "play_used": int(b.get("play", 0)),
                "agent_used": int(b.get("agent", 0)),
                "play_remaining": max(0, FREE_DAILY - int(b.get("play", 0))),
                "agent_remaining": max(0, AGENT_DAILY - int(b.get("agent", 0))),
                "day": data["day"],
                "error": INSTALL_HINT,
                "quota_kind": kind,
            }
            return False, st

        b[key] = used + 1
        ips[ip] = b
        # prune huge maps (keep last ~2k ips)
        if len(ips) > 2500:
            # drop arbitrary excess
            for drop in list(ips.keys())[:500]:
                ips.pop(drop, None)
        data["ips"] = ips
        tmp = p.with_suffix(".tmp")
        DATA.mkdir(parents=True, exist_ok=True)
        tmp.write_text(json.dumps(data, indent=2), encoding="utf-8")
        tmp.replace(p)

    return True, status(ip)


def client_ip(handler) -> str:
    """Best-effort client IP (Fly sets Fly-Client-IP / X-Forwarded-For)."""
    headers = handler.headers
    fly = headers.get("Fly-Client-IP") or headers.get("fly-client-ip")
    if fly:
        return fly.strip()
    xff = headers.get("X-Forwarded-For") or headers.get("x-forwarded-for")
    if xff:
        return xff.split(",")[0].strip()
    try:
        return handler.client_address[0]
    except Exception:  # noqa: BLE001
        return "unknown"
