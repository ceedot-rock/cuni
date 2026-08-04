# Reply targets — 5 places + exact replies (2026-07-26)

**Rule:** answer *their* problem first; Studio link second; ask one question.  
**CTA:** https://cuni-studio.fly.dev/

---

## 1) r/ProgrammingLanguages — monthly “What are you working on?”

**Thread:** https://www.reddit.com/r/ProgrammingLanguages/comments/1t0dafn/may_2026_monthly_what_are_you_working_on_thread/  
**Why:** Open invitation to share WIP languages; normal to link a demo.  
**Tone:** peer update, not ad.

**Paste:**
```
Working on CuNi (Code:uNiTY) — small multi-target language with an
“exactness” contract: one source must behave identically on Python, Go,
and JavaScript, or the compiler refuses (no approximate transpile mode).

Free browser Studio (emit + real `cuni check` + Agent mode):
https://cuni-studio.fly.dev/

Curious what this community would change about the exactness boundary
(what should count as “identical,” how large the surface can grow).
```

---

## 2) r/ProgrammingLanguages — compile-to-JS ecosystem

**Thread:** https://www.reddit.com/r/ProgrammingLanguages/comments/1er0lif/what_is_the_state_of_the_compiletojs_language/  
**Why:** They’re comparing multi-target / JS backends; exactness is a different angle.  
**Tone:** add a dimension (not “use mine instead of TS”).

**Paste:**
```
Different angle from “best JS target language”: I’m more interested in
*same program, same behavior across py/go/js* than in a single rich target.

CuNi’s gate is: emit + run all three, require identical stdout, or refuse.
Not a TS competitor — deliberately small surface so exactness is checkable.

If useful to poke at: https://cuni-studio.fly.dev/ (no install)
Would love critique on whether “byte-identical stdout” is the right metric.
```

---

## 3) r/ProgrammingLanguages — “where new languages are needed”

**Thread:** https://www.reddit.com/r/ProgrammingLanguages/comments/1ikm7jh/where_are_the_biggest_areas_that_need_a_new/  
**Why:** Opportunity to name polyglot/agent drift as the niche.  
**Tone:** problem statement + optional demo.

**Paste:**
```
One niche I keep hitting: agent + polyglot stacks where the *same* policy
has to run in Python and also in Go/JS, and ports silently diverge.

I’m trying a hard gate rather than a richer language: CuNi refuses unless
py/go/js agree (exactness). Free try: https://cuni-studio.fly.dev/

Do you see that as a real gap, or is “just pick one language” always enough?
```

---

## 4) X — agents in Python *and* JS (efficiency / multi-runtime)

**Post:** https://x.com/AIByJohannes/status/2080341952537489493  
**Text (context):** “JavaScript is not a very efficient language. Neither is Python. Your agent is likely using both.”  
**Why:** Exact multi-runtime *law* is the reply; not “use one language.”

**Paste (reply):**
```
Using both is the point for a lot of stacks — the failure mode is policy
drifting between them.

I’ve been experimenting with “law in one source, exact on py/go/js or refuse”
(CuNi Studio free): https://cuni-studio.fly.dev/

Curious if you’d want a hard exactness gate, or just pick the faster runtime.
```

---

## 5) X — prove agent code works / not only Python

**Post:** https://x.com/dysinger/status/2081223415642587582  
**Text (context):** Require advanced langs + prove code works; “Using python hobbles you.”  
**Why:** You agree on *proof*; offer multi-runtime exactness as a lighter proof layer.

**Paste (reply):**
```
Agree that “prove it runs as advertised” matters more than JSON tools.

Different cut: not “escape Python,” but “same law on py *and* go/js or refuse.”
That’s the CuNi exactness gate (try free): https://cuni-studio.fly.dev/

Would you take multi-runtime exactness as a proof layer, or only full formal methods?
```

---

## Bonus (if you have energy)

### HN — Go for AI agents (polyglot reality)
**Thread:** https://news.ycombinator.com/item?id=47222270  
**Paste (comment):**
```
Go is great for agents that stay in one runtime. The messy case is still
“Python for the model glue, Go for the worker, JS on the edge” with the
same business rules.

I’ve been playing with a small language that refuses unless py/go/js match
(exactness): https://cuni-studio.fly.dev/

Is multi-runtime identity a real problem for people here, or edge-case?
```

---

## Order to post (tonight / this week)

1. **#1** monthly working-on (lowest friction)  
2. **#4** or **#5** on X (if you’re logged in)  
3. **#3** problem-gap thread  
4. **#2** only if the compile-to-JS thread is still active  
5. Bonus HN if you already comment there sometimes  

After each: paste the public URL into `docs/OUTREACH.md`.
