# Getting started with CuNi

CuNi (Code:uNiTY) is a small programming language with one hard rule:
**a program produces identical output on every supported language target,
or it refuses to compile.** No approximate mode.

## Install

You need Rust (https://rustup.rs). Then:

```sh
git clone https://github.com/ceedot-rock/cuni.git
cd cuni
cargo install --path .
```

This puts `cuni` on your PATH (via `~/.cargo/bin`).

## Your first program

Save this as `hello.cuni`:

```cuni
say("hello from cuni")
let x = 40 + 2
say(x)
```

Run it through the exactness gate:

```sh
cuni check hello.cuni --only py,js,rs
```

Expected output:

```
emit/run 3/3 ok
exactness: PASS (3 langs)
```

`cuni check` emits your program in each target language, runs each one
with that language's real toolchain, and compares stdout byte-for-byte.
`--only` limits the gate to the seats you have installed; without it,
`check` runs all 144 catalog languages.

## What "exactness or refuse" means

Try something a target can't do:

```cuni
say(1.5)
```

```sh
cuni hello_float.cuni --emit sol hello_float.sol
```

```
cuni: emit refused for sol: Solidity refused: float literals have no Solidity form; refusing
```

CuNi refuses to emit rather than silently changing your program's meaning.
This is the product: the refusal *is* the guarantee.

## The 144-language catalog

`cuni check` without `--only` runs the full catalog: 144 language ids.
Ten seats — Python, Go, JavaScript, TypeScript, C, C++, Rust, Ruby, Lua,
and Solidity — compile through their real toolchains. The remaining 134
emit Python under a language-specific file extension and run under
`python3`, so every seat's stdout can actually be compared. List them:

```sh
cuni --list-langs
```

Emit any single seat directly:

```sh
cuni hello.cuni --emit rs hello.rs
```

## Solidity / EVM example

```cuni
say("hello from cuni")
let x = 40 + 2
say(x)
```

```sh
cuni hello.cuni --emit sol hello.sol
```

Produces a real contract (`contract CuniContract`) whose `run()` function
emits the program's output as Solidity events (`LogString`, `LogInt`).
Compile and deploy it with `solc` like any other contract — the same
source that passes `cuni check` on every other seat.

## Toolchains you need per seat

| Seat | Needs on PATH |
|------|---------------|
| py | `python3` |
| js, ts | `node` |
| go | `go` |
| c, cpp | `gcc` / `g++` |
| rs | `rustc` |
| rb | `ruby` |
| lua | `lua` |
| sol | `solc` (emit works without it; compiling needs it) |

If a toolchain is missing, `cuni check` tells you which seat failed and
why — use `--only` to gate just the seats you have.

## Next steps

- `cuni run hello.cuni` — evaluate in-process (quick, but not a substitute for `check`)
- `cuni ingest program.py -o program.cuni` — bring an existing Python/Go/JS program into CuNi, or refuse
- Read `SPEC.md` for the full language reference
