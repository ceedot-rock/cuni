//! # Universal / portable IR — **NOT DONE / unfinished**
//!
//! **Status:** thin M2 sketch only. Pipeline today remains
//! `src/ast.rs` → `src/emit.rs` (+ codegen_*) with catalog ids in `src/langs.rs`.
//! This module does **not** replace that path and is **not** production-ready.
//!
//! Honesty (say out loud): **119** catalog seats; **~7** native
//! (`py`, `go`, `js`, `ts`, `c`, `cpp`, `rs`); Studio/Publish gate **`py,go,js` only**;
//! **majority lowering**. Exactness refuse is sacred — **no approximate mode**.
//!
//! See `docs/milestones/M2_IR_SKETCH.md` and `docs/UNIVERSAL_AST_V0.md`.
//!
//! ## Relation to existing Translate spine
//!
//! | Artifact | Role vs this stub |
//! |----------|-------------------|
//! | `src/ast.rs` | Front-end AST — intended *source* of a future lower into IR |
//! | `src/emit.rs` | `SeatKind` (`Native` / `Lowering`) + `generate_exact` — seat spelling stays here |
//! | `src/langs.rs` | 119 catalog ids — IR must not invent seats |
//!
//! Anything beyond scaffolding (emit-from-IR, native graduation) is **M3+**.

#![allow(dead_code)]

use crate::ast::Program;

/// Stable string for tests / docs: IR remains unfinished.
pub const UNFINISHED_MARKER: &str = "IR is NOT DONE / unfinished";

/// Catalog law: seat count in `src/langs.rs` (measure, don't soft-claim 120).
pub const CATALOG_SEAT_COUNT: usize = 119;

/// Native maturity ids today (~7). Mirrors `emit::seat_kind` natives; kept here so
/// this stub compiles in both the lib and bin crates without pulling emit/codegen.
pub const NATIVE_SEAT_IDS: &[&str] = &["py", "go", "js", "ts", "c", "cpp", "rs"];

/// Studio / Publish hosted gate (Fly image) — not a claim that other natives are absent.
pub const STUDIO_GATE_IDS: &[&str] = &["py", "go", "js"];

/// Exactness seat mark — mirrors `emit::SeatKind` honesty (native vs lowering).
/// Lowering ≠ skip; lowering ≠ first-class native. No approximate mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeatMark {
    Native,
    Lowering,
}

/// Per-seat plan attachment (sketch). Real wiring to `langs::LANGS` / `emit::seat_kind`
/// is **not** implemented here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeatPlanEntry {
    pub lang_id: &'static str,
    pub mark: SeatMark,
}

/// Placeholder portable IR module — items TBD; deliberately empty.
///
/// Suggested future shape (from M2 card): defs / link / typ / iface / enum;
/// no raw pointers, macros, or async-in-core.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct IrModule {
    /// Reserved: future portable items lowered from `ast::Program`.
    pub items: Vec<IrItemStub>,
    /// Reserved: seat plan sidecar (native | lowering per catalog id).
    pub seat_plan: Vec<SeatPlanEntry>,
}

/// Stub item kinds only — not a real IR instruction set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrItemStub {
    /// Stand-in for a portable `def` / `link` / closed type form.
    Unimplemented(&'static str),
}

/// Why IR work stops — unfinished sketch or future hard-fail refuse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrRefuse {
    /// M2 honesty: lowering AST → IR is not implemented.
    Unfinished(&'static str),
    /// Future: oddity / exactness hard-fail (no approximate rewrite).
    HardFail { code: &'static str, detail: String },
}

impl std::fmt::Display for IrRefuse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IrRefuse::Unfinished(msg) => write!(f, "{UNFINISHED_MARKER}: {msg}"),
            IrRefuse::HardFail { code, detail } => {
                write!(f, "IR hard-fail [{code}]: {detail} (no approximate mode)")
            }
        }
    }
}

/// Sketch: classify a catalog id the way `emit::seat_kind` does — **local mirror only**.
/// Does not consult `langs::LANGS`; call sites must still treat `src/langs.rs` as law.
pub fn seat_mark_for_id(lang_id: &str) -> SeatMark {
    if NATIVE_SEAT_IDS.contains(&lang_id) {
        SeatMark::Native
    } else {
        SeatMark::Lowering
    }
}

/// Build a **sketch** seat plan for the honesty tier (not wired into check/emit).
pub fn sketch_seat_plan() -> Vec<SeatPlanEntry> {
    let mut plan = Vec::with_capacity(NATIVE_SEAT_IDS.len() + 1);
    for id in NATIVE_SEAT_IDS {
        plan.push(SeatPlanEntry {
            lang_id: id,
            mark: SeatMark::Native,
        });
    }
    // Majority of the 119 are lowering; one sentinel reminds callers not to soft-claim.
    plan.push(SeatPlanEntry {
        lang_id: "(catalog-rest-lowering)",
        mark: SeatMark::Lowering,
    });
    plan
}

/// Intended AST → IR entry point — **always refuses unfinished**.
///
/// Real lowering is M3+; do not call this from emit/check paths yet.
pub fn lower_from_ast(_program: &Program) -> Result<IrModule, IrRefuse> {
    Err(IrRefuse::Unfinished(
        "lower_from_ast is a stub; pipeline remains ast → emit",
    ))
}

/// Empty scaffolding module for tests / future fill-in.
pub fn empty_module() -> IrModule {
    IrModule {
        items: Vec::new(),
        seat_plan: sketch_seat_plan(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unfinished_marker_is_honest() {
        assert!(
            UNFINISHED_MARKER.contains("NOT DONE"),
            "marker must stay explicit: {UNFINISHED_MARKER}"
        );
        assert!(
            UNFINISHED_MARKER.contains("unfinished"),
            "marker must stay explicit: {UNFINISHED_MARKER}"
        );
    }

    #[test]
    fn lower_from_ast_refuses_as_unfinished() {
        let program = Program { items: vec![] };
        let err = lower_from_ast(&program).expect_err("IR lower must not pretend to work");
        let msg = err.to_string();
        assert!(
            msg.contains(UNFINISHED_MARKER),
            "refuse must carry unfinished marker, got: {msg}"
        );
    }

    #[test]
    fn honesty_tiers_match_catalog_law() {
        assert_eq!(CATALOG_SEAT_COUNT, 119);
        assert_eq!(NATIVE_SEAT_IDS.len(), 7);
        assert_eq!(STUDIO_GATE_IDS, &["py", "go", "js"]);
        assert_eq!(seat_mark_for_id("py"), SeatMark::Native);
        assert_eq!(seat_mark_for_id("rs"), SeatMark::Native);
        assert_eq!(seat_mark_for_id("java"), SeatMark::Lowering);
        let plan = sketch_seat_plan();
        assert!(plan.iter().any(|e| e.mark == SeatMark::Lowering));
        let empty = empty_module();
        assert!(empty.items.is_empty());
    }
}
