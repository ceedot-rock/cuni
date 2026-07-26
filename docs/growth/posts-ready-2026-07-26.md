# Ready-to-paste community posts — 2026-07-26

**Metrics at draft time:** 1★ · 0 forks · v0.1.6  
**Repo:** https://github.com/ceedot-rock/cuni  
**Status:** drafts only — **human must click submit** (HN auth, Reddit account, TWiR PR)  
**Paired action queue:** [`action-queue-2026-07-26.md`](action-queue-2026-07-26.md) (inline P0/P1 comments)  
**Scout:** [`conversation-opportunities-2026-07-26.md`](conversation-opportunities-2026-07-26.md)

Do not claim production maturity. Frame as early open-source experiment with a hard exactness gate.

---

## 1. Show HN (priority #1)

**When:** weekday US morning if possible · avoid weekends  
**URL:** https://news.ycombinator.com/submit  
**Type:** Show HN

### Title (copy-paste)

```
Show HN: CuNi – one source to exact Python, Go, and JS (or refuse)
```

**Alt titles** (if first is taken / want A-B later):

```
Show HN: CuNi – write once, identical py/go/js behavior or the compiler refuses
```

```
Show HN: CuNi 0.1 – multi-target language with a hard exactness contract
```

### Body (copy-paste)

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

### Reply kit (first comments / FAQs)

**Q: How is this different from a transpiler?**  
```
Most multi-target tools aim for “close enough.” CuNi’s gate is the opposite:
if a construct can’t map to identical behavior on py/go/js, it doesn’t emit.
cuni check is the product surface for that — three runtimes, same stdout, or fail.
```

**Q: Why three targets only?**  
```
Deliberately small surface so every construct has a proven mapping. Expanding
targets without an exactness story is how you get silent drift. v0.1 is the
contract + CI + interop demo first.
```

**Q: Can I use this in production?**  
```
No pitch for that yet — tag is v0.1.6. Useful today if you want a hard gate
on portable logic or a typed HTTP contract shared across languages (see link
tutorial). Feedback and failing cases welcome.
```

**Q: Written in what?**  
```
Compiler is Rust (cargo install). Targets are Python, Go, and JavaScript.
```

**Q: How is this different from Haxe / multi-target “write once”?**  
```
Haxe optimizes for breadth of backends. CuNi optimizes for a refusal property
on a tiny surface: if we can’t match behavior on py/go/js, we don’t emit.
Different tradeoff — not a Haxe replacement.
```

---

## 2. Reddit — r/ProgrammingLanguages (priority #1 with Show HN)

**URL:** https://www.reddit.com/r/ProgrammingLanguages/submit  
**Flair:** if available, Implementation / Project

### Title

```
CuNi: a small language with an exactness contract — one source → identical Python, Go, JS behavior, or refuse
```

### Body (markdown)

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

---

## 3. Reddit — r/rust (tooling / honest framing)

**URL:** https://www.reddit.com/r/rust/submit  
**Note:** CuNi is *written in* Rust; it does **not** target Rust. Lead with that.

### Title

```
[Project] CuNi 0.1.6 — Rust-written language: exact multi-target emit to Python/Go/JS + link interop
```

### Body

```markdown
**Honest framing:** CuNi is a small language **implemented in Rust**, not a Rust-target compiler. Sharing because the toolchain is cargo-native and the interesting bit is the exactness gate.

One `.cuni` source → Python + Go + JavaScript with a hard contract: identical behavior on all three, or refuse. `cuni check` runs all three and requires matching stdout. There’s also `link` for cross-language HTTP contracts (Go server ← py/js/go clients from one definition).

- https://github.com/ceedot-rock/cuni  
- Tutorial: https://github.com/ceedot-rock/cuni/blob/master/docs/LINK_TUTORIAL.md  
- `cargo install --git https://github.com/ceedot-rock/cuni --tag v0.1.6`

v0.1 — early. Feedback welcome, especially on compiler/UX and where refusal is too strict or not strict enough.
```

---

## 4. Reddit — r/golang (interop only — do **not** post “replace Go”)

**Only if** you want a third community post this week. Frame as interop demo.

### Title

```
Interop demo: one typed contract → Go HTTP server + Python/JS/Go clients (CuNi link)
```

### Body

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

**Skip r/Python / r/javascript** outbound this week unless Show HN gains traction — same interop framing only. (Inbound r/Python API-types comment is in action-queue C5 if thread still warm.)

---

## 5. This Week in Rust — Project/Tooling Updates blurb

**Rules (from TWiR README):**  
- **Not** a bare repo/crate link  
- Prefer tutorial / long-form / release writeup with how-to  
- Link text ≈ page title + small project description  
- One project per contributor per week  
- Submit PR against current draft in `draft/`

**Confirmed draft file (API 2026-07-26):** `draft/2026-07-29-this-week-in-rust.md`  
**PR target:** https://github.com/rust-lang/this-week-in-rust  
**Section:** `### Project/Tooling Updates`

### Preferred line (tutorial link — strongest fit)

```markdown
* [CuNi 0.1.6 — flagship tutorial: one link, three languages (exact multi-target py/go/js, Rust compiler)](https://github.com/ceedot-rock/cuni/blob/master/docs/LINK_TUTORIAL.md)
```

### Alt line (release + tutorial in description)

```markdown
* [Flagship tutorial: one `link`, three languages — CuNi 0.1.6 exact multi-target](https://github.com/ceedot-rock/cuni/blob/master/docs/LINK_TUTORIAL.md)
```

### PR description (copy into GitHub PR body)

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

### Human steps for TWiR PR

1. Fork `rust-lang/this-week-in-rust` (or open branch if already forked)  
2. Edit `draft/2026-07-29-this-week-in-rust.md` (or whatever the current `draft/*.md` is)  
3. Under `### Project/Tooling Updates`, paste preferred bullet  
4. Open PR with the description above  
5. Do **not** also spam bare `github.com/ceedot-rock/cuni` alone  

---

## 6. Conversation scout — merged into action queue

**File:** [`conversation-opportunities-2026-07-26.md`](conversation-opportunities-2026-07-26.md)  
**Final copy-paste comments:** [`action-queue-2026-07-26.md`](action-queue-2026-07-26.md) §B–C  

### P0 (do this week)

| # | Thread | Action queue |
|---|--------|--------------|
| 1 | [HN Solod](https://news.ycombinator.com/item?id=48895199) sound transpile | B1 |
| 2 | [HN Jacquard](https://news.ycombinator.com/item?id=48894630) agent PL | B2 |
| 3 | [Reddit Diplomat](https://www.reddit.com/r/rust/comments/1u5u5j5/diplomat_multilanguage_ffi_for_rust_libraries/) | B3 |
| 4 | [X Nudge](https://x.com/Nekomya_Dev/status/2080672826500034652) multi-target | B4 |

### P1 (after outbound + P0)

| # | Thread | Action queue |
|---|--------|--------------|
| 5 | [r/golang compiles-to-Go](https://www.reddit.com/r/golang/comments/1sgobdq/list_of_programing_languages_that_compile_to_go/) | C1 |
| 6 | [X golden/contract tests](https://x.com/hwisesa23/status/2081263849039077664) | C2 |
| 7 | [r/Compilers multi-target](https://www.reddit.com/r/Compilers/comments/1ugucyo/thoughts_on_multitarget_compilation/) | C3 |
| 8 | [HN WAYWO July](https://news.ycombinator.com/item?id=48884984) | C4 |
| 9 | r/Python API types (if not stale) | C5 |

### Standby (generic — only if a *new* live thread fits)

**A. Multi-target / transpile drift**

```
I’ve been hacking on a small language (CuNi) that treats multi-target as a
refusal problem: one source → py/go/js with identical stdout, or the compiler
won’t emit. cuni check is literally “run three backends, require matching
output.” Early (v0.1), but the interesting design choice is the hard gate
instead of approximate transpile. Tutorial if useful:
https://github.com/ceedot-rock/cuni/blob/master/docs/LINK_TUTORIAL.md
```

**B. Polyglot HTTP / shared API contract**

```
For shared contracts across languages we’ve been using a single link
definition that codegen’s a Go HTTP handler plus Python/JS clients (same path
+ JSON). Not a full framework — just typed interop codegen. Demo:
https://github.com/ceedot-rock/cuni/blob/master/docs/LINK_TUTORIAL.md
```

**C. Agent multi-runtime consistency**

```
If the pain is logic that must stay consistent across runtimes (not just FFI),
an exactness-checked multi-target emit can help: one source, three backends,
compiler refuses on divergence. Early OSS experiment:
https://github.com/ceedot-rock/cuni — happy to hear where the boundary is too
strict.
```

---

## 7. Optional tip drafts (not sent — prior list already covered)

Already emailed (see OUTREACH.md): TC, VB, Register, Ars, ZDNet, InfoWorld, SD Times, DevClass, TLDR, Rust Bytes, personal contacts.

**Hold unless human wants more tips:**

| Outlet | Angle | Note |
|--------|-------|------|
| Cooper Press (JS Weekly / Golang Weekly) | interop demo, not “new language” spam | contact form on cooperpress.com |
| Changelog News | submit form: changelog.com/news/submit | human submit |
| The New Stack | only if tips@ still valid | verify before send |

**No new cold emails this session** — priority is Show HN + Reddit + TWiR + P0 comments.

---

## 8. X / short amplification (optional, after Show HN is live)

```
CuNi 0.1.6: write once → identical Python, Go, JS behavior — or the compiler refuses.

cuni check = emit + run all three, require same stdout
link = one contract → Go server + py/js clients

https://github.com/ceedot-rock/cuni
```

Post only after Show HN URL exists so you can reply with the HN thread.

---

## Submit checklist (human)

| # | Action | Done? |
|---|--------|-------|
| 1 | Post Show HN (section 1) | ☐ |
| 2 | P0: HN Solod comment | ☐ |
| 3 | P0: HN Jacquard comment | ☐ |
| 4 | Post r/ProgrammingLanguages (section 2) | ☐ |
| 5 | Open TWiR PR (section 5) | ☐ |
| 6 | P0: Reddit Diplomat + X Nudge | ☐ |
| 7 | Post r/rust (section 3) | ☐ |
| 8 | P1 comments (golang list / golden X / Compilers / WAYWO) | ☐ |
| 9 | Optional: r/golang interop (section 4) | ☐ |
| 10 | Reply on Show HN with FAQ kit | ☐ |
| 11 | Log live URLs back into docs/OUTREACH.md | ☐ |
