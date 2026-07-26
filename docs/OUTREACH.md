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
