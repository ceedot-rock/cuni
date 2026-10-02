//! Exactness emit + run plan for every catalog language.
//!
//! Native seats compile with that language's toolchain. Other ids use the
//! Python lowering so the 113-language gate still emit+runs instead of skipping.

use crate::ast::Program;
use crate::codegen_c;
use crate::codegen_go;
use crate::codegen_java;
use crate::codegen_js;
use crate::codegen_lua;
use crate::codegen_php;
use crate::codegen_pl;
use crate::codegen_pas;
use crate::codegen_f90;
use crate::codegen_lisp;
use crate::codegen_ml;
use crate::codegen_r;
use crate::codegen_py;
use crate::codegen_rb;
use crate::codegen_rs;
use crate::codegen_sol;
use crate::codegen_sql;
use crate::langs::Lang;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeatKind {
    Native,
    Lowering,
}

pub fn seat_kind(lang: &Lang) -> SeatKind {
    match lang.id {
        "py" | "go" | "js" | "ts" | "c" | "cpp" | "rs" | "rb" | "lua" | "sol" | "java" | "sql" | "php" | "r" | "ml" | "lisp" | "f90" | "pas" | "pl" => {
            SeatKind::Native
        }
        _ => SeatKind::Lowering,
    }
}

pub fn generate_exact(program: &Program, lang: &Lang) -> Result<String, String> {
    match lang.id {
        "go" => codegen_go::generate(program).map_err(|e| format!("Go refused: {e}")),
        "js" | "ts" => Ok(codegen_js::generate(program)),
        "py" => Ok(codegen_py::generate(program)),
        "rb" => Ok(codegen_rb::generate(program)),
        "lua" => codegen_lua::generate(program).map_err(|e| format!("Lua refused: {e}")),
        "c" | "cpp" => Ok(codegen_c::generate(program)),
        "rs" => Ok(codegen_rs::generate(program)),
        // Solidity is the only seat that can honestly refuse: an unsupported
        // construct must fail the emit, never produce a comment-only file
        // that solc would accept with exit 0 (a false pass).
        "sol" => codegen_sol::generate(program).map_err(|e| format!("Solidity refused: {e}")),
        // Java and SQL are real seats with real refusal semantics: anything
        // without an exact mapping is an Err, never a guess.
        "java" => codegen_java::generate(program).map_err(|e| format!("Java refused: {e}")),
        "sql" => codegen_sql::generate(program).map_err(|e| format!("SQL refused: {e}")),
        "php" => codegen_php::generate(program).map_err(|e| format!("PHP refused: {e}")),
        "pl" => codegen_pl::generate(program).map_err(|e| format!("Perl refused: {e}")),
        "pas" => codegen_pas::generate(program).map_err(|e| format!("Pascal refused: {e}")),
        "f90" => codegen_f90::generate(program).map_err(|e| format!("Fortran refused: {e}")),
        "lisp" => codegen_lisp::generate(program).map_err(|e| format!("Lisp refused: {e}")),
        "ml" => codegen_ml::generate(program).map_err(|e| format!("OCaml refused: {e}")),
        "r" => codegen_r::generate(program).map_err(|e| format!("R refused: {e}")),
        _ => {
            let mut s = String::new();
            s.push_str(&format!(
                "# CuNi exactness artifact — {} ({})\n",
                lang.name, lang.id
            ));
            s.push_str("# Seat pending a native toolchain. Python lowering so exactness still emit+runs.\n");
            s.push_str(&codegen_py::generate(program));
            Ok(s)
        }
    }
}

pub struct ExecPlan {
    pub compile: Option<(String, Vec<String>)>,
    pub run: (String, Vec<String>),
    /// If false, this seat is emit+compile verified only (no stdout to
    /// compare, e.g. a contract with no EVM on the check machine).
    pub compares_stdout: bool,
}

impl ExecPlan {
    fn std(run: (String, Vec<String>)) -> Self {
        ExecPlan {
            compile: None,
            run,
            compares_stdout: true,
        }
    }
}

pub fn exec_plan(lang: &Lang, artifact: &Path) -> ExecPlan {
    let path = artifact.to_string_lossy().into_owned();
    let bin = artifact.with_extension("bin");
    let bin_s = bin.to_string_lossy().into_owned();
    match lang.id {
        "go" => ExecPlan {
            compile: None,
            run: ("go".into(), vec!["run".into(), path]),
            compares_stdout: true,
        },
        "js" | "ts" => ExecPlan {
            compile: None,
            run: ("node".into(), vec![path]),
            compares_stdout: true,
        },
        "c" => ExecPlan {
            compile: Some((
                "gcc".into(),
                vec![
                    "-x".into(),
                    "c".into(),
                    "-O0".into(),
                    "-std=gnu11".into(),
                    "-o".into(),
                    bin_s.clone(),
                    path,
                ],
            )),
            run: (bin_s, vec![]),
            compares_stdout: true,
        },
        "cpp" => ExecPlan {
            compile: Some((
                "g++".into(),
                vec![
                    "-x".into(),
                    "c++".into(),
                    "-O0".into(),
                    "-std=gnu++17".into(),
                    "-o".into(),
                    bin_s.clone(),
                    path,
                ],
            )),
            run: (bin_s, vec![]),
            compares_stdout: true,
        },
        "rs" => {
            let rs = if artifact.extension().and_then(|e| e.to_str()) == Some("rs") {
                PathBuf::from(artifact)
            } else {
                artifact.with_extension("rs")
            };
            ExecPlan {
                compile: Some((
                    "rustc".into(),
                    vec![
                        "-O".into(),
                        "-o".into(),
                        bin_s.clone(),
                        rs.to_string_lossy().into_owned(),
                    ],
                )),
                run: (bin_s, vec![]),
                compares_stdout: true,
            }
        }
        "rb" => ExecPlan {
            compile: None,
            run: ("ruby".into(), vec![path]),
            compares_stdout: true,
        },
        "lua" => ExecPlan {
            compile: None,
            run: ("lua5.4".into(), vec![path]),
            compares_stdout: true,
        },
        "java" => {
            // The backend emits a package-private `class Main`, so javac
            // accepts the harness's `<stem>_java.java` filename; `-d`
            // keeps Main.class inside the seat's own work dir, and
            // `java -cp <dir> Main` runs it there (never the repo cwd).
            let dir = artifact
                .parent()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| ".".into());
            ExecPlan {
                compile: Some(("javac".into(), vec!["-d".into(), dir.clone(), path])),
                run: ("java".into(), vec!["-cp".into(), dir, "Main".into()]),
                compares_stdout: true,
            }
        }
        "sql" => ExecPlan {
            // One SELECT per `say`, run in order against an empty database.
            // `.read` is a sqlite3 dot-command, accepted as the SQL argument.
            compile: None,
            run: (
                "sqlite3".into(),
                vec![
                    "-batch".into(),
                    "-noheader".into(),
                    ":memory:".into(),
                    format!(".read {path}"),
                ],
            ),
            compares_stdout: true,
        },
        "php" => ExecPlan::std(("php".into(), vec![path])),
        "pl" => ExecPlan::std(("perl".into(), vec![path])),
        "f90" => {
            let out = artifact.with_extension("");
            ExecPlan {
                compile: Some(("gfortran".into(), vec!["-o".into(), format!("{}", out.display()), path.clone()])),
                run: ("./".to_string() + &out.display().to_string(), vec![]),
                compares_stdout: true,
            }
        },
        "pas" => {
            let out = artifact.with_extension("");
            ExecPlan {
                compile: Some(("fpc".into(), vec![format!("-o{}", out.display()), path.clone()])),
                run: ("./".to_string() + &out.display().to_string(), vec![]),
                compares_stdout: true,
            }
        },
        "lisp" => ExecPlan::std(("sbcl".into(), vec!["--script".into(), path])),
        "ml" => ExecPlan::std(("ocaml".into(), vec![path])),
        "sol" => {
            // A contract has no stdout: solc compiling it IS the verification
            // (deployable). Excluded from the stdout comparison; the dice
            // values are cross-checked against the interpreter instead.
            let out_dir = artifact.with_extension("solc_out");
            ExecPlan {
                compile: Some((
                    "solc".into(),
                    vec![
                        "--bin".into(),
                        "--optimize".into(),
                        "-o".into(),
                        out_dir.to_string_lossy().into_owned(),
                        "--overwrite".into(),
                        path.clone(),
                    ],
                )),
                run: ("solc".into(), vec!["--ast-compact-json".into(), path]),
                compares_stdout: false,
            }
        }
        _ => ExecPlan {
            compile: None,
            run: ("python3".into(), vec![path]),
            compares_stdout: true,
        },
    }
}
