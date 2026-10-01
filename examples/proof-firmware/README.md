# Proof firmware — replace trust with proof

One thermostat controller, written once in CuNi, shipped to two native
targets — C and Rust — only if the two behave *identically*. Not "look
similar", not "passes review": the exactness gate compiles the same source
to both languages, runs both, and demands byte-identical output. If the gate
fails, nothing ships. That is the whole profile.

## What this proves

- **One source of truth.** `controller.cuni` is the entire control logic: a
  bang-bang thermostat with hysteresis (heater ON at or below 19.7 °C, OFF at
  or above 20.5 °C, holds state in between), stepping through a fixed,
  hard-coded sensor trace of 23 readings, printing the actuator decision per
  step (`HEAT_ON` / `HEAT_OFF`, plus `FAULT_SENSOR` on a bad reading, where
  the controller holds its last decision). Integer math only, fully
  deterministic.
- **Two targets, one behavior.** The C build and the Rust build print
  byte-identical decisions on the fixed trace. Verified by running both, not
  by reading the code.
- **Refusal is the default.** `promote.sh` promotes binaries into
  `released/` *only* after the gate passes. Any failure — a type error, a
  divergence between targets, a broken compiler in the chain — prints
  `REFUSED`, releases nothing, and exits non-zero.

## Files

| File | What it is |
|---|---|
| `controller.cuni` | The control logic, in CuNi. The only thing that can be shipped. |
| `promote.sh` | The refuse-to-promote gate: `cuni check --only c,rs --timeout 120`, then emit + native compile (gcc, rustc), then a post-compile stdout diff, then copy to `released/`. |
| `refuse-me.cuni` | Deliberately broken fixture (calls an undefined function) that the negative test uses to exercise the refusal path. Do not "fix" it. |

## Run it

```sh
# needs: cuni on PATH (or CUNI_BIN), gcc, rustc
./promote.sh                 # gates controller.cuni, releases to ./released/
./promote.sh refuse-me.cuni  # REFUSED, exit 1, no released/ created
```

`PROMOTE_DIR` can point the script at a copy of this directory (the
integration test does this) so `examples/` is never polluted with build
artifacts. `CUNI_BIN` overrides the cuni binary path.

## Honest scope

- This is a **behavioral** proof (byte-identical observed output on a fixed
  input trace), not a formal proof of the generated machine code.
- Integer logic only — the profile deliberately stays inside what both
  backends support exactly (no floats, no I/O, no concurrency).
- The fixed trace is the proof's boundary: identical decisions *on these
  inputs*. A different trace is a different claim and must pass the gate
  again.
- The gate trusts the toolchains (gcc, rustc) the same way any compiled
  firmware does; CuNi removes the *translation* risk between targets, not
  the toolchain risk.
