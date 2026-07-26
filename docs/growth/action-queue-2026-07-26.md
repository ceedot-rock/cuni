# CuNi action queue — 2026-07-26

**Pipeline status:** wired (scout → growth → this queue)  
**Stars at wire:** 1 · **Goal:** public posts + P0 comments  

Open this file and work **top to bottom**. Check boxes as you go. Paste resulting URLs into `docs/OUTREACH.md`.

---

## A. Outbound posts (human click — highest leverage)

### A1. Show HN ⏰ ~3 min
1. Open https://news.ycombinator.com/submit  
2. Paste from `posts-ready-2026-07-26.md` §1  
3. After post: record URL → OUTREACH  

### A2. r/ProgrammingLanguages ⏰ ~3 min
- https://www.reddit.com/r/ProgrammingLanguages/submit  
- Body: `posts-ready-2026-07-26.md` §2  

### A3. r/rust ⏰ ~3 min
- https://www.reddit.com/r/rust/submit  
- Body: `posts-ready-2026-07-26.md` §3 (Rust-*built* framing)  

### A4. This Week in Rust PR ⏰ ~10 min
- Fork https://github.com/rust-lang/this-week-in-rust  
- Add under Project/Tooling Updates in current `draft/`  
- Use tutorial/release link, not bare repo only  
- PR body in `posts-ready-2026-07-26.md` §5  

---

## B. Inbound comments (P0 — drafts ready)

### B1. HN Solod — sound transpile
- URL: https://news.ycombinator.com/item?id=48895199  
- Draft: `conversation-opportunities-2026-07-26.md` §1  

### B2. Reddit Diplomat multi-lang FFI
- URL: https://www.reddit.com/r/rust/comments/1u5u5j5/diplomat_multilanguage_ffi_for_rust_libraries/  
- Draft: conversation file §2  

### B3. X Nudge multi-target agent lang
- URL: https://x.com/Nekomya_Dev/status/2080672826500034652  
- Draft: conversation file §3  

---

## C. Re-run pipeline (agents)

```bash
# CLI note
./scripts/run-growth-pipeline.sh 2026-07-27

# In Grok:
#   Run workflow cuni-growth-pipeline with args {"date":"2026-07-27"}
# Or: /cuni-growth
```

| Agent | Type id |
|-------|---------|
| Scout | `cuni-conversation-scout` |
| Growth | `cuni-growth-exec` |
| Workflow | `cuni-growth-pipeline` |

---

## D. Already done (do not redo)

- [x] Press tips (TC, VB, Register, Ars, ZDNet, InfoWorld, SD Times, DevClass, TLDR)  
- [x] Warm emails (ptasz13, zo.computer, rust bytes)  
- [x] Agents + workflow files on disk  
- [x] v0.1.5 / v0.1.6 releases  

---

## E. After you post

Tell Grok: **“resume growth exec — I posted HN at &lt;url&gt;”**  
so OUTREACH is updated and scout can mine replies.
