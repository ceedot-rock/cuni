# $CuNi pump.fun token — launch package

**Playground (primary CTA):** https://cuni-studio.fly.dev/


**Status:** Ready to create. Blocked only on SOL in hot wallet.

| Field | Value |
|-------|--------|
| **Name** | `Code:uNiTY` |
| **Symbol** | `CuNi` |
| **Logo** | `cuni-logo-1000.png` (1000×1000, from `assets/logo.png`) |
| **Alt logo** | `cuni-logo-512.png` |
| **Website** | https://github.com/ceedot-rock/cuni |
| **Twitter/X** | https://x.com/ceedotrock *(update if handle differs)* |
| **Telegram** | *(leave empty unless channel exists)* |
| **Creator hot wallet** | `ZHq1SCjyr2fReu9VmtpmZwDEbo1uzbrZMqPuY2ivb9V` |

## Description (paste-ready)

```
CuNi (Code:uNiTY) — open-source language: one source → exact Python, Go & JavaScript. Same behavior on all three or the compiler refuses.

v0.1.6 live · MIT · cuni check exactness · link interop (Go server + py/js clients)

Repo: github.com/ceedot-rock/cuni
Install: cargo install --git https://github.com/ceedot-rock/cuni --tag v0.1.6
Release: github.com/ceedot-rock/cuni/releases/tag/v0.1.6
Tutorial: github.com/ceedot-rock/cuni/blob/master/docs/LINK_TUTORIAL.md
Proof: cuni check examples/full.cuni  → exactness: PASS (py/go/js)

Not a financial product. Token is a community signal for the CuNi language project — real software you can install and run today. Build agents, polyglot contracts, and shared logic without port drift.
```

## Short blurb (if character-limited)

```
CuNi = Code:uNiTY. Write once → identical Python, Go, JS — or compile fails. v0.1.6 open source. github.com/ceedot-rock/cuni
```

## Traffic-driving links (priority order)

1. **Website field:** https://github.com/ceedot-rock/cuni  
2. **In description:** release v0.1.6, LINK_TUTORIAL, install one-liner  
3. **Optional secondary site:** https://agentrider.xyz  
4. **Contact:** ceedotrock@gmail.com · GitHub @ceedot-rock  

## Funding path (required before on-chain create)

Hot Solana signer (we control):  
`ZHq1SCjyr2fReu9VmtpmZwDEbo1uzbrZMqPuY2ivb9V` → **currently 0 SOL**

Recommended: send **≥ 0.05 SOL** (create + fees + small buy). For ~$50 deployment: convert ETH→SOL and send **all** remaining after bridge fees to the hot address above.

Uncontrolled RH ETH (~0.027 ETH ≈ $50):  
`0x64E31E05583F250644b76d0FFe12e129ea4DeeCe` — **no private key on this machine**.  
You must move funds from that wallet yourself (or import its key into env).

## Launch command

```bash
cd /tmp/pump-launch   # or projects/cuni/scripts/pump-launch
node create-cuni-token.js
```

After success: share `https://pump.fun/<MINT>` + GH + install line on X / Farcaster / Discord.
