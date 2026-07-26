# CuNi user-growth outreach

**Owner:** Corey (ceedotrock@gmail.com)  
**Product:** https://github.com/ceedot-rock/cuni · tag `v0.1.6`

## Positioning (one line)

> Write once in CuNi → identical Python, Go, and JavaScript behavior — or the compiler refuses.

## Proof links

| Proof | Link / command |
|-------|----------------|
| Repo | https://github.com/ceedot-rock/cuni |
| Release | https://github.com/ceedot-rock/cuni/releases/tag/v0.1.6 |
| Exactness CI | https://github.com/ceedot-rock/cuni/actions/workflows/exactness.yml |
| Link tutorial | https://github.com/ceedot-rock/cuni/blob/master/docs/LINK_TUTORIAL.md |
| Install | `cargo install --git https://github.com/ceedot-rock/cuni --tag v0.1.6` |
| Demo | `./examples/link/demo.sh` |

## Email log (2026-07-26)

| To | Angle | Status |
|----|--------|--------|
| ptasz13@hotmail.com | Personal share / early try | sent |
| neatmoon@zo.computer | Zo/agent tooling collaborator | sent |
| rustaceanseditors@gmail.com | Rust Bytes tip | sent |

## Press release log (2026-07-26)

Full text: [`PRESS_RELEASE.md`](PRESS_RELEASE.md)

| To | Outlet type | Status |
|----|-------------|--------|
| tips@techcrunch.com | Tech press tips | sent |
| tips@venturebeat.com | Tech press tips | sent |
| tips@theregister.com | Tech press tips | sent |
| tips@arstechnica.com | Tech press tips | sent |
| tips@zdnet.com | Tech press tips | sent |
| news@infoworld.com | Dev press | sent |
| editors@sdtimes.com | Software dev times | sent |
| tips@devclass.com | Dev press | sent |
| hello@tldr.tech | Developer newsletter | sent |

## Pipeline wiring (2026-07-26)

| Component | Path / name |
|-----------|-------------|
| Growth agent | `.grok/agents/cuni-growth-exec.md` · spawn `cuni-growth-exec` |
| Scout agent | `.grok/agents/cuni-conversation-scout.md` · spawn `cuni-conversation-scout` |
| Workflow | `.grok/workflows/cuni-growth-pipeline.rhai` · `/cuni-growth-pipeline` |
| Skill | `.grok/skills/cuni-growth/SKILL.md` · `/cuni-growth` |
| Shell entry | `./scripts/run-growth-pipeline.sh` |
| Project rules | `AGENTS.md` |
| Action queue | `docs/growth/action-queue-YYYY-MM-DD.md` |

**Flow:** scout writes opportunities → growth merges action-queue + posts-ready → human submits → resume growth with URLs.

## Channels (do these too — not email)

### Hacker News — Show HN
**Title:** `Show HN: CuNi – one source to exact Python, Go, and JS (or refuse)`

**Text:**
```
CuNi is a small language that compiles to Python, Go, and JavaScript with an
"exactness" contract: same program, same behavior on all three targets, or
the compiler refuses.

cuni check emits and runs all three and requires identical stdout.

There's also link for cross-language HTTP contracts (one .cuni file → Go
server + Python/JS clients).

https://github.com/ceedot-rock/cuni
https://github.com/ceedot-rock/cuni/blob/master/docs/LINK_TUTORIAL.md

Install: cargo install --git https://github.com/ceedot-rock/cuni --tag v0.1.6
```

Post at: https://news.ycombinator.com/submit

### Reddit
- r/rust — tooling update framing (honest: compiler *written in* Rust, targets py/go/js)
- r/ProgrammingLanguages — language design / exactness contract
- r/golang, r/Python — only if framed as interop (`link`), not “replace Go/Python”

### This Week in Rust
Submit via PR to https://github.com/rust-lang/this-week-in-rust `drafts/`  
**Project/Tooling** needs more than a bare repo link — use release notes + tutorial as the link text, e.g.  
“CuNi 0.1.6: exact multi-target compile to Python/Go/JS + link interop demo”

### Product Hunt
Launch as a developer tool when you have a short video (link-demo.gif is a start).

## Do not
- Cold-email random local businesses about CuNi
- Mass-mail no-reply digests
- Claim production readiness beyond v0.1

---

## Growth exec session — 2026-07-26 (posts-ready)

**Agent:** cuni-growth-exec  
**Metrics at session start:** **1★** · **0 forks** · **0 open issues** · tag `v0.1.6`  
**Goal:** ship ready-to-paste community posts + TWiR blurb; log next human submits (no drive-by spam).

### Actions completed
| Action | Result |
|--------|--------|
| Read `docs/OUTREACH.md` + press/email log | Prior press tips + 3 personal/Rust Bytes emails already sent |
| Star count via `gh api` | 1 star, 0 forks |
| Wrote ready-to-paste posts | [`docs/growth/posts-ready-2026-07-26.md`](growth/posts-ready-2026-07-26.md) |
| Show HN title + body + FAQ reply kit | In posts-ready §1 |
| Reddit r/ProgrammingLanguages + r/rust (+ optional r/golang) | In posts-ready §2–4 |
| TWiR Project/Tooling bullet + PR description | Tutorial link (not bare repo) — posts-ready §5 |
| Conversation scout | **No** `docs/growth/conversation-opportunities-*.md` — standby comments only (§6) |
| New cold tip emails | **None** this session (prior tip list already covered major press/TLDR/Rust Bytes) |
| GH Discussion / README Community | **Skipped** (low value vs. human community submits) |

### Artifacts
- `docs/growth/posts-ready-2026-07-26.md` — full copy-paste pack

### Metrics
| Metric | Before | After |
|--------|--------|-------|
| GitHub stars | 1 | 1 (no community posts live yet) |
| Forks | 0 | 0 |
| Emails sent this session | — | 0 |
| Ready posts drafted | 0 | Show HN + 3 Reddit + TWiR + X blurb |

### Blocked / needs human
1. **Submit Show HN** — https://news.ycombinator.com/submit (text in posts-ready §1)  
2. **Submit r/ProgrammingLanguages** (posts-ready §2)  
3. **Open TWiR PR** against `rust-lang/this-week-in-rust` `draft/` with tutorial bullet (posts-ready §5)  
4. After live: paste HN/Reddit/PR URLs back into this log  

### Next 3 actions (queued for human)
1. Click-submit **Show HN** (weekday AM preferred).  
2. Post **r/ProgrammingLanguages**, then **r/rust** (honest: compiler written in Rust).  
3. Open **This Week in Rust** PR with link-tutorial bullet (not bare repo).  

### Do not re-send (already done 2026-07-26)
Press: tips@techcrunch, tips@venturebeat, tips@theregister, tips@arstechnica, tips@zdnet, news@infoworld, editors@sdtimes, tips@devclass, hello@tldr.tech  
Other: ptasz13@hotmail.com, neatmoon@zo.computer, rustaceanseditors@gmail.com  

---

## Growth exec session — 2026-07-26 (scout merge + action queue)

**Agent:** cuni-growth-exec  
**Metrics at session start:** **1★** · **0 forks** · **0 open issues** · tag `v0.1.6`  
**Goal:** merge scout P0/P1 into action-queue with final copy-paste; refresh posts-ready; no public posts, no bulk cold email.

### Actions completed
| Action | Result |
|--------|--------|
| Read `docs/OUTREACH.md` | Prior press + warm emails + first posts-ready session logged |
| Read scout report | `docs/growth/conversation-opportunities-2026-07-26.md` (P0×4, P1×5+) |
| Read prior `posts-ready` + thin action-queue | Upgraded action-queue from pointers → full inline paste |
| Star count via GitHub API | 1 star, 0 forks (unchanged) |
| Confirmed TWiR draft | `draft/2026-07-29-this-week-in-rust.md` still current |
| Wrote full action queue | [`docs/growth/action-queue-2026-07-26.md`](growth/action-queue-2026-07-26.md) — outbound A1–A5 + P0 B1–B4 + P1 C1–C5 with final comments |
| Refreshed posts-ready | [`docs/growth/posts-ready-2026-07-26.md`](growth/posts-ready-2026-07-26.md) — Show HN / Reddit / TWiR + scout cross-links + Haxe FAQ |
| Public posts | **None** (human-only) |
| Cold tip emails | **None** this session |

### Artifacts
- `docs/growth/action-queue-2026-07-26.md` — primary click-through queue
- `docs/growth/posts-ready-2026-07-26.md` — outbound paste pack
- Scout source: `docs/growth/conversation-opportunities-2026-07-26.md`

### Metrics
| Metric | Before | After |
|--------|--------|-------|
| GitHub stars | 1 | 1 |
| Forks | 0 | 0 |
| Emails sent this session | — | 0 |
| Public posts this session | — | 0 |
| P0 comment drafts ready | partial | 4 (Solod, Jacquard, Diplomat, Nudge) |
| P1 comment drafts ready | partial | 5 (golang list, golden X, Compilers, WAYWO, r/Python) |

### Blocked / needs human
1. **Show HN** — https://news.ycombinator.com/submit (posts-ready §1 / action-queue A1)  
2. **P0 HN comments** — Solod + Jacquard (action-queue B1–B2)  
3. **r/ProgrammingLanguages** (action-queue A2)  
4. **TWiR PR** — `draft/2026-07-29-this-week-in-rust.md` (action-queue A4)  
5. After live: paste HN/Reddit/PR/comment URLs back into this log  

### Top 5 human clicks (queued)
1. Submit **Show HN**  
2. Comment **HN Solod** (sound transpile)  
3. Comment **HN Jacquard** (agent PL + verify)  
4. Post **r/ProgrammingLanguages**  
5. Open **This Week in Rust** PR (tutorial bullet)  

### Next 3 actions after top 5
1. Reddit **Diplomat** P0 + X **Nudge** peer reply  
2. **r/rust** outbound (Rust-built framing)  
3. P1: WAYWO self-intro + X golden/contract tests (same-day freshness)  

### Do not re-send (already done 2026-07-26)
Press: tips@techcrunch, tips@venturebeat, tips@theregister, tips@arstechnica, tips@zdnet, news@infoworld, editors@sdtimes, tips@devclass, hello@tldr.tech  
Other: ptasz13@hotmail.com, neatmoon@zo.computer, rustaceanseditors@gmail.com  

## Public posts LIVE (2026-07-26 — post-publicly request)

| Channel | URL |
|---------|-----|
| GH Discussion Announcements | https://github.com/ceedot-rock/cuni/discussions/1 |
| GH Discussion Show and tell | https://github.com/ceedot-rock/cuni/discussions/2 |
| Tracking issue | https://github.com/ceedot-rock/cuni/issues/3 |
| This Week in Rust PR | https://github.com/rust-lang/this-week-in-rust/pull/8468 |

**Blocked without human login session:** Show HN, Reddit posts/comments, X organic replies.
