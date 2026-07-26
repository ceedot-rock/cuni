# CuNi action queue — 2026-07-26 (growth merge)

**Pipeline:** scout → growth merge · **Do not auto-post** (human clicks only)  
**Stars:** 1 · **Forks:** 0 · **Tag:** v0.1.6  
**Sources:** `conversation-opportunities-2026-07-26.md` · `posts-ready-2026-07-26.md`  
**Rule:** one link max · disclose “I built…” · lead with OP’s problem · no production claims  

Work **top to bottom**. Check boxes. Paste live URLs into `docs/OUTREACH.md`.

---

## Top 5 human clicks (do these first)

| # | Action | Time | Done? |
|---|--------|------|-------|
| 1 | **Show HN** — submit post | ~3 min | ☐ |
| 2 | **HN Solod** — P0 comment (sound transpile) | ~5 min | ☐ |
| 3 | **HN Jacquard** — P0 comment (agent PL + verify) | ~5 min | ☐ |
| 4 | **r/ProgrammingLanguages** — outbound post | ~3 min | ☐ |
| 5 | **TWiR PR** — Project/Tooling bullet | ~10 min | ☐ |

Then: Diplomat Reddit · Nudge X · r/rust · WAYWO · golden-tests X.

---

## A. Outbound posts (owned channels)

### A1. Show HN ⏰ ~3 min · HIGHEST LEVERAGE

1. Open https://news.ycombinator.com/submit  
2. **Type:** Show HN  
3. Paste title + body below  
4. After post: record URL → `docs/OUTREACH.md`  
5. Keep FAQ reply kit open (`posts-ready` §1)

**Title**

```
Show HN: CuNi – one source to exact Python, Go, and JS (or refuse)
```

**Body**

```
CuNi is a small open-source language (v0.1.6, MIT) that compiles one source
file to Python, Go, and JavaScript under an "exactness" contract: same
program, same behavior on all three targets — or the compiler refuses.

Proof surface:
- cuni check emits and runs all three backends and requires byte-identical stdout
- link: one typed HTTP+JSON contract → Go server + Python/JS/Go clients

I'm not claiming production readiness. The interesting bit is refusing
approximate multi-target emit instead of papering over semantic drift.

Repo: https://github.com/ceedot-rock/cuni
Tutorial (link interop): https://github.com/ceedot-rock/cuni/blob/master/docs/LINK_TUTORIAL.md
Release: https://github.com/ceedot-rock/cuni/releases/tag/v0.1.6

Install:
  cargo install --git https://github.com/ceedot-rock/cuni --tag v0.1.6

Quick proof:
  cuni check examples/full.cuni
  ./examples/link/demo.sh
```

- [ ] Posted · URL: _______________

---

### A2. r/ProgrammingLanguages ⏰ ~3 min

**URL:** https://www.reddit.com/r/ProgrammingLanguages/submit  
**Flair:** Implementation / Project if available

**Title**

```
CuNi: a small language with an exactness contract — one source → identical Python, Go, JS behavior, or refuse
```

**Body**

```markdown
**Pitch:** write once in CuNi → identical behavior on Python, Go, and JavaScript — or the compiler refuses.

**Why it might be interesting here:** most multi-target / transpile stories optimize for coverage. CuNi optimizes for a *refusal* property: if a construct can’t map with matching runtime behavior, it doesn’t emit. `cuni check` emits and runs all three targets and requires byte-identical stdout.

**Also:** `link` — one typed HTTP+JSON contract that generates a Go server handler plus Python/JS/Go clients (shared path/JSON shape). Flagship tutorial below.

**Status:** v0.1.6, MIT, early. Not claiming production readiness.

- Repo: https://github.com/ceedot-rock/cuni  
- Link tutorial: https://github.com/ceedot-rock/cuni/blob/master/docs/LINK_TUTORIAL.md  
- Exactness CI: https://github.com/ceedot-rock/cuni/actions/workflows/exactness.yml  
- Install: `cargo install --git https://github.com/ceedot-rock/cuni --tag v0.1.6`

Happy to take design criticism on the exactness boundary (what should count as “identical,” how large the language surface can grow, etc.).
```

- [ ] Posted · URL: _______________

---

### A3. r/rust ⏰ ~3 min (Rust-*built*, not Rust-target)

**URL:** https://www.reddit.com/r/rust/submit

**Title**

```
[Project] CuNi 0.1.6 — Rust-written language: exact multi-target emit to Python/Go/JS + link interop
```

**Body**

```markdown
**Honest framing:** CuNi is a small language **implemented in Rust**, not a Rust-target compiler. Sharing because the toolchain is cargo-native and the interesting bit is the exactness gate.

One `.cuni` source → Python + Go + JavaScript with a hard contract: identical behavior on all three, or refuse. `cuni check` runs all three and requires matching stdout. There’s also `link` for cross-language HTTP contracts (Go server ← py/js/go clients from one definition).

- https://github.com/ceedot-rock/cuni  
- Tutorial: https://github.com/ceedot-rock/cuni/blob/master/docs/LINK_TUTORIAL.md  
- `cargo install --git https://github.com/ceedot-rock/cuni --tag v0.1.6`

v0.1 — early. Feedback welcome, especially on compiler/UX and where refusal is too strict or not strict enough.
```

- [ ] Posted · URL: _______________

---

### A4. This Week in Rust PR ⏰ ~10 min

**Repo:** https://github.com/rust-lang/this-week-in-rust  
**Draft file:** `draft/2026-07-29-this-week-in-rust.md`  
**Section:** `### Project/Tooling Updates`  
**Rule:** tutorial link, **not** bare repo alone

**Bullet (paste under Project/Tooling Updates)**

```markdown
* [CuNi 0.1.6 — flagship tutorial: one link, three languages (exact multi-target py/go/js, Rust compiler)](https://github.com/ceedot-rock/cuni/blob/master/docs/LINK_TUTORIAL.md)
```

**PR description**

```
## Summary
Add CuNi 0.1.6 under Project/Tooling Updates.

## Why this fits
- Not a bare repo link: points at the flagship **tutorial** (link interop: Go server + Python/JS clients from one contract).
- Compiler is written in **Rust** (`cargo install`); targets are Python, Go, JavaScript under an exactness contract.
- Includes how-to + demo commands, not only a changelog.

## Link
https://github.com/ceedot-rock/cuni/blob/master/docs/LINK_TUTORIAL.md

Happy to reword the bullet if editors prefer different framing.
```

**Steps:** fork → edit draft → paste bullet → open PR with description above

- [ ] PR opened · URL: _______________

---

### A5. Optional — r/golang interop only (not “replace Go”)

**URL:** https://www.reddit.com/r/golang/submit  
**Only after** A1–A2 if bandwidth remains.

**Title**

```
Interop demo: one typed contract → Go HTTP server + Python/JS/Go clients (CuNi link)
```

**Body**

```markdown
Not “another language to replace Go.” Small open-source experiment for **shared contracts**.

You write one `link` definition; codegen produces a Go handler plus Python/JS/Go clients that speak the same JSON over HTTP. Demo:

```bash
./examples/link/demo.sh
# Python / JS / Go clients → Go server, same response
```

Tutorial: https://github.com/ceedot-rock/cuni/blob/master/docs/LINK_TUTORIAL.md  
Repo: https://github.com/ceedot-rock/cuni (v0.1.6, MIT)

Also has an “exactness” mode for same-file multi-target stdout checks across py/go/js — orthogonal to the server interop story.
```

- [ ] Posted · URL: _______________

---

## B. Inbound comments — P0 (final copy-paste)

### B1. HN Solod — sound transpile · P0 ⏰ ~5 min

**Thread:** https://news.ycombinator.com/item?id=48895199  
**Context:** ~209 pts / 174c · sound transpile = type-checked AST → legal names + full semantics  
**Placement:** prefer under the sound-transpile / “not string rewrite” subthread (kfreds/ventana area), not a me-too top-level

**Comment (copy-paste)**

```
The “transpiler must resolve symbols and emit legal target names from a type-checked AST” point is the whole game. I’ve been working on the opposite direction of “better C”: a small language that targets Python, Go, and JS with an exactness gate — same program, same behavior on all three, or the compiler refuses the construct (`cuni check`). Most multi-target tools paper over semantic drift; refusing feels closer to how people talk about soundness in this thread. Early OSS if useful: https://github.com/ceedot-rock/cuni
```

- [ ] Commented · URL: _______________

---

### B2. HN Jacquard — agent PL + verification · P0 ⏰ ~5 min

**Thread:** https://news.ycombinator.com/item?id=48894630  
**Context:** ~102 pts / 59c · language for agent-written, human-reviewed code  
**Angle:** verification / multi-lang drift — not product pitch

**Comment (copy-paste)**

```
The verification angle is what stuck with me: agents will fill in details whether or not the intent was right. One failure mode I keep hitting is multi-language emit — an agent ships Python, then “the same” logic in Go/JS, and prod finds they diverged. I’ve been experimenting with a tiny language that only emits py/go/js when behavior matches under a three-runtime check (`cuni check`), otherwise refuses. Complements effect/replay systems rather than competing: gate the portable surface, review the rest. Early OSS: https://github.com/ceedot-rock/cuni
```

- [ ] Commented · URL: _______________

---

### B3. Reddit Diplomat multi-lang FFI · P0 ⏰ ~5 min

**Thread:** https://www.reddit.com/r/rust/comments/1u5u5j5/diplomat_multilanguage_ffi_for_rust_libraries/  
**Context:** ~109↑ / 26c · author thread (Manishearth) · multi-lang *surfaces* from one Rust core  
**Tone:** complementary (FFI vs portable logic / HTTP), disclose authorship

**Comment (copy-paste)**

```
Diplomat’s “one Rust library, many language surfaces” story is exactly the pain I’ve hit from the other side — wanting shared *logic* (not only FFI bindings) that still runs as first-class Python/Go/JS. I built CuNi as a small language that emits those three with an exactness check (identical behavior or refuse) plus a `link` mode for typed HTTP+JSON peers across languages. Complements FFI tools rather than replacing them: use Diplomat when you need a native library ABI; use something like CuNi when the boundary is portable source or HTTP contracts. Repo if anyone’s curious: https://github.com/ceedot-rock/cuni (v0.1, MIT).
```

- [ ] Commented · URL: _______________

---

### B4. X Nudge multi-target agent lang · P0 ⏰ ~3 min

**Post:** https://x.com/Nekomya_Dev/status/2080672826500034652  
**Context:** peer OSS · compiles to Python & TypeScript · traces-as-tests · low reach, high fit  
**Tone:** peer-builder, one link, question back

**Reply (copy-paste)**

```
Fellow multi-target language person — CuNi takes a related cut: one small source → exact Python + Go + JS (or the compiler refuses), with `cuni check` as a CI gate. Your traces-as-tests / determinism angle is the same family of problem as semantic drift across backends. Would be curious how Nudge handles constructs that don’t map cleanly between py and ts. https://github.com/ceedot-rock/cuni
```

- [ ] Replied · URL: _______________

---

## C. Inbound comments — P1 (after P0 / outbound)

### C1. r/golang compiles-to-Go list · P1 ⏰ ~5 min

**Thread:** https://www.reddit.com/r/golang/comments/1sgobdq/list_of_programing_languages_that_compile_to_go/  
**Note:** CuNi is multi-target (Go among others), not Go-only — say so

**Comment (copy-paste)**

```
Nice list. Slightly different cut: I’ve been working on a small language that targets Go *and* Python *and* JS from one source, with a hard exactness gate (`cuni check`) so it only emits when behavior matches across all three runtimes — otherwise it refuses the construct. Go is a great target for the reasons people list here (simple runtime, deploy story); the hard part for multi-target is silent semantic drift between backends. Early OSS if anyone’s collecting multi-backend experiments: https://github.com/ceedot-rock/cuni
```

- [ ] Commented · URL: _______________

---

### C2. X contract/golden tests for agents · P1 ⏰ ~3 min · same-day freshness

**Post:** https://x.com/hwisesa23/status/2081263849039077664  
**Angle:** `cuni check` as three-runtime golden gate

**Reply (copy-paste)**

```
Strong list — especially differential/golden and contract tests. One gate we’ve found useful for multi-language agent emit: compile one small portable source to Python, Go, and JS, run the same harness on all three, and fail CI if stdout diverges (or if a construct can’t map exactly). Forces the “refuse” path instead of silent drift. Early OSS experiment: https://github.com/ceedot-rock/cuni (`cuni check` + `link` for HTTP peers)
```

- [ ] Replied · URL: _______________

---

### C3. r/Compilers multi-target · P1 ⏰ ~5 min

**Thread:** https://www.reddit.com/r/Compilers/comments/1ugucyo/thoughts_on_multitarget_compilation/  
**Tone:** technical first; product last

**Comment (copy-paste)**

```
On high-level multi-target (different *languages*, not ISAs), the pitfall that bit me hardest is silent semantic drift: a construct that “works” on two backends but changes meaning on a third. For a small language I’m building (Python/Go/JS targets), the rule became: if you can’t prove matching behavior, refuse emit and fail `check` in CI. That kept the frontend honest more than any backend tweak. Curious whether you plan any cross-target semantic tests beyond ABI layout.
```

- [ ] Commented · URL: _______________

---

### C4. HN Ask WAYWO July 2026 · P1 ⏰ ~5 min

**Thread:** https://news.ycombinator.com/item?id=48884984  
**Note:** legitimate self-intro (your work), not spam into a foreign product thread

**Comment (copy-paste)**

```
Working on CuNi — a small OSS language that compiles to exact Python, Go, and JavaScript from one source (or the compiler refuses). Motivation: AI agents and polyglot monorepos keep emitting “the same” logic in three languages that quietly diverge in prod. `cuni check` runs the three backends and fails on drift; `link` is for typed HTTP+JSON contracts across language peers. MIT, early (v0.1): https://github.com/ceedot-rock/cuni — feedback on the refuse-gate design welcome.
```

- [ ] Commented · URL: _______________

---

### C5. r/Python API types (only if not stale) · P1

**Thread:** https://www.reddit.com/r/Python/comments/1qetxz1/do_you_prefer_manually_written_or_generated_api/  
**Check first:** still open / recent activity; otherwise skip

**Comment (copy-paste)**

```
Schema-first has won for me when clients span more than one language. OpenAPI generators are uneven; what I want is a single contract that produces matching clients without a Java-heavy toolchain. I open-sourced a small experiment: CuNi `link` defines a typed HTTP+JSON contract once and emits Go server + Python/JS/Go clients from the same source, with `cuni check` for portable logic. Early (v0.1) and not an OpenAPI replacement — but the “one truth, multi-lang peers” idea may match what you’re aiming at: https://github.com/ceedot-rock/cuni
```

- [ ] Commented or skipped · URL: _______________

---

## D. Hold / skip

| Item | Why |
|------|-----|
| HN Loreline / Ceramic | Aging — cite only if multi-target HN spikes |
| X Uncle Bob agent swarm | High views but cold; optional only if warm |
| X Arthur deterministic transpile | Tiny reach |
| goscript issues | Peer OSS technical only — no drive-by star beg |
| Press tip re-sends | Already sent 2026-07-26 (see OUTREACH) |
| Bulk cold email | Forbidden this session |

---

## E. Already done (do not redo)

- [x] Press tips: TC, VB, Register, Ars, ZDNet, InfoWorld, SD Times, DevClass, TLDR  
- [x] Warm: ptasz13, neatmoon@zo.computer, rustaceanseditors@gmail.com (Rust Bytes)  
- [x] Scout report: `docs/growth/conversation-opportunities-2026-07-26.md`  
- [x] Posts pack: `docs/growth/posts-ready-2026-07-26.md`  
- [x] Growth merge: this action queue (inline final copy)  
- [x] v0.1.6 release + agents/workflow on disk  

---

## F. After you post

Tell Grok: **“resume growth exec — I posted HN at &lt;url&gt;”** (and Reddit/PR URLs)

So OUTREACH is updated, FAQ replies can be queued, and scout can mine thread replies.

---

## G. Re-run pipeline

```bash
./scripts/run-growth-pipeline.sh 2026-07-27
# In Grok: /cuni-growth  or  run workflow cuni-growth-pipeline with date 2026-07-27
```

| Agent | Type id |
|-------|---------|
| Scout | `cuni-conversation-scout` |
| Growth | `cuni-growth-exec` |
| Workflow | `cuni-growth-pipeline` |
