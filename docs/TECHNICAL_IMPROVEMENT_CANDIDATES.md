# Technical Improvement Candidates (CuNi)

**Purpose**: Evidence list so the single highest-leverage technical step can be chosen deliberately.

**Date**: 2026-07-28

## Candidates (unordered)

1. **Emit fidelity / stdout normalization**  
   Edge cases where py/go/js diverge on whitespace, float printing, or empty collections. Tightening the exactness runner’s normalization would reduce false FAILs and make the gate more trustworthy.

2. **Typeck diagnostics**  
   More precise error messages and recovery suggestions (especially around `link`, modules, and fallible types). High visibility for new users in Studio.

3. **`link` surface expansion**  
   Additional portable patterns (e.g. simple request/response shapes beyond the current demo) while keeping the “exact or refuse” contract.

4. **Agent speech → law coverage**  
   Broader, more robust parsing of natural “spend / budget / score / echo” phrases into the existing skill pack, plus a clear path for new skills written in CuNi.

5. **Studio quota / cost observability**  
   Better visibility of remaining free-tier capacity and clearer refusal messages when limits hit (already partially present).

6. **Compiler performance / incremental checks**  
   Faster `cuni check` for multi-file / module graphs so Studio feels instant even on larger examples.

## Recommendation heuristic

Prefer the change that:
- Makes the exactness promise more believable to a first-time visitor, **or**
- Unlocks the next integration point with Agent-Rider (publish → register → run),
- While staying inside the locked integration model (`link` + exactness gate + Studio→Rider).

Pick one, ship it, measure, then regenerate the candidate list.
