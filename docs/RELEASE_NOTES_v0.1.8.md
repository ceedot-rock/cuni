# CuNi v0.1.8

Write one program. Print many coding languages. Python, Go, and JavaScript must still print the same thing, or the compiler refuses.

## Install

```bash
cargo install --git https://github.com/ceedot-rock/cuni --tag v0.1.8
```

Studio (no install): https://cuni-studio.fly.dev/

## What this release does

- `cuni --list-langs` — catalog of coding languages
- `cuni <file.cuni> --emit-all DIR` — write one file per language
- Studio language picker and `GET /api/langs`
- Exactness is unchanged: `cuni check` still runs Python, Go, and JavaScript and requires identical stdout

The extra printers share the same AST. They are not a claim that every language was executed.

## Not in this tag

- Exactness on languages other than Python, Go, and JavaScript
- A crate on crates.io (install from the git tag)
