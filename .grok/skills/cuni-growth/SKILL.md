---
name: cuni-growth
description: >
  Run the CuNi user-growth machine: conversation scout + growth exec pipeline.
  Use when the user says "grow CuNi", "run growth", "wire growth", "find threads",
  "marketing exec", "conversation scout", or /cuni-growth.
---

# CuNi growth skill

## Agents

| Name | File | Role |
|------|------|------|
| `cuni-conversation-scout` | `.grok/agents/cuni-conversation-scout.md` | Mine HN/Reddit/X/GitHub for comment opportunities |
| `cuni-growth-exec` | `.grok/agents/cuni-growth-exec.md` | Execute acquisition; write posts/emails/logs |

Also mirrored under `~/.grok/agents/` for global spawn.

## Pipeline (preferred)

Run the workflow:

```
workflow name: cuni-growth-pipeline
args: { "date": "YYYY-MM-DD" }
```

Or from chat: “Run cuni-growth-pipeline with today’s date.”

Phases:
1. **Scout** → `docs/growth/conversation-opportunities-<date>.md`
2. **Growth** → `docs/growth/action-queue-<date>.md` + `posts-ready-<date>.md` + OUTREACH log
3. **Human** submits HN/Reddit/comments (agents draft only by default)

## Manual spawn

```
spawn_subagent subagent_type=cuni-conversation-scout
spawn_subagent subagent_type=cuni-growth-exec
```

Order: **scout first**, then growth (or use the workflow).

## Scripts

```bash
./scripts/run-growth-pipeline.sh           # prints how to invoke
./scripts/run-growth-pipeline.sh --note    # appends a run note
```

## Rules

- Scout never posts comments unless user says “post”.
- Growth never spams; tip lines + warm contacts only.
- Always update `docs/OUTREACH.md`.
