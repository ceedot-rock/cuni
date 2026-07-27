# Agent Chat + Autonomy (feat branch)

## Goals

1. **Multi-step planner** — one user goal can expand to several exact CuNi skills in sequence.
2. **Quarantine skill writer** — LLM/user-proposed `.cuni` only becomes law after `cuni check` PASS.
3. **Learning skills** — teach CuNi through speech (`learn` skill).
4. **Session memory** — recent turns feed speech context.
5. **Chat + voice skeleton** — Studio chat panel; browser Web Speech API for mic input.

## Modes (Chat)

| Mode | Intent |
|------|--------|
| `execute` | Route speech → existing skill → exactness → run |
| `learn` | Prefer `learn` skill; explain + mini examples |
| `code` | Propose new law into quarantine; adopt only on PASS |

## Host CLI additions

```bash
# multi-step plan (stub planner splits on `;` or uses LLM JSON plan)
python3 examples/agent/host/run_agent.py --plan --message "spend 3; echo ok; score 2 8"

# session memory is automatic under examples/agent/sessions/

# propose law (quarantine)
python3 examples/agent/host/write_skill.py propose draft.cuni
```

## Studio

- Agent mode gains a **Chat** strip: history, mode select, mic button.
- `/api/agent/chat` accepts `{ message, mode, host, history? }` and returns steps + law effects.

## Exactness rule (unchanged)

No skill runs in production path without `cuni check` PASS.
Proposed law stays in quarantine until adopt.
