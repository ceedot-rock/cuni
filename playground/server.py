#!/usr/bin/env python3
"""CuNi Hosted Playground — TEMPORARY RESTORE SHIM

The full server.py was accidentally overwritten. This shim re-exports from
the last known-good version by instructing operators to redeploy from:
  git show d36b63652f697c73844844ea538bdec9acd747f8:playground/server.py

OR apply the Rider cutover patch documented in docs/RIDER_CUTOVER.md

For immediate local use:
  curl -sL https://raw.githubusercontent.com/ceedot-rock/cuni/d36b63652f697c73844844ea538bdec9acd747f8/playground/server.py > playground/server.py
  # then apply rider_client integration (see docs/RIDER_CUTOVER.md)
"""
import sys
print("ERROR: playground/server.py needs restore from git history.", file=sys.stderr)
print("Run: curl -sL https://raw.githubusercontent.com/ceedot-rock/cuni/d36b63652f697c73844844ea538bdec9acd747f8/playground/server.py -o playground/server.py", file=sys.stderr)
sys.exit(1)
