use crate::ast::{FnDecl, Item, Program, Type};

/// Reserved global identifiers per target, narrow and hand-picked — not an
/// exhaustive list of every builtin/global a target exposes (Python's alone
/// would be enormous and mostly irrelevant to `ext` bodies in practice).
/// Scoped specifically to names an `ext` body plausibly *calls*, where a CuNi
/// `ext` declaration sharing that exact name would shadow the global with a
/// same-named top-level function/class, turning an intended call to the
/// global into silent self-recursion. See SPEC.md §18 and
/// OPEN_ITEMS_PROPOSAL.md item 5.
///
/// Go has no entry here: Go has no ambient/global identifiers of this kind
/// (imports are explicit, so there's nothing an `ext` name could silently
/// shadow) — this check is a no-op for the Go backend.
const JS_RESERVED: &[&str] = &["fetch", "console", "Math", "JSON", "Promise", "require", "module", "process", "globalThis"];
const PY_RESERVED: &[&str] = &["print", "len", "open", "input", "list", "dict", "set", "str", "int", "float", "map", "filter", "range", "sorted", "format", "exec", "eval"];

/// Checks every `ext` declaration in `program` against the reserved-name list
/// for `target` ("py", "go", or "js"). Returns the first colliding `ext` name
/// found, if any — deliberately over-conservative (refuses even if the
/// colliding `ext` body doesn't actually reference the global), since a false
/// refusal is far cheaper than the silent runtime bug it prevents (see
/// OPEN_ITEMS_PROPOSAL.md item 5, option (b)).
pub fn find_ext_collision<'a>(program: &'a Program, target: &str) -> Option<&'a str> {
    let reserved: &[&str] = match target {
        "js" => JS_RESERVED,
        "py" => PY_RESERVED,
        _ => return None,
    };
    for item in &program.items {
        if let Item::Ext(ext) = item {
            if reserved.contains(&ext.name.as_str()) {
                return Some(&ext.name);
            }
        }
    }
    None
}

fn is_wire_scalar(ty: &Type) -> bool {
    matches!(ty, Type::Named(name) if matches!(name.as_str(), "int" | "float" | "str" | "bool"))
}

/// `link` (SPEC.md §19) generates a JSON wire codec per param/return type.
/// v1 scope is deliberately narrow: only scalar types (`int`, `float`,
/// `str`, `bool`) have a codec generator here — `list<T>`/`map<K,V>`/`typ`
/// fields would need a recursive, per-shape (de)serializer this toy
/// compiler doesn't build yet. Rather than silently emit a wrong or
/// half-working codec for an unsupported shape, the compiler refuses,
/// consistent with §2's compile-or-refuse posture. Returns the offending
/// link's name and the bad type's source text, if any.
pub fn find_bad_link_type(program: &Program) -> Option<(&str, String)> {
    for item in &program.items {
        if let Item::Def(FnDecl { is_link: true, name, params, ret_type, .. }) = item {
            for p in params {
                if !is_wire_scalar(&p.ty) {
                    return Some((name, format!("param `{}` has type {:?}", p.name, p.ty)));
                }
            }
            if !is_wire_scalar(ret_type) {
                return Some((name, format!("return type {:?}", ret_type)));
            }
        }
    }
    None
}
