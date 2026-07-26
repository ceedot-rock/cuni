# CuNi agent notes

## Freeze (current)

**Only build** hosted Studio: `playground/` (emit + `cuni check` + Notelog + Critic Book).  
See `docs/FREEZE.md`. Do not expand language/registry/pump/side projects unless Studio is blocked.

## Growth agents (wired)

When the user wants **users, marketing, distribution, or conversation mining** for CuNi:

1. Prefer workflow **`cuni-growth-pipeline`** (scout → growth exec).
2. Or spawn in order:
   - `cuni-conversation-scout` — find threads
   - `cuni-growth-exec` — turn into posts/actions/logs

Definitions: `.grok/agents/*.md` (also `~/.grok/agents/`).

## Artifacts

| Path | Producer |
|------|----------|
| `docs/growth/conversation-opportunities-*.md` | scout |
| `docs/growth/posts-ready-*.md` | growth exec |
| `docs/growth/action-queue-*.md` | growth exec (merged P0/P1 + submits) |
| `docs/OUTREACH.md` | cumulative log |

## Product facts

- Repo: https://github.com/ceedot-rock/cuni
- Install: `cargo install --git https://github.com/ceedot-rock/cuni --tag v0.1.6`
- Exactness: `cuni check`
- Flagship: `./examples/link/demo.sh`
