# Conversation scout report — 2026-07-26

**Product:** CuNi (v0.1.6) — exact multi-target language → Python + Go + JS from one source (or refuse).  
`cuni check` · `link` cross-language HTTP contracts · MIT · https://github.com/ceedot-rock/cuni  

**Mission:** live/recent public threads where a helpful, non-spammy CuNi mention is on-topic.  
**Default:** draft only — **do not post** unless growth exec explicitly says “post” / “comment”.

---

## Scan window / queries used

| Window | ~2026-04 → 2026-07-26 preferred; some high-signal older threads retained if still topical |
|--------|------------------------------------------------------------------------------------------|
| HN | Algolia `search_by_date` / `search`: transpiler, polyglot, multi-target, OpenAPI/SDK codegen, Haxe/Loreline, Solod, monorepo, agent codegen |
| Reddit | web + page open: r/rust Diplomat, r/Compilers multi-target, r/Python API types, r/ProgrammingLanguages multitarget |
| X | keyword + semantic: transpile/polyglot/multi-target, agent codegen, gRPC cross-language contracts, Nudge lang |
| GitHub | goscript issues (Go→TS correctness), Diplomat pure-Python backend, related SDK generators |
| Lobsters | newest + polyglot/transpile/OpenAPI queries |

**High-relevance filters applied:** multi-target / transpile exactness, polyglot contracts, agent multi-lang emit, shared API types, compiler pedagogy.  
**Downranked:** dead Show HNs (0 comments), natural-language polyglot spam, pure crypto/fitness, closed multi-year threads.

---

## Top opportunities (ranked)

| P | Source | URL | Why relevant | Draft comment | Action / effort |
|---|--------|-----|--------------|---------------|-----------------|
| **P0** | HN · Solod (Go→C subset) | https://news.ycombinator.com/item?id=48895199 | **209 pts**, Jul 13 2026. Live debate on *what a sound transpiler must do* (typed AST → legal names, full semantics, not string rewrite). CuNi’s “exact or refuse” gate is the same philosophy, three high-level targets instead of C. | See draft §1 | **Comment** · ~5 min · reply under a transpile/semantics subthread |
| **P0** | Reddit · Diplomat multi-lang FFI | https://www.reddit.com/r/rust/comments/1u5u5j5/diplomat_multilanguage_ffi_for_rust_libraries/ | **109↑ / 26c**, Jun 14 2026. Author thread (Manishearth). People choosing multi-lang *surfaces* (Py/JS/C++) from one Rust core. Complementary story: when you want portable *logic* + HTTP peers rather than FFI layers. | See draft §2 | **Comment** · ~5 min · disclose authorship; contrast not compete |
| **P0** | X · Nudge (agent lang → py/ts) | https://x.com/Nekomya_Dev/status/2080672826500034652 | Jul 24 2026. Peer OSS language: typed agent DSL, **compiles to Python & TypeScript**, traces-as-tests, CI. Natural peer exchange on multi-target emit + determinism gates. Low volume but high topical fit. | See draft §3 | **Reply** · ~3 min · peer-builder tone, one link max |
| **P1** | HN · Loreline (Haxe multi-target) | https://news.ycombinator.com/item?id=47555035 | **75 pts / 20c**, Mar 28 2026. Explicit multi-target narrative language (C++/C#/JS/Java/Py/Lua). Commenters call out multi-language embedding as the draw. | See draft §4 | **Comment if still active** · or bookmark for next Loreline/Haxe spike |
| **P1** | Reddit · r/Compilers multi-target | https://www.reddit.com/r/Compilers/comments/1ugucyo/thoughts_on_multitarget_compilation/ | Jun 27 2026. Author just landed multi-ISA codegen; pitfalls of diverse backends. Pivot: high-level multi-target (py/go/js) has *semantic* not ABI hazards — refuse constructively. | See draft §5 | **Comment** · ~5 min · technical, no product pitch first |
| **P1** | HN · Ceramic / Haxe | https://news.ycombinator.com/item?id=44472812 | **93 pts**, Jul 2025. Top comment: Haxe “doesn’t live up to its promises… API isn’t consistent enough for complex applications to work with all of its backends.” **Exact CuNi differentiator.** Thread older — use only if replying to a *new* multi-target HN and citing this pain, or if Ceramic resurfaces. | See draft §6 | **Watch / cite** · low post priority unless revived |
| **P1** | Reddit · Python API types gen | https://www.reddit.com/r/Python/comments/1qetxz1/do_you_prefer_manually_written_or_generated_api/ | Jan 2026. Schema-first vs hand clients; multi-language SDKs. CuNi `link` = one typed HTTP contract → py/js/go clients without OpenAPI toolchain pain. | See draft §7 | **Comment** · ~5 min · answer OP first, link second |
| **P1** | HN · polyglot monorepo Changesets | https://news.ycombinator.com/item?id=47845274 | **20 pts / 6c**, Apr 21 2026. Versioning across npm/PyPI/crates. Adjacent: shared *behavior* contract, not only package versions. | See draft §8 | **Light comment** · 3 min · only if thread still gets eyes |
| **P2** | X · gRPC polyglot contracts | https://x.com/0xJustUzair/status/2079552552455344582 | Jul 21 2026 thread: proto as cross-lang wire contract; ends “which internal call would you move first?” Space for small HTTP JSON + shared source alternative. Low engagement (dozens of views). | See draft §9 | **Optional reply** · low reach |
| **P2** | GitHub · goscript (Go→TS) | https://github.com/s4wave/goscript · issues around correctness (#142 closed Jul 2026) | **~229★**, active. Single-direction transpile with real correctness bugs — reinforces “exact multi-target is hard.” Peer OSS; open an issue or discussion only if offering a concrete comparison, not drive-by marketing. | See draft §10 | **Watch** · engage on technical issues only |

---

## Draft comments (copy-ready)

### §1 — HN Solod (P0)

> The “transpiler must resolve symbols and emit legal target names from a type-checked AST” point is the whole game. I’ve been working on the opposite direction of “better C”: a small language that targets Python, Go, and JS with an exactness gate — same program, same behavior on all three, or the compiler refuses the construct (`cuni check`). Most multi-target tools paper over semantic drift; refusing feels closer to how people talk about soundness in this thread. Early OSS if useful: https://github.com/ceedot-rock/cuni

### §2 — Reddit Diplomat (P0)

> Diplomat’s “one Rust library, many language surfaces” story is exactly the pain I’ve hit from the other side — wanting shared *logic* (not only FFI bindings) that still runs as first-class Python/Go/JS. I built CuNi as a small language that emits those three with an exactness check (identical behavior or refuse) plus a `link` mode for typed HTTP+JSON peers across languages. Complements FFI tools rather than replacing them: use Diplomat when you need a native library ABI; use something like CuNi when the boundary is portable source or HTTP contracts. Repo if anyone’s curious: https://github.com/ceedot-rock/cuni (v0.1, MIT).

### §3 — X Nudge (P0)

> Fellow multi-target language person — CuNi takes a related cut: one small source → exact Python + Go + JS (or the compiler refuses), with `cuni check` as a CI gate. Your traces-as-tests / determinism angle is the same family of problem as semantic drift across backends. Would be curious how Nudge handles constructs that don’t map cleanly between py and ts. https://github.com/ceedot-rock/cuni

### §4 — HN Loreline (P1)

> The multi-target embedding angle is what sold me too — most narrative tools lock you into one host language. I’ve been exploring the hard version of that problem for general code: emit Python/Go/JS only when behavior can match exactly across all three, otherwise refuse. Haxe-style breadth is impressive; exactness-first is a different tradeoff for smaller surfaces. Early experiment: https://github.com/ceedot-rock/cuni

### §5 — Reddit r/Compilers multi-target (P1)

> On high-level multi-target (different *languages*, not ISAs), the pitfall that bit me hardest is silent semantic drift: a construct that “works” on two backends but changes meaning on a third. For a small language I’m building (Python/Go/JS targets), the rule became: if you can’t prove matching behavior, refuse emit and fail `check` in CI. That kept the frontend honest more than any backend tweak. Curious whether you plan any cross-target semantic tests beyond ABI layout.

### §6 — HN Ceramic / Haxe consistency (P1 — cite, careful)

> That inconsistency-across-backends complaint is the main reason I stopped treating multi-target as “compile everything and hope.” CuNi’s bet is the inverse: a tiny language surface and a hard exactness gate for py/go/js — identical stdout under `cuni check` or no emit. Not a Haxe replacement; just an experiment in refusing approximate multi-target. https://github.com/ceedot-rock/cuni

### §7 — Reddit r/Python API types (P1)

> Schema-first has won for me when clients span more than one language. OpenAPI generators are uneven; what I want is a single contract that produces matching clients without a Java-heavy toolchain. I open-sourced a small experiment: CuNi `link` defines a typed HTTP+JSON contract once and emits Go server + Python/JS/Go clients from the same source, with `cuni check` for portable logic. Early (v0.1) and not an OpenAPI replacement — but the “one truth, multi-lang peers” idea may match what you’re aiming at: https://github.com/ceedot-rock/cuni

### §8 — HN polyglot monorepo (P1)

> Versioning polyglot packages is half the problem; the other half is behavioral drift between language implementations of the “same” API. We’ve been experimenting with one source that either emits exact py/go/js or refuses, plus a shared HTTP contract mode for services. Early OSS: https://github.com/ceedot-rock/cuni — mostly useful as a CI gate on portable logic today.

### §9 — X gRPC thread (P2)

> Agree on contract-first. For small internal polyglot services where HTTP JSON is enough, we’ve been trying one source language that emits matching Python/Go/JS peers under an exactness check, with `link` for the HTTP contract side — less protobuf ceremony, explicit refuse when targets would diverge. Early: https://github.com/ceedot-rock/cuni

### §10 — goscript (P2, technical only)

> Don’t drive-by star-beg. If engaging on a correctness issue, keep it technical:

> “Been following single-target Go→TS correctness work — the encoding/json class of bugs is exactly why multi-target tools need a hard equivalence gate. We use three-runtime stdout identity (`cuni check`) as CI for a small multi-target language; happy to compare test harness ideas if useful.”

---

## Priority summary for growth exec

| Priority | Do this week | Effort |
|----------|--------------|--------|
| **P0** | HN Solod comment (§1) | 5 min |
| **P0** | Reddit Diplomat comment (§2) | 5 min |
| **P0** | X Nudge peer reply (§3) | 3 min |
| **P1** | r/Compilers multi-target (§5) if still open | 5 min |
| **P1** | r/Python API types (§7) if not stale | 5 min |
| **Hold** | Loreline / Ceramic / monorepo — only on revival or as citations | — |
| **Avoid** | Mass-replying dead Show HNs (Transpilatron, Drift, Zinc) | — |

**Do not post** without explicit go-ahead. Prefer author disclosure (“I built…”). One link max. Lead with the OP’s problem.

---

## Skipped (and why)

| Candidate | Why skipped |
|-----------|-------------|
| HN Show HN: Transpilatron (Python→C AI) | 5 pts, **0 comments** — dead |
| HN Show HN: Drift (English→async Python) | 2 pts, 0 comments — dead |
| HN Zinc polyglot shared memory | 2 pts, noise comment — dead |
| HN DuckDB SQL transpiler (Jul 24) | SQL dialect, not language multi-target |
| HN Ask: JS→Python migration (Mar 2025) | >1 year old; consensus was “don’t transpile, rewrite” |
| Lobsters Verse (Jul 25) | Metaverse scripting PL debate; poor CuNi fit |
| Lobsters HTML/ZIP/PNG polyglot files | Binary polyglot, not programming languages |
| X natural-language “polyglot” learners | Off-topic |
| X GoDaddy polyglot websites | Marketing SEO |
| Mitchell Hashimoto Go+agents (Apr) | High reach but thread cold; agent-Go productivity ≠ multi-target exactness |
| HN Sideko (Aug 2025) | Good “deterministic codegen” theme but commercial SDK gen; 5 comments, stale |
| r/ProgrammingLanguages multitarget (2018) | Ancient |
| Temporal “SDKs for 8 languages” HN | 2 pts, no discussion |

---

## Suggested next scan queries

1. HN Algolia (daily): `transpiler`, `polyglot monorepo`, `multi-target`, `Haxe`, `OpenAPI codegen`, `exact semantics`, `Show HN language`
2. Reddit: `site:reddit.com/r/rust OR r/golang OR r/ProgrammingLanguages "OpenAPI" OR transpile OR polyglot` (past month)
3. X: `("compiles to" OR transpiler) (Python OR Go) (TypeScript OR JavaScript) since:YYYY-MM-DD` + semantic “exact multi-language codegen”
4. GitHub: issues on goscript / Diplomat / OpenAPI generators mentioning “correctness”, “semantic”, “multi language”
5. Lobsters tags: `compilers`, `plt`, `programming` + keywords transpile/polyglot
6. Watch for: next Haxe release post, next “write once run anywhere” skepticism spike, agent-harness posts about multi-language emit breaking CI
7. **Own channel:** once Show HN from `posts-ready-2026-07-26.md` lands, scout *replies* for secondary comment opportunities

---

## Notes for next scout

- **Best live fit right now:** Solod (HN), Diplomat (Reddit), Nudge (X) — all within ~6 weeks and on exact multi-target / multi-lang surfaces.
- **Differentiator that lands:** Haxe-style breadth *without* backend consistency → CuNi’s refuse gate. Quote Ceramic comment carefully; never trash Haxe authors.
- **Do not** claim CuNi is a Rust *target* language; it is Rust-*built*, multi-target to py/go/js.
- Pair this file with `posts-ready-2026-07-26.md` (owned Show HN / subreddit intros) — scout is inbound; posts-ready is outbound.

**File path:** `/home/cee/projects/cuni/docs/growth/conversation-opportunities-2026-07-26.md`
