//! Catalog of coding languages. Exactness emit+runs every id in LANGS.
//! 45 ids use real emitters (native seats, see seat_kind in emit.rs);
//! 5 ids (swift, m, vb, st, hack) are Python lowerings executed by python3,
//! and 3 onchain profiles (vy, move, cairo) emit chain-shaped artifacts,
//! so stdout can actually be compared.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lang {
    pub id: &'static str,
    pub name: &'static str,
    pub ext: &'static str,
    pub family: Family,
}

impl Lang {
    /// Unique on-disk name under `--emit-all` (`{id}.{ext}`).
    pub fn out_file(&self) -> String {
        format!("{}.{}", self.id, self.ext)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Python,
    Ruby,
    Lua,
    Perl,
    Php,
    R,
    Julia,
    Elixir,
    Bash,
    Powershell,
    C,
    Cpp,
    Java,
    CSharp,
    Kotlin,
    Scala,
    Swift,
    Dart,
    Go,
    Rust,
    Zig,
    Nim,
    Js,
    Ts,
    Haskell,
    Lisp,
    Clojure,
    Ocaml,
    Fsharp,
    Erlang,
    Fortran,
    Pascal,
    Cobol,
    Prolog,
    Matlab,
    Sql,
    Assembly,
    Solidity,
    Groovy,
    Objc,
    Vb,
    Crystal,
    Ada,
    D,
    Awk,
    Tcl,
    Smalltalk,
    Haxe,
    Vlang,
    Hack,
}

pub const LANGS: &[Lang] = &[
    // Quality backends (native runtimes).
    Lang {
        id: "py",
        name: "Python",
        ext: "py",
        family: Family::Python,
    },
    Lang {
        id: "go",
        name: "Go",
        ext: "go",
        family: Family::Go,
    },
    Lang {
        id: "js",
        name: "JavaScript",
        ext: "js",
        family: Family::Js,
    },
    // Rest of the catalog: exactness still emit+runs each id (Python lowering).
    Lang {
        id: "ts",
        name: "TypeScript",
        ext: "ts",
        family: Family::Ts,
    },
    Lang {
        id: "c",
        name: "C",
        ext: "c",
        family: Family::C,
    },
    Lang {
        id: "cpp",
        name: "C++",
        ext: "cpp",
        family: Family::Cpp,
    },
    Lang {
        id: "cs",
        name: "C#",
        ext: "cs",
        family: Family::CSharp,
    },
    Lang {
        id: "java",
        name: "Java",
        ext: "java",
        family: Family::Java,
    },
    Lang {
        id: "kt",
        name: "Kotlin",
        ext: "kt",
        family: Family::Kotlin,
    },
    Lang {
        id: "scala",
        name: "Scala",
        ext: "scala",
        family: Family::Scala,
    },
    Lang {
        id: "swift",
        name: "Swift",
        ext: "swift",
        family: Family::Swift,
    },
    Lang {
        id: "rs",
        name: "Rust",
        ext: "rs",
        family: Family::Rust,
    },
    Lang {
        id: "rb",
        name: "Ruby",
        ext: "rb",
        family: Family::Ruby,
    },
    Lang {
        id: "php",
        name: "PHP",
        ext: "php",
        family: Family::Php,
    },
    Lang {
        id: "lua",
        name: "Lua",
        ext: "lua",
        family: Family::Lua,
    },
    Lang {
        id: "pl",
        name: "Perl",
        ext: "pl",
        family: Family::Perl,
    },
    Lang {
        id: "r",
        name: "R",
        ext: "R",
        family: Family::R,
    },
    Lang {
        id: "jl",
        name: "Julia",
        ext: "jl",
        family: Family::Julia,
    },
    Lang {
        id: "ex",
        name: "Elixir",
        ext: "ex",
        family: Family::Elixir,
    },
    Lang {
        id: "erl",
        name: "Erlang",
        ext: "erl",
        family: Family::Erlang,
    },
    Lang {
        id: "hs",
        name: "Haskell",
        ext: "hs",
        family: Family::Haskell,
    },
    Lang {
        id: "ml",
        name: "OCaml",
        ext: "ml",
        family: Family::Ocaml,
    },
    Lang {
        id: "fs",
        name: "F#",
        ext: "fs",
        family: Family::Fsharp,
    },
    Lang {
        id: "lisp",
        name: "Common Lisp",
        ext: "lisp",
        family: Family::Lisp,
    },
    Lang {
        id: "clj",
        name: "Clojure",
        ext: "clj",
        family: Family::Clojure,
    },
    Lang {
        id: "dart",
        name: "Dart",
        ext: "dart",
        family: Family::Dart,
    },
    Lang {
        id: "zig",
        name: "Zig",
        ext: "zig",
        family: Family::Zig,
    },
    Lang {
        id: "nim",
        name: "Nim",
        ext: "nim",
        family: Family::Nim,
    },
    Lang {
        id: "cr",
        name: "Crystal",
        ext: "cr",
        family: Family::Crystal,
    },
    Lang {
        id: "d",
        name: "D",
        ext: "d",
        family: Family::D,
    },
    Lang {
        id: "v",
        name: "V",
        ext: "v",
        family: Family::Vlang,
    },
    Lang {
        id: "ada",
        name: "Ada",
        ext: "adb",
        family: Family::Ada,
    },
    Lang {
        id: "pas",
        name: "Pascal",
        ext: "pas",
        family: Family::Pascal,
    },
    Lang {
        id: "f90",
        name: "Fortran",
        ext: "f90",
        family: Family::Fortran,
    },
    Lang {
        id: "cob",
        name: "COBOL",
        ext: "cob",
        family: Family::Cobol,
    },
    Lang {
        id: "m",
        name: "MATLAB",
        ext: "m",
        family: Family::Matlab,
    },
    Lang {
        id: "pro",
        name: "Prolog",
        ext: "pro",
        family: Family::Prolog,
    },
    Lang {
        id: "sql",
        name: "SQL",
        ext: "sql",
        family: Family::Sql,
    },
    Lang {
        id: "asm",
        name: "Assembly",
        ext: "s",
        family: Family::Assembly,
    },
    Lang {
        id: "sol",
        name: "Solidity",
        ext: "sol",
        family: Family::Solidity,
    },
    Lang {
        id: "groovy",
        name: "Groovy",
        ext: "groovy",
        family: Family::Groovy,
    },
    Lang {
        id: "m-objc",
        name: "Objective-C",
        ext: "m",
        family: Family::Objc,
    },
    Lang {
        id: "vb",
        name: "Visual Basic",
        ext: "vb",
        family: Family::Vb,
    },
    Lang {
        id: "sh",
        name: "Bash",
        ext: "sh",
        family: Family::Bash,
    },
    Lang {
        id: "ps1",
        name: "PowerShell",
        ext: "ps1",
        family: Family::Powershell,
    },
    Lang {
        id: "awk",
        name: "Awk",
        ext: "awk",
        family: Family::Awk,
    },
    Lang {
        id: "tcl",
        name: "Tcl",
        ext: "tcl",
        family: Family::Tcl,
    },
    Lang {
        id: "st",
        name: "Smalltalk",
        ext: "st",
        family: Family::Smalltalk,
    },
    Lang {
        id: "hx",
        name: "Haxe",
        ext: "hx",
        family: Family::Haxe,
    },
    Lang {
        id: "hack",
        name: "Hack",
        ext: "hack",
        family: Family::Hack,
    },
    // More coding languages, mapped onto existing families.
    Lang {
        id: "vy",
        name: "Vyper",
        ext: "vy",
        family: Family::Python,
    },
    Lang {
        id: "move",
        name: "Move",
        ext: "move",
        family: Family::Solidity,
    },
    Lang {
        id: "cairo",
        name: "Cairo",
        ext: "cairo",
        family: Family::Solidity,
    },
    // Extended catalog: more real languages, mapped onto existing families.
];

#[allow(dead_code)]
pub fn find(id: &str) -> Option<&'static Lang> {
    LANGS
        .iter()
        .find(|l| l.id == id || l.ext == id || l.name.eq_ignore_ascii_case(id))
}

#[allow(dead_code)]
pub fn ids() -> Vec<&'static str> {
    LANGS.iter().map(|l| l.id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn catalog_ids_and_out_files_are_unique() {
        let mut ids = HashSet::new();
        let mut files = HashSet::new();
        for l in LANGS {
            assert!(ids.insert(l.id), "duplicate lang id {}", l.id);
            assert!(
                files.insert(l.out_file()),
                "duplicate out file {}",
                l.out_file()
            );
            assert!(!l.id.is_empty());
            assert!(!l.name.is_empty());
            assert!(!l.ext.is_empty());
        }
        assert!(
            LANGS.len() >= 50,
            "catalog should cover a full coding-language set, got {}",
            LANGS.len()
        );
    }

    #[test]
    fn exactness_trio_is_first() {
        assert_eq!(LANGS[0].id, "py");
        assert_eq!(LANGS[1].id, "go");
        assert_eq!(LANGS[2].id, "js");
    }
}
