# CuNi Protocol

**Canonical JSON:** https://cuni-studio.fly.dev/.well-known/cuni-protocol.json  
**This document:** https://cuni-studio.fly.dev/PROTOCOL.md  
**Source:** https://github.com/ceedot-rock/cuni/blob/master/PROTOCOL.md

CuNi is 119 languages. One program. Same stdout on every catalog seat, or the compiler refuses. That rule is the protocol.

## 1. Exactness

A CuNi program with no `ext` blocks **emits and runs** on every language in the catalog. Stdout must match, or `cuni check` exits 1.

`cuni run` evaluates in-process (no emit). That runner is a **seat**: `cuni check` also runs it and refuses if it diverges from catalog gold. Programs with `ext` skip the in-process seat.

Native seats today: Python, Go, JavaScript, TypeScript, C, C++, Rust. Other catalog ids still emit+run (Python lowering until that seat is native). `--receipt` records `native` vs `lowering`.

A citizen receipt (`cuni check --receipt`) carries `source_hash`: SHA-256 of the `.cuni` bytes. The program is that hash, not the path. Agent-Rider refuses register if a claimed hash does not match the source.

## 2. Speech is not law

Models may talk. They may not ship. A program is a **citizen** only after exactness PASS. Agent-Rider refuses contract register unless `exactness.passed === true`.

## 3. Prove

Foreign code is judged by CuNi gold:

```
cuni prove file.cuni --against impl.py
```

If they diverge, the implementation is wrong.

## 4. Ingest

Reverse the protocol: `cuni ingest impl.py` produces CuNi, or refuses. v1 is a Python subset.

## 5. Money laws

Integer cents, same gold on every native seat:

| Law | Protects |
|-----|----------|
| `examples/laws/suite-meter.cuni` | 2 GiB free / month, then 8¢/GiB, $1 min |
| `examples/laws/catalog-plans.cuni` | Public SKU list prices |
| `examples/laws/rider-fee.cuni` | Rider 5% task fee |
| `examples/laws/spend-control.cuni` | Agent spend cap |

## 5b. Compressor laws

Formulas, fills, and order. Not Combined GC. Not host xz.

| Law | Protects |
|-----|----------|
| `examples/compressors/pcc-ops.cuni` | PCC1 op ids |
| `examples/compressors/never-expand.cuni` | coded ≥ raw → keep raw |
| `examples/compressors/zeros.cuni` | ZERO packed size |
| `examples/compressors/math-arith.cuni` | MTH1 u8 ramp keep |
| `examples/compressors/trustream.cuni` | 4 KiB tile gene order |

## 6. Where it lives

| Surface | URL |
|---------|-----|
| Studio | https://cuni-studio.fly.dev/ |
| Well-known | https://cuni-studio.fly.dev/.well-known/cuni-protocol.json |
| Protocol (lab site) | https://www.slidphilabs.com/.well-known/cuni-protocol.json |
| Agent-Rider MCP | https://agentrider.fly.dev/api/mcp |
| Agent-Rider card | https://agentrider.fly.dev/.well-known/agent.json |
| GitHub | https://github.com/ceedot-rock/cuni |

Not an RFC. Not an MCP server by itself. This is the exactness protocol agents and humans can fetch.
