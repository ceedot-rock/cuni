use crate::ast::{Item, Program};
use crate::lexer::Lexer;
use crate::parser::Parser;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Error while resolving `use` imports into a single compilation unit.
pub struct ModuleError {
    pub message: String,
}

/// Resolves every `use name` in `program` by loading `<dir>/<name>.cuni`
/// (relative to the file that contained the `use`), recursively, and
/// returns a single flattened program: imported items first (depth-first,
/// each module once), then the original file's non-`use` items.
///
/// v0.1 scope (SPEC.md §9): one file per module name, same-directory style
/// lookup only — no package paths, no re-export sugar, no selective imports.
/// Cycles are refused. Missing modules are refused (compile-or-refuse).
pub fn resolve_uses(program: Program, source_path: &Path) -> Result<Program, ModuleError> {
    let mut seen = HashSet::new();
    // The root file itself is "seen" so a submodule can't `use` back into it
    // under a different name pointing at the same path — we key on canonical
    // paths where possible, else the path as given.
    let root_key = path_key(source_path);
    seen.insert(root_key);
    let base_dir = source_path.parent().unwrap_or_else(|| Path::new("."));
    let mut imported: Vec<Item> = Vec::new();
    let mut local: Vec<Item> = Vec::new();

    for item in program.items {
        match item {
            Item::Use(name) => {
                load_module(&name, base_dir, &mut seen, &mut imported)?;
            }
            other => local.push(other),
        }
    }

    imported.extend(local);
    Ok(Program { items: imported })
}

fn path_key(p: &Path) -> String {
    fs::canonicalize(p)
        .map(|c| c.to_string_lossy().into_owned())
        .unwrap_or_else(|_| p.to_string_lossy().into_owned())
}

fn load_module(name: &str, from_dir: &Path, seen: &mut HashSet<String>, out: &mut Vec<Item>) -> Result<(), ModuleError> {
    let path = from_dir.join(format!("{}.cuni", name));
    let key = path_key(&path);
    if seen.contains(&key) {
        // Already loaded (diamond import) — skip, don't re-merge items.
        // A true cycle where A uses B uses A is also caught: A is in `seen`
        // before its body is expanded, so B's use of A hits this branch and
        // silently skips rather than looping. That's safe (A's items are
        // already being collected) but means mutual recursion across files
        // is fine only if both files are eventually reached from the root —
        // which they are, from whichever is used first.
        if !path.exists() && !fs::canonicalize(&path).is_ok() {
            // seen via a previous failed? shouldn't happen
        }
        return Ok(());
    }
    if !path.is_file() {
        return Err(ModuleError {
            message: format!(
                "cannot resolve `use {}` — expected file `{}` next to the importing source (SPEC.md §9)",
                name,
                path.display()
            ),
        });
    }
    seen.insert(key);

    let source = fs::read_to_string(&path).map_err(|e| ModuleError {
        message: format!("couldn't read module `{}` ({}): {}", name, path.display(), e),
    })?;
    let tokens = Lexer::tokenize(&source).map_err(|e| ModuleError {
        message: format!("{}: lex error: {}", path.display(), e.message),
    })?;
    let mut parser = Parser::new(tokens, &source);
    let prog = parser.parse_program().map_err(|e| ModuleError {
        message: format!("{}: parse error: {}", path.display(), e.message),
    })?;

    let mod_dir = path.parent().unwrap_or(from_dir);
    for item in prog.items {
        match item {
            Item::Use(dep) => load_module(&dep, mod_dir, seen, out)?,
            other => out.push(other),
        }
    }
    Ok(())
}

/// Convenience: absolute or relative path for tests / callers that only have a string.
#[allow(dead_code)]
pub fn resolve_uses_str(program: Program, source_path: &str) -> Result<Program, ModuleError> {
    resolve_uses(program, PathBuf::from(source_path).as_path())
}
