# CuNi Studio — cost control & money

**Problem:** Hosted Studio on Fly runs real `cuni check` (emit + py/go/js). Heavy use = CPU + volume + bandwidth on *your* bill.

**Goal:** Stay free for discovery, hard-cap spend, charge only for sustained power users.

## What costs money today

| Resource | Current setup | Risk |
|----------|---------------|------|
| Fly machine | shared-cpu-1x, 1GB, `min_machines_running = 0`, auto-stop | Idle ≈ $0; cold start OK |
| Volume | 1GB `cuni_studio_data` | Small fixed |
| Concurrent runs | `CUNI_PLAYGROUND_MAX_CONCURRENT=2` | Good — keep |
| Timeout | 45s | Good — keep |
| Agent / chat | More CPU per request | Highest risk if viral |

Fly shared-cpu with scale-to-zero is already the right baseline. The danger is **many concurrent exactness runs**, not idle time.

## Hard gates (implement / keep)

### 1. Machine & concurrency (ops)

```toml
# fly.toml — already mostly correct
auto_stop_machines = "stop"
auto_start_machines = true
min_machines_running = 0

[env]
  CUNI_PLAYGROUND_MAX_CONCURRENT = "2"   # never raise without paid plan
  CUNI_PLAYGROUND_TIMEOUT = "30"         # consider 30s for free tier
  CUNI_PLAYGROUND_FREE_DAILY = "40"      # runs per IP per day (new)
  CUNI_PLAYGROUND_AGENT_DAILY = "20"     # agent/chat runs per IP per day
```

### 2. Per-IP daily quota (app)

- Track `client IP → count` in `/data/quota.json` (volume).
- Free: 40 playground runs/day, 20 agent runs/day.
- Over quota → `429` + message: “Free daily limit. Run locally or upgrade.”
- Local CLI remains unlimited (users pay their own CPU).

### 3. Payload & abuse

- Max source size (already ~200KB) — keep.
- Reject empty / spam bodies.
- No LLM calls on free Studio by default (user’s key only via self-host).

### 4. Convert path (saves you money)

Always push heavy users off Fly:

1. Studio (taste)  
2. `cargo install …`  
3. Local `cuni check`  

CTA in 429 and in UI footer: **“Unlimited on your machine.”**

## Monetization (when traffic is real)

### Tier sketch

| Tier | Price | Limits |
|------|-------|--------|
| **Free** | $0 | IP quotas above; scale-to-zero Studio |
| **Pro** | ~$9–15/mo | Higher quotas, priority queue, longer timeout |
| **Team** | later | Shared quota, SSO later |

### Stripe shape (you already have Stripe connected)

1. One product: **CuNi Studio Pro**  
2. Checkout Session or Payment Link  
3. Webhook → store `customer_email` / `stripe_customer_id` → higher quota key  
4. Optional: magic-link login later; v1 can be “paste Pro code” or cookie after Checkout success  

Do **not** bill per exactness run at first — metering is complex and feels hostile for a compiler playground. Cap free, sell monthly capacity.

### What not to do early

- Don’t keep `min_machines_running = 1` (burns money 24/7).  
- Don’t raise concurrent above 2 on free.  
- Don’t call paid LLMs server-side without a user key / paid tier.  
- Don’t store huge Notelog growth without rotation (cap already ~500 entries).

## Fly spend alarms

1. Fly dashboard → set **billing alert** (e.g. $10 and $25).  
2. Weekly: check machine hours + volume.  
3. If viral: temporarily set `MAX_CONCURRENT=1` and lower daily free quota via env (no redeploy of logic if env-driven).

## Recommended next code (same PR or follow-up)

1. `quota.py` — IP daily counters on volume.  
2. Wire into `/api/run`, `/api/check`, `/api/agent/*`.  
3. Env: `CUNI_PLAYGROUND_FREE_DAILY`, `CUNI_PLAYGROUND_AGENT_DAILY`.  
4. UI: show remaining free runs.  
5. Stripe Payment Link for Pro when ready.

## One-line policy

> Free Studio is a demo with hard daily caps; serious use is local CLI; Pro is paid capacity on the host you operate.
