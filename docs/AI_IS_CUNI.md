# AI is CuNi — design

**Status:** design (not shipped).  
**One line:** The agent is not a chat wrapper around Python. **The agent’s portable mind is CuNi** — exact across runtimes, or it refuses to exist.

**Related:** Studio https://cuni-studio.fly.dev/ · `cuni check` · `link` · `docs/REGISTRY.md` · freeze: Studio first, this after convert path is warm.

---

## 0. Name / slogan

| Short | Meaning |
|-------|---------|
| **AI runs off CuNi** | Decision logic, tools contracts, and portable skills are written in CuNi and checked for exactness before any model or host runs them. |
| **AI is CuNi** | Identity: the durable self of the agent *is* the CuNi program (versioned, exact, multi-runtime). Models are *speech*; CuNi is *law*. |

Humans talk to models. Models hallucinate and drift.  
**CuNi is how you speak to machines so they must agree** — py, go, js same behavior or refuse.

---

## 1. Problem

| Failure mode | What happens today |
|--------------|-------------------|
| Agent logic in one language | “Works in my Python” ≠ works in the Go worker or JS edge |
| Prompt-only agents | No compile-time contract; behavior is probabilistic |
| Multi-agent stacks | Each runtime reimplements tools; silent divergence |
| Trust | Users can’t verify an agent will do the *same* thing everywhere |

You wanted to talk to machines. Machines need a language that **enforces agreement**.

---

## 2. Thesis

```
┌─────────────────────────────────────────────────────────┐
│  LLM (any provider)     = speech / planning / decoding  │
│  CuNi program           = identity + portable law       │
│  cuni check             = trust gate before deploy      │
│  link                   = how agents call each other    │
│  Studio Notelog/Critic  = memory of the design dialogue │
└─────────────────────────────────────────────────────────┘
```

- **Not** “AI writes CuNi once and we throw it away.”  
- **Yes** “AI proposes CuNi; exactness accepts or rejects; **accepted CuNi is the agent**.”

The agent you ship is the `.cuni` (and its emitted py/go/js), not the chat transcript.

---

## 3. Architecture (v0 mind)

```
                    ┌──────────────┐
   user / goal ───► │  Orchestrator │  (thin host: Node or Go or Python)
                    │  “speech”     │  calls LLM for plan / natural language
                    └──────┬───────┘
                           │ proposes or loads skills as .cuni
                           ▼
                    ┌──────────────┐
                    │  CuNi core   │  skills, policies, tool adapters
                    │  (identity)  │  def / link / fail / exact stdlib
                    └──────┬───────┘
              ┌────────────┼────────────┐
              ▼            ▼            ▼
           emit-py      emit-go      emit-js
              │            │            │
         cuni check ── exactness PASS required ──► deploy
              │
              ▼
     runtimes (worker / edge / local)
              │
              ▼
     link ──HTTP+JSON──► other agents / tools (Quikgater, etc.)
```

### Layers

| Layer | Role | CuNi? |
|-------|------|--------|
| **Speech** | LLM chat, planning, “decode user intent” | No — any model |
| **Law** | Policies, tool contracts, pure transforms | **Yes — CuNi only** |
| **Voice** | Emit to py/go/js for the host that runs | Generated from CuNi |
| **Wire** | Agent↔agent / agent↔tool | **`link`** |
| **Memory of design** | Notelog + Critic Book | Studio / files, not the model weights |

---

## 4. What “the AI is CuNi” looks like in code (sketch)

### 4.1 Skill = portable CuNi

```cuni
# skills/normalize_url.cuni
def normalize_url(url: str) -> str do
    # pure, exact — same string on py/go/js
    ret url
end
```

Gate: `cuni check skills/normalize_url.cuni` must PASS before the orchestrator may load it.

### 4.2 Tool contract = `link`

```cuni
# tools/fetch_fact.cuni
link FetchFact(url: str) -> str ? do
    # body is the portable “intent”; hosts implement transport
    # or ext only when truly host-specific (breaks exactness for that part)
    ret `ok`
end
```

Other agents call the same contract from py/go/js clients — **one definition**.

### 4.3 Policy = CuNi that can `fail`

```cuni
def allow_spend(usd: int) -> int ? do
    if usd > 5 do
        fail "over budget"
    end
    ret usd
end
```

Exactness: budget rules don’t silently differ between the JS edge and the Go worker.

### 4.4 Orchestrator (not CuNi) only does

1. Talk to human / LLM.  
2. Load **versioned** CuNi skills that already **passed** `cuni check`.  
3. Route `link` calls.  
4. Never invent portable logic only in the prompt.

---

## 5. Trust model

| Gate | When | Fail means |
|------|------|------------|
| **Typeck** | emit | Agent identity doesn’t type-check |
| **Exactness** | `cuni check` | Agent would *be different people* on different runtimes |
| **Critic Book** | design time | Humans/agents record *why* a skill was refused |
| **Notelog** | sessions | Lab history of what the AI tried in Studio |

**Shipping rule:** no skill in production without exactness PASS (or an explicit `ext` quarantine with no exactness claim).

---

## 6. Product surfaces (how this ships)

| Surface | Role |
|---------|------|
| **CuNi Studio** | Where humans (and later agents) author skills; Run exactness; Notelog/Critic |
| **CLI `cuni check`** | CI gate for agent repos |
| **Registry** (later) | Versioned skills + link contracts with forced exactness CI |
| **Runtime hosts** | Thin: “load this .cuni skill, run emitted code, call LLM for speech only” |
| **$CuNi / brand** | Optional signal; **not** the runtime |

### Minimal demo (v0 — can exist without new language features)

1. Folder `agent/` with a few `.cuni` skills + `cuni check` in CI.  
2. Host script (Python or Node) that:  
   - takes a user message → LLM returns “call skill X with args”  
   - runs **only** skills that passed check  
3. Document: “this agent’s mind is these files.”

No new syntax required. Design uses what CuNi already is.

---

## 7. “AI runs off CuNi” runtime loop

```
loop:
  observe (user, tools, memory)
  speech  = LLM.plan(observe, available_skills_manifest)
  law     = select CuNi skill(s) from speech
  assert skill has exactness badge
  effect  = run emitted skill on current host (py|go|js)
  if fail → Critic Book / user
  if ok   → Notelog + continue
```

The LLM never *is* the policy. The LLM *chooses among* exact CuNi laws.

---

## 8. Non-goals (keep honest)

- CuNi does **not** replace the LLM.  
- CuNi does **not** need to be Turing-complete agent OS on day one.  
- `ext` blocks are **host glue**, not “the real agent.”  
- No claim that v0.1 is production agent platform.  
- Do not block Studio freeze for full agent OS implementation.

---

## 9. Roadmap (design phases)

| Phase | Deliverable | Depends |
|-------|-------------|---------|
| **P0** | This doc + messaging (“AI is CuNi”) | **done** (`docs/AI_IS_CUNI.md`) |
| **P1** | `examples/agent/` — skills + mind + exactness | **done** |
| **P2** | Thin host `examples/agent/host/run_agent.py` + speech stub/LLM | **done** |
| **P2.1** | Per-skill entrypoints + `manifest.json` + `--check-all` / `--skill` | **done** |
| **P2.2** | Host arg injection (`lawgen`), multi-step `--loop`, `--repl`, session JSONL, `tool_plan_get` + host HTTP | **done** |
| **P2.3** | CI: exactness.yml runs agent pack + `--check-all` | **done** |
| **P3** | Studio mode “Agent skills” (open agent folder, check all) | Studio |
| **P4** | Registry packages for skills + real `link` HTTP tools | registry design |
| **P5** | Multi-host proof: same skill bag on Go worker + JS edge | P2 |

---

## 10. Messaging (for posts)

> The most connected humans are busy talking to machines.  
> I talk to machines in **CuNi** — exact across Python, Go, and JavaScript, or refuse.  
> **The AI isn’t the chat. The AI is the CuNi that passed exactness.**

Try: https://cuni-studio.fly.dev/

---

## 11. Open design questions

1. Skill manifest format (JSON list of CuNi paths + check hashes)?  
2. Should LLM be allowed to *write* new `.cuni` only into a quarantine until check PASSes? (**Recommend: yes.**)  
3. Memory: pure CuNi vs host DB — keep durable state out of exactness surface at first.  
4. Name: **CuNi Agent**, **Law runtime**, **Exact agent** — pick when P1 ships.

---

## 12. Decision

**Accept thesis:** AI that *runs off* CuNi means portable agent **law** is CuNi; models are speech; exactness is the citizenship test.

**Build order (when unfrozen for this track):** P1 example agent pack → P2 one thin host → Studio “skills” UX.  
Until then: use Studio + this doc as the north star for “speak CuNi to the machines.”
