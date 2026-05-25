//! Cutover guard (Stage 3, Task 9): assert the retired `BriefingCache`
//! leaves no *code* references behind.
//!
//! The in-memory `BriefingCache` (and its `CacheKey`) were deleted in favor
//! of trajectory-keyed briefing replay (`BriefingReplay`, keyed by
//! `TrajectoryKey.fingerprint()`). This guard walks every `.rs` file under
//! `crates/` and fails if `BriefingCache` appears as live code — i.e. on any
//! line that is NOT a comment. Doc/comment references to the historical name
//! (e.g. the trajectory-store module doc that explains the migration) are
//! permitted, mirroring the plan's grep guard intent while tolerating the
//! migration-history prose the cutover is documented in.

use std::fs;
use std::path::{Path, PathBuf};

fn crates_root() -> PathBuf {
    // crates/xvision-eval -> ../.. is the repo root; then crates/.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Skip build output / vendored trees.
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name == "target" || name == ".git" || name == "node_modules" {
                continue;
            }
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// True if `line` is a comment (`//` / `//!` / `///`) or starts with `*`
/// (inside a block-doc) once trimmed. Conservative: a code line with a
/// trailing comment that also references the name still counts as a hit,
/// which is the safe direction for a guard.
fn is_comment_line(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("//") || t.starts_with('*') || t.starts_with("/*")
}

#[test]
fn no_briefing_cache_code_references_remain() {
    let root = crates_root();
    let mut files = Vec::new();
    collect_rs_files(&root, &mut files);
    assert!(!files.is_empty(), "expected to find .rs files under {root:?}");

    // This guard file itself necessarily contains the literal string.
    let this_file = Path::new(file!())
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    let mut offenders: Vec<String> = Vec::new();
    for f in &files {
        if f.file_name().and_then(|n| n.to_str()) == Some(this_file) {
            continue;
        }
        let Ok(content) = fs::read_to_string(f) else {
            continue;
        };
        if !content.contains("BriefingCache") {
            continue;
        }
        for (n, line) in content.lines().enumerate() {
            if line.contains("BriefingCache") && !is_comment_line(line) {
                offenders.push(format!("{}:{}: {}", f.display(), n + 1, line.trim()));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "BriefingCache must have no live code references after the Stage 3 cutover; found:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn cache_module_file_is_deleted() {
    let cache_rs = crates_root().join("xvision-intern").join("src").join("cache.rs");
    assert!(
        !cache_rs.exists(),
        "crates/xvision-intern/src/cache.rs must be deleted (Stage 3 Task 9): {cache_rs:?}"
    );
}
