# Removed Catalog Entries ("Fat Trim") — 2026-10-02

The CuNi catalog listed 144 languages. 31 entries were removed because they
can **never** be natively supported: no real, installable toolchain exists
anywhere (dead dialects, proprietary platform-only languages, duplicates,
or things that are not programming languages at all).

**Criteria for removal** (all must hold):
1. No open-source or freely-installable toolchain exists today, AND
2. No plausible path to one exists (dead, proprietary, or not a language).

**Kept when unsure**: every entry with any plausible native future was kept,
even if obscure (e.g. `st-iec` via matiec, `bat`/`ahk` as Windows-only but
real, HDLs/shaders as compute targets, config languages with real interpreters).

## Removed: duplicates (6)

| id | name | reason |
|----|------|--------|
| f77 | Fortran 77 | Duplicate of `f90` (Fortran). One Fortran seat is enough. |
| pgsql | PostgreSQL | Duplicate of `sql`. Dialect, not a distinct language seat. |
| tsql | T-SQL | Duplicate of `sql`. Dialect, not a distinct language seat. |
| plsql | PL/SQL | Duplicate of `sql`. Dialect, not a distinct language seat. |
| mm | Objective-C++ | Duplicate of `objc` (Objective-C). Same toolchain family (clang). |
| dpr | Delphi | Duplicate of `pas` (Pascal); also proprietary (Embarcadero, no open toolchain). |

## Removed: dead dialects, no maintained toolchain (6)

| id | name | reason |
|----|------|--------|
| as | ActionScript | Flash EOL 2020; no maintained compiler. Dead. |
| ceylon | Ceylon | Discontinued by Red Hat; no maintained toolchain. Dead. |
| frege | Frege | Discontinued (Haskell-for-JVM); no maintained toolchain. Dead. |
| eta | Eta | Discontinued (Haskell-for-JVM); no maintained toolchain. Dead. |
| cyclone | Cyclone | Discontinued safe-C dialect; no maintained toolchain. Dead. |
| cmm | C-- | Discontinued; no maintained toolchain. Dead. |

## Removed: proprietary, no open toolchain (6)

| id | name | reason |
|----|------|--------|
| apex | Salesforce Apex | Platform-only; no local compiler exists outside Salesforce. |
| sas | SAS | Proprietary (SAS Institute); no open toolchain. |
| wl | Wolfram Language | Proprietary (Mathematica); no open toolchain. |
| abap | ABAP | Proprietary (SAP); no open toolchain. |
| rpg | RPG | Proprietary (IBM i); no open toolchain. |
| jai | Jai | Closed beta (Jonathan Blow); no public toolchain. |

## Removed: not programming languages (11)

| id | name | reason |
|----|------|--------|
| graphql | GraphQL | Query language; no "run a program" semantics. |
| cypher | Cypher | Query language (Neo4j); no program semantics. |
| sparql | SPARQL | Query language; no program semantics. |
| tex | TeX | Typesetting system; not a programming language. |
| cmake | CMake | Build system; its scripting is for builds, not programs. |
| mk | Make | Build system; not a programming language. |
| sed | sed | Stream editor; not a programming language. |
| proto | Protocol Buffers | Interface definition language; not a programming language. |
| hcl | HCL | Config language (Terraform); not a programming language. |
| bicep | Bicep | Config language (Azure); not a programming language. |
| gcode | G-code | Machine-control codes; not a programming language. |

## Removed: esoteric / experimental without toolchain (2)

| id | name | reason |
|----|------|--------|
| holyc | HolyC | TempleOS esoteric joke; no real toolchain outside TempleOS. |
| cpp2 | Cpp2 | Herb Sutter's experimental proposal; no production toolchain. |

## Summary

- **Removed**: 31 entries
- **Remaining**: 113 entries (144 − 31)
- All removals documented above with per-entry reasons.
- Entries kept despite obscurity (with reason): `st-iec` (matiec exists),
  `bat`/`ahk` (real, Windows-only), `cu` (nvcc real), `vhdl`/`sv`/`glsl`/`wgsl`
  (real HDL/shader toolchains), `nix`/`jsonnet`/`dhall` (real interpreters),
  `qml` (Qt real), `openscad` (real), `datalog` (real implementations),
  `carbon` (Google toolchain exists), `vy` (Vyper — the onchain seat, not a duplicate).

---

# Second Cut: Focus Trim — 2026-10-02

**This is a different kind of removal.** The 31 entries above were cut because
they can *never* be natively supported. The 60 entries below are **real
languages with real toolchains** — cut purely as a focus decision: the catalog
is now the top 50 plus the onchain chain profiles. Nothing below reflects on
the quality of these languages; they were cut for scope, not for cause.

**Criteria**: every catalog entry beyond position 50, except the onchain
chain profiles (`vy`, `move`, `cairo` — the shipped 0.8.0 Onchain Division).

**Preserved**: `vy` (Vyper), `move` (Move), `cairo` (Cairo) — onchain profiles.

## Removed: real languages, out of scope (60)

| id | name | note |
|----|------|------|
| scm | Scheme | Real (Guile/Racket). Out of top-50 scope. |
| rkt | Racket | Real. Out of top-50 scope. |
| el | Emacs Lisp | Real. Out of top-50 scope. |
| fnl | Fennel | Real (Lua-family). Out of top-50 scope. |
| hy | Hy | Real (Python-family). Out of top-50 scope. |
| raku | Raku | Real. Out of top-50 scope. |
| mojo | Mojo | Real (Modular). Out of top-50 scope. |
| coffee | CoffeeScript | Real. Out of top-50 scope. |
| elm | Elm | Real. Out of top-50 scope. |
| purs | PureScript | Real. Out of top-50 scope. |
| idr | Idris | Real. Out of top-50 scope. |
| lean | Lean | Real. Out of top-50 scope. |
| re | Reason | Real. Out of top-50 scope. |
| res | ReScript | Real. Out of top-50 scope. |
| sml | Standard ML | Real. Out of top-50 scope. |
| gleam | Gleam | Real. Out of top-50 scope. |
| vala | Vala | Real. Out of top-50 scope. |
| odin | Odin | Real. Out of top-50 scope. |
| cu | CUDA | Real (nvcc). Out of top-50 scope. |
| pde | Processing | Real. Out of top-50 scope. |
| wat | WebAssembly | Real (text format). Out of top-50 scope. |
| ll | LLVM IR | Real. Out of top-50 scope. |
| zsh | Zsh | Real shell. Out of top-50 scope. |
| fish | Fish | Real shell. Out of top-50 scope. |
| bat | Batch | Real (Windows). Out of top-50 scope. |
| octave | Octave | Real (MATLAB-compatible). Out of top-50 scope. |
| bas | BASIC | Real. Out of top-50 scope. |
| io | Io | Real. Out of top-50 scope. |
| eiffel | Eiffel | Real. Out of top-50 scope. |
| vhdl | VHDL | Real HDL. Out of top-50 scope. |
| sv | SystemVerilog | Real HDL. Out of top-50 scope. |
| glsl | GLSL | Real shader. Out of top-50 scope. |
| wgsl | WGSL | Real shader. Out of top-50 scope. |
| ahk | AutoHotkey | Real. Out of top-50 scope. |
| rexx | Rexx | Real. Out of top-50 scope. |
| forth | Forth | Real. Out of top-50 scope. |
| nix | Nix | Real. Out of top-50 scope. |
| st-iec | IEC Structured Text | Real (matiec). Out of top-50 scope. |
| openscad | OpenSCAD | Real. Out of top-50 scope. |
| pony | Pony | Real. Out of top-50 scope. |
| qml | QML | Real (Qt). Out of top-50 scope. |
| jsonnet | Jsonnet | Real. Out of top-50 scope. |
| dhall | Dhall | Real. Out of top-50 scope. |
| bal | Ballerina | Real. Out of top-50 scope. |
| xtend | Xtend | Real. Out of top-50 scope. |
| gosu | Gosu | Real. Out of top-50 scope. |
| netrexx | NetRexx | Real. Out of top-50 scope. |
| agda | Agda | Real. Out of top-50 scope. |
| curry | Curry | Real. Out of top-50 scope. |
| clean | Clean | Real. Out of top-50 scope. |
| roc | Roc | Real. Out of top-50 scope. |
| koka | Koka | Real. Out of top-50 scope. |
| ats | ATS | Real. Out of top-50 scope. |
| chapel | Chapel | Real. Out of top-50 scope. |
| carbon | Carbon | Real (Google). Out of top-50 scope. |
| pawn | Pawn | Real. Out of top-50 scope. |
| slang | S-Lang | Real. Out of top-50 scope. |
| datalog | Datalog | Real. Out of top-50 scope. |
| mercury | Mercury | Real. Out of top-50 scope. |
| oz | Oz | Real. Out of top-50 scope. |

## Summary (both cuts)

- **First cut**: 31 entries (never natively supportable) — 144 → 113
- **Second cut**: 60 entries (real languages, out of top-50 scope) — 113 → 53
- **Remaining**: 53 entries = top 50 + 3 onchain profiles (vy, move, cairo)
- All 91 removals documented with per-entry reasons.
