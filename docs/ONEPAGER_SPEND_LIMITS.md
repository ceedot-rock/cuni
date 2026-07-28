# CuNi for spend / limits teams

**One line:** Critical spend rules that must mean the same in every runtime — or they don’t ship.

## The problem
Spend caps, floors, and approval thresholds often exist in more than one place (Python service, Go worker, JS agent tools). When they drift, you get disputes, overspend, or false denials.

## What CuNi does
- Write the rule **once**
- Prove **Python, Go, and JavaScript** agree (exactness)
- **Refuse** if they don’t
- Only then **run** or **register** the rule

## 30-second proof
1. Open https://cuni-studio.fly.dev/  
2. Flagship example loads: can I spend `amount` under `cap`?  
3. Click **Run exactness** → PASS or clear FAIL  
4. Optional: **Publish** (only after PASS)  
5. Agent mode: type `spend 4 cap 5` → same law

## Example rules
- Simple cap: amount ≤ cap  
- Reserve floor: leave a minimum after spend  
- Daily remaining budget  
- Dual-control threshold (flag when amount > threshold)  
- Tool allow only if tool is permitted **and** spend fits cap  

## Who it’s for
Teams that ship multi-runtime or agent automation where a wrong limit costs money or trust.

## What’s free / what’s next
- **Free:** public Studio demo  
- **Paid (planned):** private workspace — saved policies, history, audit trail  

## Links
- Studio: https://cuni-studio.fly.dev/  
- Plain walkthrough: [DEMO_PLAIN.md](DEMO_PLAIN.md)  
- Positioning: [POSITIONING.md](POSITIONING.md)  
- Release: [v0.1.7](https://github.com/ceedot-rock/cuni/releases/tag/v0.1.7)
