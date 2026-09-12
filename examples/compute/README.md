# Compute examples

Small algorithms in CuNi. `cuni run` executes one native seat. `cuni check` is the proof (identical stdout or refuse).

```bash
cuni run examples/compute/fib.cuni          # 55
cuni check examples/compute --timeout 180   # exactness: PASS (119 langs)
```

| File | What it runs | Gold stdout |
|------|----------------|-------------|
| `range.cuni` | `range` / `abs` / `min` / `max` / `str.len` / `slice` | `0\n3\n0\n1\n2\n4\n2\n9\n2\nell\n0\n` |
| `fib.cuni` | fib(10) | `55\n` |
| `gcd.cuni` | gcd(48, 18) | `6\n` |
| `sum-range.cuni` | sum of `range(10)` | `45\n` |
| `sort.cuni` | selection sort `[5,1,4,2]` | `1 2 4 5\n` |
