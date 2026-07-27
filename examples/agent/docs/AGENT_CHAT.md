# Agent Chat + Autonomy

## Goals

1. **Multi-step planner** — one user goal → several exact CuNi skills.
2. **Quarantine skill writer** — proposed `.cuni` only after `cuni check` PASS.
3. **Learning skills** — teach CuNi via speech (`learn`).
4. **Session memory** — recent turns feed speech.
5. **Chat + voice skeleton** — Studio chat; Web Speech API for mic.

## Modes

| Mode | Intent |
|------|--------|
| `execute` | Speech → skill → exactness → run |
| `learn` | Prefer `learn` skill |
| `code` | Propose law → quarantine |

## Host CLI

```bash
python3 examples/agent/host/run_agent.py --plan --message "spend 3; echo ok; explain exactness"
python3 examples/agent/host/run_agent.py --mode learn --message "explain link"
```

## Studio API (target)

`POST /api/agent/chat`

```json
{
  "message": "explain exactness",
  "mode": "learn",
  "host": "py",
  "plan": false
}
```

→ `agent_lib.chat_turn(...)`  
Subject to **daily IP quota** (see `docs/STUDIO_COST.md`).

## Exactness

No skill runs without `cuni check` PASS.
