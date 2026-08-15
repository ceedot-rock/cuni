"""φ-rest — after fire, rest.

Called site bytes: φ-split + AES-256-GCM (cryptography) or φ-split +
BLAKE2s stream (stdlib fallback), open once, serve from RAM.
Not Chamber. Chamber is the product. This is the rest.
"""

from __future__ import annotations

import hashlib
import os
from pathlib import Path

PHI = (1 + 5**0.5) / 2

try:
    from cryptography.hazmat.primitives.ciphers.aead import AESGCM  # type: ignore

    _HAS_AES = True
except Exception:  # noqa: BLE001
    AESGCM = None  # type: ignore
    _HAS_AES = False


def _mix_index(i: int, n: int) -> int:
    return int(i * PHI) % n


def phi_split(key: bytes) -> tuple[bytes, bytes]:
    a = os.urandom(len(key))
    b = bytearray(len(key))
    for i in range(len(key)):
        b[i] = key[i] ^ a[_mix_index(i, len(key))]
    return a, bytes(b)


def phi_join(a: bytes, b: bytes) -> bytes:
    key = bytearray(len(a))
    for i in range(len(a)):
        key[i] = b[i] ^ a[_mix_index(i, len(a))]
    return bytes(key)


def _stream(key: bytes, n: int) -> bytes:
    out = bytearray()
    counter = 0
    while len(out) < n:
        out.extend(hashlib.blake2s(key + counter.to_bytes(8, "big"), digest_size=32).digest())
        counter += 1
    return bytes(out[:n])


def cloak(plain: bytes) -> dict:
    key = os.urandom(32)
    a, b = phi_split(key)
    if _HAS_AES:
        iv = os.urandom(12)
        ct = AESGCM(key).encrypt(iv, plain, None)
        return {"iv": iv, "ct": ct, "shares": (a, b), "mode": "aes"}
    iv = os.urandom(16)
    stream = _stream(key + iv, len(plain))
    ct = bytes(x ^ y for x, y in zip(plain, stream))
    tag = hashlib.blake2s(key + iv + ct, digest_size=16).digest()
    return {"iv": iv, "ct": ct, "tag": tag, "shares": (a, b), "mode": "blake"}


def open_sealed(sealed: dict) -> bytes:
    key = phi_join(*sealed["shares"])
    if sealed.get("mode") == "aes" and _HAS_AES:
        return AESGCM(key).decrypt(sealed["iv"], sealed["ct"], None)
    expect = hashlib.blake2s(key + sealed["iv"] + sealed["ct"], digest_size=16).digest()
    if expect != sealed.get("tag"):
        raise ValueError("phi-rest tag mismatch")
    stream = _stream(key + sealed["iv"], len(sealed["ct"]))
    return bytes(x ^ y for x, y in zip(sealed["ct"], stream))


class PhiRest:
    def __init__(self, root: Path, called: list[str] | None = None) -> None:
        self.root = Path(root).resolve()
        self.called = list(called or [])
        self.hot: dict[Path, bytes] = {}

    def boot(self) -> dict:
        n = 0
        nbytes = 0
        for rel in self.called:
            file = Path(rel) if os.path.isabs(rel) else self.root / rel
            if not file.is_file():
                continue
            opened = open_sealed(cloak(file.read_bytes()))
            self.hot[file.resolve()] = opened
            n += 1
            nbytes += len(opened)
        return {"n": n, "bytes": nbytes}

    def from_rest(self, file_path: str | Path) -> bytes | None:
        return self.hot.get(Path(file_path).resolve())
