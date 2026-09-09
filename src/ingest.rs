//! Reverse CuNi: ingest a tiny Python / Go / JS subset into `.cuni`, or refuse.
//!
//! v1 covers: `def name(args): return expr`, top-level `print(...)`, ints,
//! strings, `+ * -`. Anything else refuses. This is the start of "paste
//! Python, get the CuNi that would have produced it."

use std::path::Path;

pub fn ingest_file(path: &Path) -> Result<String, String> {
    let src = std::fs::read_to_string(path)
        .map_err(|e| format!("couldn't read {}: {}", path.display(), e))?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "py" => ingest_py(&src),
        "go" => Err("ingest: Go subset not implemented yet — refuse (write CuNi, emit Go)".into()),
        "js" | "mjs" | "cjs" => {
            Err("ingest: JS subset not implemented yet — refuse (write CuNi, emit JS)".into())
        }
        "cuni" => Ok(src),
        _ => Err(format!(
            "ingest: refuse unknown seat `.{}` — give .py (v1) or .cuni",
            ext
        )),
    }
}

fn ingest_py(src: &str) -> Result<String, String> {
    let mut out = String::from("# ingested from Python subset — exactness still required\n");
    let lines: Vec<&str> = src.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let raw = lines[i];
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            i += 1;
            continue;
        }
        if let Some(rest) = line.strip_prefix("def ") {
            let name = rest
                .split('(')
                .next()
                .ok_or("ingest: refuse malformed def")?
                .trim();
            if !is_ident(name) {
                return Err(format!("ingest: refuse def name `{name}`"));
            }
            let args = between(rest, '(', ')')
                .ok_or("ingest: refuse def args")?;
            let params: Vec<&str> = args
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(|s| s.split(':').next().unwrap().trim())
                .collect();
            for p in &params {
                if !is_ident(p) {
                    return Err(format!("ingest: refuse param `{p}`"));
                }
            }
            i += 1;
            let mut body = Vec::new();
            while i < lines.len() {
                let b = lines[i];
                if b.starts_with(' ') || b.starts_with('\t') {
                    body.push(b.trim());
                    i += 1;
                } else if b.trim().is_empty() {
                    i += 1;
                } else {
                    break;
                }
            }
            let ret = body
                .iter()
                .find_map(|l| l.strip_prefix("return "))
                .ok_or("ingest: refuse def without `return` (v1 subset)")?;
            let params_s = params
                .iter()
                .map(|p| format!("{p}: int"))
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!("def {name}({params_s}) -> int do\n"));
            out.push_str(&format!("    ret {}\n", py_expr_to_cuni(ret)?));
            out.push_str("end\n\n");
            continue;
        }
        if let Some(rest) = line.strip_prefix("print(") {
            let inner = rest
                .strip_suffix(')')
                .ok_or("ingest: refuse print(...)")?;
            out.push_str(&format!("say({})\n", py_expr_to_cuni(inner)?));
            i += 1;
            continue;
        }
        return Err(format!(
            "ingest: refuse Python line not in v1 subset: {line}"
        ));
    }
    if out.lines().count() < 2 {
        return Err("ingest: refuse empty Python subset".into());
    }
    Ok(out)
}

fn py_expr_to_cuni(e: &str) -> Result<String, String> {
    let e = e.trim();
    if (e.starts_with('"') && e.ends_with('"')) || (e.starts_with('\'') && e.ends_with('\'')) {
        return Ok(format!("\"{}\"", &e[1..e.len() - 1].replace('"', "\\\"")));
    }
    if e.parse::<i64>().is_ok() {
        return Ok(e.to_string());
    }
    if is_ident(e) {
        return Ok(e.to_string());
    }
    // very small expr: a + b, f(1, 2)
    if e.contains('(') && e.ends_with(')') {
        return Ok(e.to_string());
    }
    if e.contains('+') || e.contains('*') || e.contains('-') {
        return Ok(e.to_string());
    }
    Err(format!("ingest: refuse expression `{e}`"))
}

fn is_ident(s: &str) -> bool {
    let mut c = s.chars();
    match c.next() {
        Some(x) if x.is_ascii_alphabetic() || x == '_' => {
            c.all(|x| x.is_ascii_alphanumeric() || x == '_')
        }
        _ => false,
    }
}

fn between<'a>(s: &'a str, a: char, b: char) -> Option<&'a str> {
    let i = s.find(a)?;
    let j = s.rfind(b)?;
    if j > i {
        Some(&s[i + 1..j])
    } else {
        None
    }
}
