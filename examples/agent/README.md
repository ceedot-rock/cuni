# Agent mind = CuNi

**Thesis:** AI *runs off* CuNi. Models are **speech**. CuNi is **law**.  
Design: [`docs/AI_IS_CUNI.md`](../../docs/AI_IS_CUNI.md) · Studio: https://cuni-studio.fly.dev/

```
speech (stub | LLM)  →  skill id + params
law     (generated or static .cuni)  →  cuni check MUST PASS
effect  (py | go | js)  →  run emitted code
host tools (optional)  →  e.g. HTTP after tool_plan_get
session  →  examples/agent/sessions/*.jsonl
```

## Quick start

```bash
cargo build --release

# every skill is a citizen
python3 examples/agent/host/run_agent.py --check-all

# speech → params → law → host
python3 examples/agent/host/run_agent.py --message "spend 4 cap 5" --host go
python3 examples/agent/host/run_agent.py --message "echo hello" --host js
python3 examples/agent/host/run_agent.py --message "score 3 9 80" --host py

# multi-step loop
python3 examples/agent/host/run_agent.py --loop \
  --message "spend 2" \
  --message "echo step2" \
  --message "tag Agent"

# interactive
python3 examples/agent/host/run_agent.py --repl
```

Optional LLM routing:

```bash
export CUNI_AGENT_LLM_KEY=sk-...
python3 examples/agent/host/run_agent.py --llm --message "cap my spend at 5"
```

Optional real HTTP after `tool_plan_get`:

```bash
export CUNI_AGENT_HTTP_BASE=https://cuni-studio.fly.dev
python3 examples/agent/host/run_agent.py --message "get /api/health" --host py
```

## Skills (`manifest.json`)

| id | Params (from speech) | Description |
|----|----------------------|-------------|
| `mind` | — | Full composed identity |
| `budget` | `spend N`, `cap M` | allow_spend / clamp_spend |
| `text` | `tag Name`, `join a b` | tag_line / join_two |
| `score` | `score a b n` | prefer / score_ok |
| `tool_echo` | `echo msg` | portable tool echo |
| `tool_plan_get` | `get /path` | plan GET; host may fetch |

## Layout

| Path | Role |
|------|------|
| `manifest.json` | Catalog |
| `*.cuni` modules | Pure law libraries |
| `entry_*.cuni` / `mind.cuni` | Static entrypoints (CI) |
| `host/lawgen.py` | Generate entries with host args |
| `host/run_agent.py` | Speech → gate → effect → session |
| `sessions/` | JSONL logs (gitignored) |

## Exactness is citizenship

No skill runs unless `cuni check` PASSes on the entry (static or generated).  
CI: `.github/workflows/exactness.yml` → agent pack job.

## REPL

```
cuni> skills
cuni> host go
cuni> spend 4 cap 5
cuni> echo hi
cuni> quit
```
