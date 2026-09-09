# Lab laws (CuNi)

These programs are the meaning of prices and gates users hit. Exactness must PASS on native seats (`py,go,js,c,cpp,rs`).

| File | Protects | Gold stdout |
|------|----------|-------------|
| `suite-meter.cuni` | AWARE/TRUSTREAM hosted meter: 2 GiB free / month, then 8¢/GiB, $1 min | `0 0 100 144` |
| `catalog-plans.cuni` | SKU list prices in cents | `900 4900 49000 900 9900 7900 79000 0 49000` |
| `rider-fee.cuni` | Agent-Rider 5% task fee | `0 4 5 50` |
| `spend-control.cuni` | Agent spend cap (1 = allow) | `1 0 1` |

Prove a foreign implementation:

```bash
cuni prove examples/laws/suite-meter.cuni --against path/to/printer.py
```
