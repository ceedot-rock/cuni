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
