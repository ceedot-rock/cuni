#!/usr/bin/env bash
# Wire entrypoint for CuNi growth agents.
# Usage:
#   ./scripts/run-growth-pipeline.sh
#   ./scripts/run-growth-pipeline.sh 2026-07-26
# Full multi-agent run is via Grok: workflow cuni-growth-pipeline
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DATE="${1:-$(date -u +%Y-%m-%d)}"
GROWTH="$ROOT/docs/growth"
mkdir -p "$GROWTH"

cat <<EOF
CuNi growth pipeline
====================
Date stamp: $DATE
Repo:       $ROOT

Agents (Grok):
  1. cuni-conversation-scout  → docs/growth/conversation-opportunities-$DATE.md
  2. cuni-growth-exec         → docs/growth/action-queue-$DATE.md
                                docs/growth/posts-ready-$DATE.md

In Grok Build chat:
  Run workflow cuni-growth-pipeline with args {\"date\": \"$DATE\"}

  Or:
  Spawn cuni-conversation-scout then cuni-growth-exec

Artifacts:
EOF
ls -la "$GROWTH" 2>/dev/null || true

if [[ "${2:-}" == "--note" ]] || [[ "${1:-}" == "--note" ]]; then
  NOTE="$GROWTH/pipeline-runs.log"
  echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) pipeline-note date=$DATE" >> "$NOTE"
  echo "Appended note to $NOTE"
fi

echo
echo "Tip: after pipeline, open action-queue and submit Show HN + P0 comments."
