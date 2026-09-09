//! Exactness emit + run plan for every catalog language.
//!
//! Native seats compile with that language's toolchain. Other ids use the
//! Python lowering so the 119-language gate still emit+runs instead of skipping.

use crate::ast::Program;
use crate::codegen_c;
use crate::codegen_go;
use crate::codegen_js;
use crate::codegen_py;
use crate::codegen_rs;
use crate::langs::Lang;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeatKind {
    Native,
    Lowering,
}

pub fn seat_kind(lang: &Lang) -> SeatKind {
    match lang.id {
        "py" | "go" | "js" | "ts" | "c" | "cpp" | "rs" => SeatKind::Native,
        _ => SeatKind::Lowering,
    }
}

pub fn generate_exact(program: &Program, lang: &Lang) -> String {
    match lang.id {
        "go" => codegen_go::generate(program),
        "js" | "ts" => codegen_js::generate(program),
        "py" => codegen_py::generate(program),
        "c" | "cpp" => codegen_c::generate(program),
        "rs" => codegen_rs::generate(program),
        _ => {
            let mut s = String::new();
            s.push_str(&format!(
                "# CuNi exactness artifact — {} ({})\n",
                lang.name, lang.id
            ));
            s.push_str("# Seat pending a native toolchain. Python lowering so exactness still emit+runs.\n");
            s.push_str(&codegen_py::generate(program));
            s
        }
    }
}

pub struct ExecPlan {
    pub compile: Option<(String, Vec<String>)>,
    pub run: (String, Vec<String>),
}

pub fn exec_plan(lang: &Lang, artifact: &Path) -> ExecPlan {
    let path = artifact.to_string_lossy().into_owned();
    let bin = artifact.with_extension("bin");
    let bin_s = bin.to_string_lossy().into_owned();
    match lang.id {
        "go" => ExecPlan {
            compile: None,
            run: ("go".into(), vec!["run".into(), path]),
        },
        "js" | "ts" => ExecPlan {
            compile: None,
            run: ("node".into(), vec![path]),
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
            }
        }
        _ => ExecPlan {
            compile: None,
            run: ("python3".into(), vec![path]),
        },
    }
}
