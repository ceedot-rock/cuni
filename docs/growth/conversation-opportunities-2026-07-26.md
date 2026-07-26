# Conversation scout report — 2026-07-26

**Product:** CuNi (v0.1.6) — exact multi-target language → Python + Go + JS from one source (or refuse).  
`cuni check` · `link` cross-language HTTP contracts · MIT · https://github.com/ceedot-rock/cuni  

**Mission:** live/recent public threads where a helpful, non-spammy CuNi mention is on-topic.  
**Default:** draft only — **do not post** unless growth exec explicitly says “post” / “comment”.  
**Scout pass:** full re-scan 2026-07-26 (afternoon) — refreshed priors + new finds.

---

## Scan window / queries used

| Window | ~2026-04 → 2026-07-26 preferred; high-signal older threads kept only as cite/watch |
|--------|----------------------------------------------------------------------------------|
| **HN** | Solod transpiler semantics; Jacquard agent PL; Ask HN WAYWO Jul; Loreline/Haxe/Ceramic; polyglot monorepo; OpenAPI/SDK |
| **Reddit** | r/rust Diplomat; r/Compilers multi-target; r/golang “compiles to Go”; r/Python generated API types |
| **X** | Nudge multi-target agent lang; contract/golden tests for agents; deterministic transpile; program-design / agent slop |
| **GitHub** | goscript (Go→TS correctness); Diplomat multi-lang FFI ecosystem |
| **Lobsters** | transpile / polyglot / codegen (many threads cold or off-topic this week) |

**High-relevance filters:** multi-target / transpile exactness · polyglot contracts · agent multi-lang emit · shared API types · compiler pedagogy · CI gates for generated code.  
**Downranked:** natural-language polyglot spam · dead Show HNs (0 comments) · pure crypto · closed multi-year threads · enterprise support asks.

---

## Top opportunities (ranked)

| P | Source | URL | Why relevant | Draft | Action / effort |
|---|--------|-----|--------------|-------|-----------------|
| **P0** | HN · Solod (Go→C subset) | https://news.ycombinator.com/item?id=48895199 | **209 pts / 174c**, ~9d. Live debate: sound transpile = type-checked AST → legal names + full semantics, not string rewrite. CuNi’s “exact or refuse” is the same philosophy on py/go/js. | §1 | **Comment** · ~5 min · under kfreds/ventana transpile-semantics subthread |
| **P0** | HN · Jacquard (AI-written PL) | https://news.ycombinator.com/item?id=48894630 | **102 pts / 59c**, ~12d. Language for agent-written, human-reviewed code; effects, replay, content-addressed identity. Adjacent: exact multi-target emit + `check` as review/CI gate when agents spit py/go/js. | §2 | **Comment** · ~5 min · lead with verification, not product pitch |
| **P0** | Reddit · Diplomat multi-lang FFI | https://www.reddit.com/r/rust/comments/1u5u5j5/diplomat_multilanguage_ffi_for_rust_libraries/ | **109↑ / 26c**, Jun 14. Author thread (Manishearth). Multi-lang *surfaces* from one Rust core. Complementary: portable *logic* + HTTP peers when you don’t need ABI. | §3 | **Comment** · ~5 min · disclose authorship; contrast not compete |
| **P0** | X · Nudge (agent lang → py/ts) | https://x.com/Nekomya_Dev/status/2080672826500034652 | Jul 24. Peer OSS: typed agent DSL, **compiles to Python & TypeScript**, traces-as-tests, CI. Natural peer exchange on multi-target + determinism. Low reach (~250 views) but perfect fit. | §4 | **Reply** · ~3 min · peer-builder tone, one link |
| **P1** | Reddit · r/golang compiles-to-Go list | https://www.reddit.com/r/golang/comments/1sgobdq/list_of_programing_languages_that_compile_to_go/ | **102↑ / 27c**, Apr 9. Catalog of langs targeting Go; authors of Sky/stew self-promote. CuNi emits *to* Go (among others) with exactness gate — fits “why target Go / multi-backend” discussion. | §5 | **Comment** · ~5 min · add to catalog framing carefully (CuNi is multi-target, not Go-only) |
| **P1** | X · contract/golden tests for agents | https://x.com/hwisesa23/status/2081263849039077664 | **Today (Jul 26)**. Differential/golden + contract tests so agent output doesn’t drift. Direct fit for `cuni check` as three-runtime golden gate. Low views so far — early reply can land. | §6 | **Reply** · ~3 min · value-first on golden/diff tests |
| **P1** | Reddit · r/Compilers multi-target | https://www.reddit.com/r/Compilers/comments/1ugucyo/thoughts_on_multitarget_compilation/ | Jun 27, multi-ISA handwritten backends. Pivot: high-level multi-target (py/go/js) has *semantic* not ABI hazards — refuse constructively. Thread still open-ish. | §7 | **Comment** · ~5 min · technical, no pitch first |
| **P1** | HN · Ask WAYWO July 2026 | https://news.ycombinator.com/item?id=48884984 | **292 pts / 1k+c**, ~12d. Monthly “what are you working on?” — legitimate place to *introduce* CuNi as OP’s work, not spam a foreign thread. Still getting late replies. | §8 | **Top-level comment** · ~5 min · short, honest, one link |
| **P1** | Reddit · r/Python API types gen | https://www.reddit.com/r/Python/comments/1qetxz1/do_you_prefer_manually_written_or_generated_api/ | Jan 2026, low score but on-topic (schema-first multi-lang clients). CuNi `link` = one typed HTTP contract → py/js/go. | §9 | **Comment if not stale** · 5 min · answer OP first |
| **P1** | HN · Loreline (Haxe multi-target) | https://news.ycombinator.com/item?id=47555035 | **75 pts / 20c**, Mar 2026. Explicit multi-target narrative language. Commenters care about multi-lang embedding. Aging — only if still active or for citation. | §10 | **Watch / cite** · low unless revived |
| **P2** | X · Uncle Bob agent swarm Go | https://x.com/unclebobmartin/status/2065161022185316672 | Jun 11, **~41k views**. 1558 lines of slop for Go hello world. Angle: agents need small surfaces + hard gates, not more process theater. Thread coldish. | §11 | **Optional reply** · only if still feels warm |
| **P2** | X · deterministic transpile (Arthur) | https://x.com/ArthurReyn/status/2081315267666145595 | **Today**, Python→C deterministic transpile wish. Tiny engagement — not worth much time. | §12 | **Skip or one-liner** · low reach |
| **P2** | GitHub · goscript (Go→TS) | https://github.com/s4wave/goscript | Active Go→TS transpiler; correctness bugs reinforce “exact multi-target is hard.” Peer OSS only. | §13 | **Watch** · engage on technical issues only |
| **P2** | HN · Ceramic / Haxe consistency | https://news.ycombinator.com/item?id=44472812 | Jul 2025. Top comment: Haxe APIs not consistent enough across backends. **Exact CuNi differentiator** as citation, not drive-by. | §14 | **Cite only** when multi-target HN spikes |

---

## Draft comments (copy-ready)

### §1 — HN Solod (P0)

> The “transpiler must resolve symbols and emit legal target names from a type-checked AST” point is the whole game. I’ve been working on the opposite direction of “better C”: a small language that targets Python, Go, and JS with an exactness gate — same program, same behavior on all three, or the compiler refuses the construct (`cuni check`). Most multi-target tools paper over semantic drift; refusing feels closer to how people talk about soundness in this thread. Early OSS if useful: https://github.com/ceedot-rock/cuni

### §2 — HN Jacquard (P0)

> The verification angle is what stuck with me: agents will fill in details whether or not the intent was right. One failure mode I keep hitting is multi-language emit — an agent ships Python, then “the same” logic in Go/JS, and prod finds they diverged. I’ve been experimenting with a tiny language that only emits py/go/js when behavior matches under a three-runtime check (`cuni check`), otherwise refuses. Complements effect/replay systems rather than competing: gate the portable surface, review the rest. Early OSS: https://github.com/ceedot-rock/cuni

### §3 — Reddit Diplomat (P0)

> Diplomat’s “one Rust library, many language surfaces” story is exactly the pain I’ve hit from the other side — wanting shared *logic* (not only FFI bindings) that still runs as first-class Python/Go/JS. I built CuNi as a small language that emits those three with an exactness check (identical behavior or refuse) plus a `link` mode for typed HTTP+JSON peers across languages. Complements FFI tools rather than replacing them: use Diplomat when you need a native library ABI; use something like CuNi when the boundary is portable source or HTTP contracts. Repo if anyone’s curious: https://github.com/ceedot-rock/cuni (v0.1, MIT).

### §4 — X Nudge (P0)

> Fellow multi-target language person — CuNi takes a related cut: one small source → exact Python + Go + JS (or the compiler refuses), with `cuni check` as a CI gate. Your traces-as-tests / determinism angle is the same family of problem as semantic drift across backends. Would be curious how Nudge handles constructs that don’t map cleanly between py and ts. https://github.com/ceedot-rock/cuni

### §5 — Reddit r/golang compiles-to-Go (P1)

> Nice list. Slightly different cut: I’ve been working on a small language that targets Go *and* Python *and* JS from one source, with a hard exactness gate (`cuni check`) so it only emits when behavior matches across all three runtimes — otherwise it refuses the construct. Go is a great target for the reasons people list here (simple runtime, deploy story); the hard part for multi-target is silent semantic drift between backends. Early OSS if anyone’s collecting multi-backend experiments: https://github.com/ceedot-rock/cuni

### §6 — X contract/golden tests for agents (P1)

> Strong list — especially differential/golden and contract tests. One gate we’ve found useful for multi-language agent emit: compile one small portable source to Python, Go, and JS, run the same harness on all three, and fail CI if stdout diverges (or if a construct can’t map exactly). Forces the “refuse” path instead of silent drift. Early OSS experiment: https://github.com/ceedot-rock/cuni (`cuni check` + `link` for HTTP peers)

### §7 — Reddit r/Compilers multi-target (P1)

> On high-level multi-target (different *languages*, not ISAs), the pitfall that bit me hardest is silent semantic drift: a construct that “works” on two backends but changes meaning on a third. For a small language I’m building (Python/Go/JS targets), the rule became: if you can’t prove matching behavior, refuse emit and fail `check` in CI. That kept the frontend honest more than any backend tweak. Curious whether you plan any cross-target semantic tests beyond ABI layout.

### §8 — HN Ask WAYWO July (P1)

> Working on CuNi — a small OSS language that compiles to exact Python, Go, and JavaScript from one source (or the compiler refuses). Motivation: AI agents and polyglot monorepos keep emitting “the same” logic in three languages that quietly diverge in prod. `cuni check` runs the three backends and fails on drift; `link` is for typed HTTP+JSON contracts across language peers. MIT, early (v0.1): https://github.com/ceedot-rock/cuni — feedback on the refuse-gate design welcome.

### §9 — Reddit r/Python API types (P1)

> Schema-first has won for me when clients span more than one language. OpenAPI generators are uneven; what I want is a single contract that produces matching clients without a Java-heavy toolchain. I open-sourced a small experiment: CuNi `link` defines a typed HTTP+JSON contract once and emits Go server + Python/JS/Go clients from the same source, with `cuni check` for portable logic. Early (v0.1) and not an OpenAPI replacement — but the “one truth, multi-lang peers” idea may match what you’re aiming at: https://github.com/ceedot-rock/cuni

### §10 — HN Loreline (P1 — aging)

> The multi-target embedding angle is what sold me too — most narrative tools lock you into one host language. I’ve been exploring the hard version of that problem for general code: emit Python/Go/JS only when behavior can match exactly across all three, otherwise refuse. Haxe-style breadth is impressive; exactness-first is a different tradeoff for smaller surfaces. Early experiment: https://github.com/ceedot-rock/cuni

### §11 — X Uncle Bob agent swarm (P2)

> The 24 lines of app vs 1500 of process is a good warning. One pattern I’ve liked for agent output that *must* land in multiple languages: shrink the portable surface and put a hard equivalence gate in CI (same program → py/go/js with matching behavior, or refuse). Less “more agents,” more “smaller language + check.” Early: https://github.com/ceedot-rock/cuni

### §12 — X Arthur deterministic transpile (P2, optional)

> For programming-language→programming-language, deterministic transpile is doable *if* you constrain the source and refuse unmappable constructs. Full Python→efficient C is a research problem; a tiny language with an exactness gate across a few targets is more tractable. We’ve been experimenting that way for py/go/js: https://github.com/ceedot-rock/cuni

### §13 — goscript (P2, technical only)

> Don’t drive-by star-beg. If engaging on a correctness issue:

> “Been following single-target Go→TS correctness work — the encoding/json class of bugs is exactly why multi-target tools need a hard equivalence gate. We use three-runtime stdout identity (`cuni check`) as CI for a small multi-target language; happy to compare test harness ideas if useful.”

### §14 — HN Ceramic / Haxe (cite only)

> That inconsistency-across-backends complaint is the main reason I stopped treating multi-target as “compile everything and hope.” CuNi’s bet is the inverse: a tiny language surface and a hard exactness gate for py/go/js — identical stdout under `cuni check` or no emit. Not a Haxe replacement; just an experiment in refusing approximate multi-target. https://github.com/ceedot-rock/cuni

---

## Priority summary for growth exec

| Priority | Do this week | Effort |
|----------|--------------|--------|
| **P0** | HN Solod comment (§1) | 5 min |
| **P0** | HN Jacquard comment (§2) — **new this pass** | 5 min |
| **P0** | Reddit Diplomat comment (§3) | 5 min |
| **P0** | X Nudge peer reply (§4) | 3 min |
| **P1** | r/golang compiles-to-Go (§5) — **new** | 5 min |
| **P1** | X golden/contract tests (§6) — **new, today** | 3 min |
| **P1** | r/Compilers multi-target (§7) | 5 min |
| **P1** | HN WAYWO July self-intro (§8) | 5 min |
| **Hold** | Loreline / Ceramic / monorepo — revival or citation only | — |
| **Avoid** | Mass-replying dead Show HNs; natural-language polyglot X; drive-by goscript | — |

**Do not post** without explicit go-ahead. Prefer author disclosure (“I built…”). One link max. Lead with the OP’s problem.

---

## Skipped (and why)

| Candidate | Why skipped |
|-----------|-------------|
| HN Show HN: Transpilatron / Drift / Zinc | Dead (0–few comments) |
| HN DuckDB SQL transpiler | SQL dialect, not language multi-target |
| HN Ironwall / Ü / Mog languages | Adjacent PL Show HNs; weak multi-target-exactness hook without spam |
| Lobsters crustc (Rust→C), PureNix, Clojure→Fennel | Cool transpile but not py/go/js polyglot contracts |
| Lobsters HTML/ZIP/PNG polyglot files | Binary polyglot, not PLs |
| X natural-language “polyglot” learners / GoDaddy multi-lang sites | Off-topic |
| X Gergely Orosz “code reviews fading” (~256k views) | High reach but weak CuNi fit; would look like engagement bait |
| X dexhorthy “program design” agent slop | Good culture thread; product mention would force-fit |
| r/programming Devin / generative AI agents | Broad AI coding discourse; not multi-target specific |
| r/ProgrammingLanguages multitarget (2018) | Ancient |
| Temporal “SDKs for 8 languages” HN | 2 pts, no discussion |
| Mitchell Hashimoto Go+agents (Apr) | Thread cold; productivity ≠ multi-target exactness |

---

## Suggested next scan queries

1. HN Algolia (daily): `transpiler`, `polyglot monorepo`, `multi-target`, `Haxe`, `OpenAPI codegen`, `exact semantics`, `Show HN language`, `agent code`
2. Reddit: `site:reddit.com/r/rust OR r/golang OR r/ProgrammingLanguages "OpenAPI" OR transpile OR polyglot` (past month); r/Compilers new
3. X: `("compiles to" OR transpiler) (Python OR Go) (TypeScript OR JavaScript) since:YYYY-MM-DD` + semantic “exact multi-language codegen” / “golden test agent”
4. GitHub: issues on goscript / Diplomat / OpenAPI generators mentioning “correctness”, “semantic”, “multi language”
5. Lobsters tags: `compilers`, `plt`, `programming` + transpile/polyglot (note: site often quiet mid-week)
6. Watch for: next Haxe release post, next “write once run anywhere” skepticism spike, agent-harness posts about multi-language emit breaking CI
7. **Own channel:** once Show HN from `posts-ready-2026-07-26.md` lands, scout *replies* for secondary comment opportunities
8. Revisit **Ask HN WAYWO** monthly cadence for next month if July thread freezes

---

## Notes for next scout

- **Best live fit right now:** Solod (HN), Jacquard (HN, new), Diplomat (Reddit), Nudge (X). All within ~6 weeks; Solod still the highest-signal semantic debate.
- **New this pass:** Jacquard (agent PL + verification), r/golang compiles-to-Go list, X golden/contract testing (same-day).
- **Differentiator that lands:** Haxe-style breadth *without* backend consistency → CuNi’s refuse gate. Quote Ceramic carefully; never trash Haxe authors.
- **Do not** claim CuNi is a Rust *target* language; it is Rust-*built*, multi-target to py/go/js.
- Pair with `posts-ready-2026-07-26.md` (owned Show HN / subreddit intros) — scout is inbound; posts-ready is outbound.
- Solod thread is ~9 days old with 174 comments — still commentable but prefer a high-quality reply under the sound-transpile subthread rather than a top-level me-too.

**File path:** `/home/cee/projects/cuni/docs/growth/conversation-opportunities-2026-07-26.md`
