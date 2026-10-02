//! Repo hygiene: a merge once committed `<<<<<<<` conflict markers into 7
//! codegen files and nobody noticed until tests caught the fallout.
//! This test fails the suite if any marker ever lands in the tree again.

use std::fs;
use std::path::{Path, PathBuf};

const MARKERS: &[&str] = &["<<<<<<< ", ">>>>>>> "];
/// Dirs big, generated, or not ours.
const SKIP: &[&str] = &["target", ".git", ".github"];

fn visit(dir: &Path, bad: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if SKIP.contains(&name) {
                    continue;
                }
            }
            visit(&path, bad);
            continue;
        }
        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(_) => continue, // binary; skip
        };
        // This file names the markers in its MARKERS constant; never flag itself.
        if path.file_name().and_then(|n| n.to_str()) == Some("repo_hygiene.rs") {
            continue;
        }
        if MARKERS.iter().any(|m| text.contains(m)) {
            bad.push(path);
        }
    }
}

#[test]
fn no_conflict_markers_in_tree() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut bad = Vec::new();
    visit(&root, &mut bad);
    assert!(
        bad.is_empty(),
        "conflict markers committed in: {:?}\nResolve them before this can pass.",
        bad
    );
}
