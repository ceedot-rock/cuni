# CuNi in plain English — what you’re seeing

This is a short tour of CuNi Studio. No install. No terminal required.

Open: **https://cuni-studio.fly.dev/**

---

## Step 1 — The page loads

You’re looking at a free online workspace for CuNi.

A small example program is already filled in. It’s a simple rule:  
**“Can I spend this amount if my limit is that amount?”**

Think of it like a spending rule written once, that should mean the *same thing* no matter which computer language runs it.

---

## Step 2 — Click “Run exactness”

CuNi checks that rule in **three** languages at once: Python, Go, and JavaScript.

- If all three agree → **PASS** (green). The rule is trustworthy.
- If they disagree → **FAIL**. CuNi refuses. Nothing fuzzy is allowed.

That’s the whole product idea: **same answer everywhere, or don’t ship it.**

---

## Step 3 — (Optional) Click “Publish”

Only after a PASS can you publish.

Publish means: “This rule passed the test — remember it.”  
Behind the scenes it stores a receipt and registers it in a simple holding place (we call that the Rider stub).

You don’t need to understand the plumbing. The point is: **only verified rules get registered.**

---

## Step 4 — Switch to “Agent” mode

Same product, different door.

Here you type a short sentence like a person would talk:

> spend 4 cap 5

Meaning: “I want to spend 4; my cap is 5.”

Click **Run skill**. CuNi turns that sentence into the same spending rule, checks it again, and runs it.

- Speech is how you *ask*.
- CuNi is the *law*.
- Exactness is the *citizenship test* before it runs.

---

## What to remember

| Everyday idea | In CuNi |
|---------------|--------|
| Write a rule once | One `.cuni` program |
| It must mean the same on every machine | Exactness PASS |
| If it doesn’t match, stop | Refuse — no “close enough” |
| Ask in plain words | Agent speech (e.g. spend 4 cap 5) |
| Only good rules get filed | Publish after PASS |

---

## One sentence for anyone

**CuNi lets you write an important rule once, proves it behaves the same in Python, Go, and JavaScript, and only then lets agents or systems use it.**

More detail (technical): [DEMO.md](DEMO.md)
