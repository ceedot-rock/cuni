# CuNi Demo Script

**Goal:** show the product promise in under a minute, then optional deeper paths.

**Live Studio:** https://cuni-studio.fly.dev/  
**Release:** [v0.1.7](https://github.com/ceedot-rock/cuni/releases/tag/v0.1.7)  
**Plain English (non-technical):** [DEMO_PLAIN.md](DEMO_PLAIN.md)

---

## 30-second Studio demo (primary)

1. Open **[CuNi Studio](https://cuni-studio.fly.dev/)**  
   → `spend-control.cuni` loads by default (flagship law).

2. Click **Run exactness**.  
   → Expect **exactness PASS** and identical Python / Go / JavaScript (or a clear refuse).

3. Optional: click **Publish**.  
   → Exactness PASS → metadata stored → auto-register into Rider stub.  
   → Footer may show `registered: N`.

**Talk track:**  
*“One source. Identical py/go/js — or the compiler refuses. That policy is the same law Agent mode runs from speech.”*

---

## 60-second Agent demo

1. Switch to **Agent** mode.
2. Skill should default near **spend** (first in the list).
3. Speech: `spend 4 cap 5` → **Run skill**.  
   → Routes to CheckSpend / can_spend → exactness → host runtime (py/go/js).
4. Try `spend 9 cap 5` to show a different decision under the same law.

**Talk track:**  
*“Speech routes. Law is CuNi. Exactness is citizenship — it only runs if it passes.”*

---

## CLI demo (local)

```bash
# from repo root after cargo build --release
./examples/demo.sh
```

Or step by step:

```bash
cargo build --release
./target/release/cuni check examples/spend-control.cuni
./target/release/cuni check examples/full.cuni
python3 examples/agent/host/run_agent.py --message "spend 4 cap 5" --host go
./examples/link/demo.sh   # interop: Go server + py/js/go clients
```

---

## Refusal (show the gate)

In Studio, paste a tiny non-portable snippet or see  
[`docs/EXACTNESS_REFUSAL_EXAMPLES.md`](EXACTNESS_REFUSAL_EXAMPLES.md).

**Talk track:**  
*“No approximate mode. If the targets diverge, it refuses.”*

---

## Investor / Bob path

1. 30s Studio (Run exactness on spend-control).  
2. Publish once.  
3. Agent: `spend 4 cap 5`.  
4. Point at Progress / README CuNi + Agent-Rider section.

Use [DEMO_PLAIN.md](DEMO_PLAIN.md) when walking someone non-technical through the same steps.
