//! Exactness emit + run plan for every catalog language.
//!
//! Native seats compile with that language's toolchain: 45 of the top 50.
//! Five ids use the Python lowering so the 53-entry gate still emit+runs
//! instead of skipping: m, vb (permanently blocked — proprietary / no Linux
//! toolchain), swift, hack, st (blocked — no installable Linux toolchain).

use crate::ast::Program;
use crate::codegen_c;
use crate::codegen_go;
use crate::codegen_java;
use crate::codegen_js;
use crate::codegen_lua;
use crate::codegen_php;
use crate::codegen_pl;
use crate::codegen_v;
use crate::codegen_d;
use crate::codegen_cr;
use crate::codegen_nim;
use crate::codegen_zig;
use crate::codegen_ada;
use crate::codegen_asm;
use crate::codegen_awk;
use crate::codegen_clj;
use crate::codegen_cob;
use crate::codegen_cs;
use crate::codegen_dart;
use crate::codegen_erl;
use crate::codegen_ex;
use crate::codegen_fs;
use crate::codegen_groovy;
use crate::codegen_hs;
use crate::codegen_hx;
use crate::codegen_jl;
use crate::codegen_kt;
use crate::codegen_m_objc;
use crate::codegen_pro;
use crate::codegen_ps1;
use crate::codegen_scala;
use crate::codegen_sh;
use crate::codegen_tcl;
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
        "py" | "go" | "js" | "ts" | "c" | "cpp" | "rs" | "rb" | "lua" | "sol" | "java" | "sql" | "php" | "r" | "ml" | "lisp" | "f90" | "pas" | "pl" | "zig" | "nim" | "cr" | "d" | "v" | "ada" | "asm" | "awk" | "clj" | "cob" | "cs" | "dart" | "erl" | "ex" | "fs" | "groovy" | "hs" | "hx" | "jl" | "kt" | "m-objc" | "pro" | "ps1" | "scala" | "sh" | "tcl" => {
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
        "ada" => codegen_ada::generate(program).map_err(|e| format!("Ada refused: {e}")),
        "asm" => codegen_asm::generate(program).map_err(|e| format!("Assembly refused: {e}")),
        "awk" => codegen_awk::generate(program).map_err(|e| format!("Awk refused: {e}")),
        "clj" => codegen_clj::generate(program).map_err(|e| format!("Clojure refused: {e}")),
        "cob" => codegen_cob::generate(program).map_err(|e| format!("COBOL refused: {e}")),
        "cs" => codegen_cs::generate(program).map_err(|e| format!("C# refused: {e}")),
        "dart" => codegen_dart::generate(program).map_err(|e| format!("Dart refused: {e}")),
        "erl" => codegen_erl::generate(program).map_err(|e| format!("Erlang refused: {e}")),
        "ex" => codegen_ex::generate(program).map_err(|e| format!("Elixir refused: {e}")),
        "fs" => codegen_fs::generate(program).map_err(|e| format!("F# refused: {e}")),
        "groovy" => codegen_groovy::generate(program).map_err(|e| format!("Groovy refused: {e}")),
        "hs" => codegen_hs::generate(program).map_err(|e| format!("Haskell refused: {e}")),
        "hx" => codegen_hx::generate(program).map_err(|e| format!("Haxe refused: {e}")),
        "jl" => codegen_jl::generate(program).map_err(|e| format!("Julia refused: {e}")),
        "kt" => codegen_kt::generate(program).map_err(|e| format!("Kotlin refused: {e}")),
        "m-objc" => codegen_m_objc::generate(program).map_err(|e| format!("Objective-C refused: {e}")),
        "pro" => codegen_pro::generate(program).map_err(|e| format!("Prolog refused: {e}")),
        "ps1" => codegen_ps1::generate(program).map_err(|e| format!("PowerShell refused: {e}")),
        "scala" => codegen_scala::generate(program).map_err(|e| format!("Scala refused: {e}")),
        "sh" => codegen_sh::generate(program).map_err(|e| format!("Bash refused: {e}")),
        "tcl" => codegen_tcl::generate(program).map_err(|e| format!("Tcl refused: {e}")),
        "zig" => codegen_zig::generate(program).map_err(|e| format!("Zig refused: {e}")), 
        "nim" => codegen_nim::generate(program).map_err(|e| format!("Nim refused: {e}")), 
        "cr" => codegen_cr::generate(program).map_err(|e| format!("Crystal refused: {e}")), 
        "d" => codegen_d::generate(program).map_err(|e| format!("D refused: {e}")), 
        "v" => codegen_v::generate(program).map_err(|e| format!("V refused: {e}")), 
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
                run: (format!("{}", out.display()), vec![]),
                compares_stdout: true,
            }
        },
        "pas" => {
            let out = artifact.with_extension("");
            ExecPlan {
                compile: Some(("fpc".into(), vec![format!("-o{}", out.display()), path.clone()])),
                run: (format!("{}", out.display()), vec![]),
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

        "awk" => ExecPlan::std(("gawk".into(), vec!["-f".into(), path])),
        "sh" => ExecPlan::std(("bash".into(), vec![path])),
        "clj" => ExecPlan::std(("clojure".into(), vec![path])),
        "ex" => ExecPlan::std(("elixir".into(), vec![path])),
        "erl" => ExecPlan::std(("escript".into(), vec![path])),
        "hs" => ExecPlan::std(("runghc".into(), vec![path])),
        "pro" => ExecPlan::std(("swipl".into(), vec!["-g".into(), "main".into(), "-t".into(), "halt".into(), path])),
        "tcl" => ExecPlan::std(("tclsh".into(), vec![path])),
        "groovy" => ExecPlan::std(("groovy".into(), vec![path])),
        "dart" => ExecPlan::std(("dart".into(), vec![path])),
        "jl" => ExecPlan::std(("julia".into(), vec![path])),
        "ps1" => ExecPlan::std(("pwsh".into(), vec!["-NoProfile".into(), "-File".into(), path])),
        "m-objc" => ExecPlan {
            compile: Some(("gcc".into(), vec!["-x".into(), "objective-c".into(), "-O0".into(), "-o".into(), bin_s.clone(), path])),
            run: (bin_s, vec![]),
            compares_stdout: true,
        },
        "ada" => {
            let dir = artifact.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| ".".into());
            let safe = format!("{dir}/cuni_ada_seat.adb");
            let out = artifact.with_extension("");
            let out_s = out.to_string_lossy().into_owned();
            ExecPlan {
                compile: Some(("sh".into(), vec!["-c".into(), format!("cp \"{path}\" \"{safe}\" && cd \"{dir}\" && gnatmake -o \"{out_s}\" cuni_ada_seat.adb")])),
                run: (out_s, vec![]),
                compares_stdout: true,
            }
        },
        "cob" => {
            let out = artifact.with_extension("");
            let out_s = out.to_string_lossy().into_owned();
            ExecPlan {
                compile: Some(("cobc".into(), vec!["-x".into(), "-free".into(), "-o".into(), out_s.clone(), path.clone()])),
                run: (out_s, vec![]),
                compares_stdout: true,
            }
        },
        "asm" => {
            let out = artifact.with_extension("");
            let out_s = out.to_string_lossy().into_owned();
            ExecPlan {
                compile: Some(("gcc".into(), vec!["-x".into(), "assembler".into(), "-o".into(), out_s.clone(), path.clone()])),
                run: (out_s, vec![]),
                compares_stdout: true,
            }
        },
        "hx" => {
            let dir = artifact.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| ".".into());
            let main_hx = format!("{dir}/Main.hx");
            ExecPlan {
                compile: Some(("cp".into(), vec![path.clone(), main_hx])),
                run: ("haxe".into(), vec!["--interp".into(), "-cp".into(), dir, "--main".into(), "Main".into()]),
                compares_stdout: true,
            }
        },
        "kt" => {
            let jar = artifact.with_extension("jar");
            let jar_s = jar.to_string_lossy().into_owned();
            ExecPlan {
                compile: Some(("kotlinc".into(), vec![path.clone(), "-include-runtime".into(), "-d".into(), jar_s.clone()])),
                run: ("java".into(), vec!["-jar".into(), jar_s]),
                compares_stdout: true,
            }
        },
        "scala" => {
            let dir = artifact.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| ".".into());
            ExecPlan {
                compile: Some(("scalac".into(), vec!["-d".into(), dir.clone(), path.clone()])),
                run: ("scala".into(), vec!["-cp".into(), dir, "Main".into()]),
                compares_stdout: true,
            }
        },
        "cs" => {
            let seat = concat!(env!("CARGO_MANIFEST_DIR"), "/target/cs-seat");
            let script = format!(
                r#"set -e
seat="{seat}"
mkdir -p "$seat"
if [ ! -f "$seat/seat.csproj" ]; then
cat > "$seat/seat.csproj" <<'CSPROJ'
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net8.0</TargetFramework>
    <ImplicitUsings>disable</ImplicitUsings>
    <Nullable>disable</Nullable>
    <AssemblyName>seat</AssemblyName>
  </PropertyGroup>
</Project>
CSPROJ
fi
cp "$1" "$seat/Program.cs"
dotnet build "$seat" -c Release --nologo -v q
"#
            );
            ExecPlan {
                compile: Some(("sh".into(), vec!["-c".into(), script, "cuni-cs".into(), path])),
                run: ("dotnet".into(), vec![format!("{seat}/bin/Release/net8.0/seat.dll")]),
                compares_stdout: true,
            }
        },
        "fs" => ExecPlan {
            compile: None,
            run: (
                "sh".into(),
                vec![
                    "-c".into(),
                    r#"src="$1"; fsx="${src%.fs}.fsx"; cp "$src" "$fsx"; exec dotnet fsi --nologo "$fsx""#.into(),
                    "cuni-fs".into(),
                    path,
                ],
            ),
            compares_stdout: true,
        },
        "py" => ExecPlan::std(("python3".into(), vec![path])),
        "r" => ExecPlan::std(("Rscript".into(), vec![path])),
        "zig" => {
            let out = artifact.with_extension("");
            let out_s = out.to_string_lossy().into_owned();
            ExecPlan {
                compile: Some(("zig".into(), vec!["build-exe".into(), path.clone(), "-O".into(), "ReleaseSafe".into(), format!("-femit-bin={}", out_s)])),
                run: (out_s, vec![]),
                compares_stdout: true,
            }
        },
        "nim" => {
            // Nim derives the module name from the filename: stage through a
            // sanitized copy so artifact stems can't break the build.
            let dir = artifact.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| ".".into());
            let safe = format!("{dir}/cuni_nim_seat.nim");
            let out = artifact.with_extension("");
            let out_s = out.to_string_lossy().into_owned();
            ExecPlan {
                compile: Some(("sh".into(), vec!["-c".into(), format!("cp \"{path}\" \"{safe}\" && nim c --hints:off -o:\"{out_s}\" \"{safe}\"")])),
                run: (out_s, vec![]),
                compares_stdout: true,
            }
        },
        "cr" => {
            let out = artifact.with_extension("");
            let out_s = out.to_string_lossy().into_owned();
            ExecPlan {
                compile: Some(("crystal".into(), vec!["build".into(), "-o".into(), out_s.clone(), path.clone()])),
                run: (out_s, vec![]),
                compares_stdout: true,
            }
        },
        "d" => {
            let out = artifact.with_extension("");
            let out_s = out.to_string_lossy().into_owned();
            ExecPlan {
                compile: Some(("dmd".into(), vec![format!("-of={}", out_s), path.clone()])),
                run: (out_s, vec![]),
                compares_stdout: true,
            }
        },
        "v" => {
            let out = artifact.with_extension("");
            let out_s = out.to_string_lossy().into_owned();
            ExecPlan {
                compile: Some(("v".into(), vec!["-o".into(), out_s.clone(), path.clone()])),
                run: (out_s, vec![]),
                compares_stdout: true,
            }
        },
        _ => ExecPlan {
            compile: None,
            run: ("python3".into(), vec![path]),
            compares_stdout: true,
        },
    }
}
