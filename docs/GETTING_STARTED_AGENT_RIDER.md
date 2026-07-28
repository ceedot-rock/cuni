# Getting Started with Agent-Rider + CuNi

**Exact multi-runtime agents, coordinated.**

CuNi is a small language with one hard rule: a program either produces identical behavior in Python, JavaScript, and Go — or the compiler refuses.

Agent-Rider is the coordination layer (identity, messaging, multi-agent workflows).

They fit together like this:

1. **Write** critical agent policies and skills in CuNi (inside the free [Studio](https://cuni-studio.fly.dev/)).
2. **Verify** them with the exactness checker — the same logic produces the same results on every supported runtime.
3. **Deploy** into Agent-Rider. Rider uses CuNi `link` contracts as the standard interop mechanism and requires exactness before a policy can run.

## Try it in 90 seconds

1. Open [CuNi Studio](https://cuni-studio.fly.dev/)
2. Load an example (or try `spend-control`)
3. Hit **Run exactness**

You will see the same program emit and run as Python, Go, and JavaScript — or a clear refusal.

## Flagship example: spend control

```cuni
def can_spend(amount: int, cap: int) -> bool do
    ret amount <= cap
end

link CheckSpend(amount: int, cap: int) -> bool do
    ret can_spend(amount, cap)
end
```

This is the “law is CuNi” idea in its simplest form: a decision that must be identical no matter which language runtime the agent is using.

## Next steps

- Explore the full [link interop tutorial](LINK_TUTORIAL.md)
- See the [main README](../README.md) for install and deeper docs
- Watch for the publish path that will let Studio-verified policies move into Rider

---

CuNi + Agent-Rider — exact multi-runtime agents, coordinated.
