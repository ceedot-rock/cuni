# Compressor laws (CuNi)

Public meaning of PCC genes that are formulas, fills, and order.

Each file **encodes and decodes**. `cuni check` emit+runs the catalog; stdout must match.

```bash
# exactness + run
./scripts/run-compressors.sh

# or
cuni check examples/compressors --timeout 180
```

| File | What it runs | Gold stdout |
|------|----------------|-------------|
| `pcc-ops.cuni` | op ids + known(0/11/12) | `0 1 2 3 4 5 6 7 8 9 10 11 1 1 0` |
| `never-expand.cuni` | keep(raw,coded) | `7 8 8 40 8 10` |
| `zeros.cuni` | pack zeros; decode len+sum | `0 7 8 8 8 8 16 0 1` |
| `math-arith.cuni` | ARITH_U8 encode/decode roundtrip | `440 11 11 10 3 41 1` |
| `trustream.cuni` | 4 KiB law; run 16-byte ZERO/MATH/STORE tiles | `4096 0 8 0 11 11 136 3 16` |

MATCH / BWT / CMAQ / LZ stay in the lab Rust tree.
