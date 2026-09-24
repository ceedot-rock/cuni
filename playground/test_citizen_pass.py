"""Shape smoke for Studio citizen_receipt + /api/pass response contracts.

CI-friendly: no cuni binary, no HTTP server.
Run: python3 -m unittest playground.test_citizen_pass -v
"""

from __future__ import annotations

import hashlib
import json
import sys
import unittest
from pathlib import Path

PLAY = Path(__file__).resolve().parent
if str(PLAY) not in sys.path:
    sys.path.insert(0, str(PLAY))

from rider_client import (  # noqa: E402
    build_citizen_receipt,
    citizen_receipt_from_meta,
)


def _pass_body(source: str, gate=("py", "go", "js")) -> dict:
    src_hash = hashlib.sha256(source.encode("utf-8")).hexdigest()
    receipt = build_citizen_receipt(src_hash, checked_at="2026-09-24T00:00:00Z", targets=list(gate))
    return {
        "ok": True,
        "verdict": "PASS",
        "citizen_receipt": {
            "source_hash": receipt["source_hash"],
            "exactness": {"passed": True},
        },
        "gate": list(gate),
        "studio": "called",
    }


def _refuse_body(diagnostics: str = "exactness FAIL") -> dict:
    return {
        "ok": False,
        "verdict": "REFUSE",
        "citizen_receipt": None,
        "exactness": {"passed": False},
        "error": "Exactness FAILED – refusing PASS",
        "diagnostics": diagnostics,
        "studio": "called",
    }


class CitizenPassShapeTests(unittest.TestCase):
    def test_build_citizen_receipt_pass_fields(self):
        r = build_citizen_receipt("abc123", targets=["py", "go", "js"])
        self.assertEqual(r["source_hash"], "abc123")
        self.assertEqual(r["sourceHash"], "abc123")
        self.assertIs(r["exactness"]["passed"], True)
        self.assertEqual(r["exactness"]["targets"], ["py", "go", "js"])

    def test_citizen_receipt_from_meta_requires_pass(self):
        meta_fail = {"sourceHash": "dead", "exactness": {"passed": False}}
        self.assertIsNone(citizen_receipt_from_meta(meta_fail))
        meta_ok = {
            "sourceHash": "beef",
            "exactness": {"passed": True, "checkedAt": "t", "targets": ["py"]},
        }
        r = citizen_receipt_from_meta(meta_ok)
        assert r is not None
        self.assertEqual(r["source_hash"], "beef")
        self.assertIs(r["exactness"]["passed"], True)

    def test_pass_response_shape(self):
        body = _pass_body("say(1)\n")
        self.assertTrue(body["ok"])
        self.assertEqual(body["verdict"], "PASS")
        self.assertEqual(body["studio"], "called")
        cr = body["citizen_receipt"]
        self.assertTrue(isinstance(cr["source_hash"], str) and len(cr["source_hash"]) == 64)
        self.assertIs(cr["exactness"]["passed"], True)
        self.assertEqual(body["gate"], ["py", "go", "js"])
        # Round-trip JSON (Rider clients parse this)
        json.loads(json.dumps(body))

    def test_refuse_response_shape(self):
        body = _refuse_body("parse error")
        self.assertFalse(body["ok"])
        self.assertEqual(body["verdict"], "REFUSE")
        self.assertIsNone(body["citizen_receipt"])
        self.assertIs(body["exactness"]["passed"], False)
        self.assertEqual(body["studio"], "called")
        self.assertIn("diagnostics", body)


if __name__ == "__main__":
    unittest.main()
