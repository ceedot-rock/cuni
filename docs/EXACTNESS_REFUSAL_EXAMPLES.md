# Exactness Refusal Examples

**Purpose**: Let a first-time visitor experience *refuse* as clearly as PASS.

Open [CuNi Studio](https://cuni-studio.fly.dev/), clear the editor, paste one of the snippets below, then hit **Run exactness**.

## 1. Platform-specific output (classic refuse)

```cuni
# This will FAIL exactness — stdout differs by host
ext host_name() -> str do
  py: ret "python"
  go: ret "go"
  js: ret "javascript"
end

say(host_name())
```

Expected: exactness FAIL (or compile refusal depending on `ext` handling). The three targets cannot produce identical stdout.

## 2. Non-portable float formatting (subtle)

```cuni
say(1 / 3)
```

Depending on runtime printing of floats, this can diverge. Prefer integer math or explicit portable formatting for PASS demos.

## 3. What *does* pass (control)

```cuni
def can_spend(amount: int, cap: int) -> bool do
    ret amount <= cap
end

say(can_spend(4, 5))
say(can_spend(9, 5))
```

This is the Studio default (`spend-control`). You should see **exactness PASS** and identical py/go/js stdout.

## Teaching point

Exactness is not “mostly the same.” It is byte-identical behavior on every supported target — or the program is refused. That is the citizenship gate for Agent-Rider.
