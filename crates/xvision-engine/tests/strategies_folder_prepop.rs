//! Integration tests for `xvision_engine::strategies_folder::prepop`.
//!
//! Coverage matches the contract acceptance list at
//! `team/contracts/strategies-folder-prepopulation.md`:
//! - Happy-path init populates all five subfolders + manifest.
//! - Idempotent re-runs (no drift findings, file bytes unchanged).
//! - Drift detection (modify a copy → preserved + drift finding).
//! - `--force` mode (drift suppressed, copy overwritten).
//! - Stale-source manifest entries trigger the right finding.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use xvision_engine::strategies_folder::folder_root;
use xvision_engine::strategies_folder::prepop::{
    self, InitOptions, Manifest, ManifestEntry, MANIFEST_FILENAME,
};

const EXPECTED_SUBFOLDERS: &[&str] = &["notes", "docs", "strategy-files", "evals", "library"];

fn fresh_home() -> TempDir {
    tempfile::tempdir().unwrap()
}

fn read_manifest_from_disk(xvn_home: &Path) -> Manifest {
    let path = prepop::manifest_path(xvn_home);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read manifest {}: {e}", path.display()));
    serde_json::from_str(&text).expect("parse manifest")
}

fn assert_all_subfolders_exist(xvn_home: &Path) {
    let root = folder_root(xvn_home);
    for name in EXPECTED_SUBFOLDERS {
        let sub = root.join(name);
        assert!(sub.is_dir(), "expected {} to be a directory", sub.display());
    }
}

#[tokio::test]
async fn happy_path_init_creates_all_subfolders_and_manifest() {
    let td = fresh_home();
    let report = prepop::init(td.path(), InitOptions::default()).await.unwrap();

    // All five subfolders materialized on first run.
    assert_all_subfolders_exist(td.path());
    let mut created = report.created_subfolders.clone();
    created.sort();
    let mut expected: Vec<String> = EXPECTED_SUBFOLDERS.iter().map(|s| (*s).to_string()).collect();
    expected.sort();
    assert_eq!(created, expected);

    // Manifest exactly matches the eligible docs/strategies source set.
    let manifest = read_manifest_from_disk(td.path());
    assert_eq!(manifest.version, prepop::MANIFEST_VERSION);
    assert!(!manifest.entries.is_empty(), "manifest must not be empty");

    let rels: BTreeSet<String> = manifest.entries.iter().map(|e| e.rel_path.clone()).collect();
    let expected_rels = expected_prepop_rel_paths();
    assert_eq!(
        rels, expected_rels,
        "manifest rel_path set must exactly match eligible docs/strategies sources"
    );

    // Every manifest entry points at an actual file on disk and the
    // recorded sha matches the on-disk bytes.
    for entry in &manifest.entries {
        let abs = folder_root(td.path()).join(&entry.rel_path);
        assert!(abs.is_file(), "manifest entry {} missing on disk", entry.rel_path);
        let bytes = std::fs::read(&abs).unwrap();
        let hash = sha256_hex(&bytes);
        assert_eq!(
            hash, entry.sha256,
            "sha mismatch for {} (on-disk {} vs manifest {})",
            entry.rel_path, hash, entry.sha256
        );
        assert!(
            entry.source.starts_with("docs/strategies/"),
            "source path malformed: {}",
            entry.source
        );
    }

    // First run reports the entries as new, with no drift or stale.
    assert!(
        !report.new_files.is_empty(),
        "new_files should include first-run copies"
    );
    assert!(
        report.refreshed_files.is_empty(),
        "no refreshed entries on first run"
    );
    assert!(report.drift.is_empty(), "no drift on first run");
    assert!(report.stale_source.is_empty(), "no stale source on first run");
}

#[tokio::test]
async fn idempotent_rerun_preserves_filesystem_state() {
    let td = fresh_home();
    let _ = prepop::init(td.path(), InitOptions::default()).await.unwrap();

    // Snapshot every file under the strategies root (path + bytes)
    // before the second run, so we can verify nothing changes.
    let before = snapshot_strategies_root(td.path());

    let report = prepop::init(td.path(), InitOptions::default()).await.unwrap();
    let after = snapshot_strategies_root(td.path());

    assert_eq!(before, after, "second init must not modify bytes on disk");
    assert!(
        report.drift.is_empty(),
        "expected no drift findings on clean re-run, got {:?}",
        report.drift
    );
    assert!(
        report.stale_source.is_empty(),
        "expected no stale source on clean re-run, got {:?}",
        report.stale_source
    );
    // Subfolders already existed, so no fresh creations.
    assert!(report.created_subfolders.is_empty());
    // No file should be marked new — second run sees the same manifest.
    assert!(
        report.new_files.is_empty(),
        "expected no new_files on second run, got {:?}",
        report.new_files
    );
    // And nothing should be force-refreshed (sources unchanged).
    assert!(
        report.refreshed_files.is_empty(),
        "expected no refreshed_files on second run, got {:?}",
        report.refreshed_files
    );

    // Manifest copied_at fields are stable across re-runs for
    // unchanged entries (contract acceptance #6).
    let before_manifest = read_manifest_from_disk(td.path());
    let _ = prepop::init(td.path(), InitOptions::default()).await.unwrap();
    let after_manifest = read_manifest_from_disk(td.path());
    assert_eq!(
        before_manifest, after_manifest,
        "manifest must be byte-identical across idempotent re-runs"
    );
}

#[tokio::test]
async fn drift_detection_preserves_user_edit_and_emits_finding() {
    let td = fresh_home();
    let _ = prepop::init(td.path(), InitOptions::default()).await.unwrap();

    // Pick the first entry to "edit". We deliberately use one of
    // the JSON templates so the edit is visible in the bytes.
    let manifest = read_manifest_from_disk(td.path());
    let target_entry = manifest
        .entries
        .iter()
        .find(|e| e.rel_path.starts_with("library/templates/"))
        .expect("at least one template")
        .clone();
    let target_abs = folder_root(td.path()).join(&target_entry.rel_path);
    let user_body = b"// user edit\n";
    std::fs::write(&target_abs, user_body).unwrap();

    // Re-run without --force. The edit must be preserved and a drift
    // finding emitted for that rel_path.
    let report = prepop::init(td.path(), InitOptions::default()).await.unwrap();
    let on_disk = std::fs::read(&target_abs).unwrap();
    assert_eq!(on_disk, user_body, "user edit must be preserved");
    assert!(
        report.drift.contains(&target_entry.rel_path),
        "drift report missing {} (got {:?})",
        target_entry.rel_path,
        report.drift
    );

    // Manifest sha256 stays at the *source* hash (not the user's
    // edited bytes) — drift is a read-only finding, not a manifest
    // update.
    let manifest_after = read_manifest_from_disk(td.path());
    let entry_after = manifest_after
        .entries
        .iter()
        .find(|e| e.rel_path == target_entry.rel_path)
        .unwrap();
    assert_eq!(entry_after.sha256, target_entry.sha256);
}

#[tokio::test]
async fn force_overwrites_drift_without_finding() {
    let td = fresh_home();
    let _ = prepop::init(td.path(), InitOptions::default()).await.unwrap();

    let manifest = read_manifest_from_disk(td.path());
    let target_entry = manifest
        .entries
        .iter()
        .find(|e| e.rel_path.starts_with("library/templates/"))
        .expect("at least one template")
        .clone();
    let target_abs = folder_root(td.path()).join(&target_entry.rel_path);
    std::fs::write(&target_abs, b"// user edit\n").unwrap();

    let report = prepop::init(td.path(), InitOptions { force: true })
        .await
        .unwrap();
    assert!(
        report.drift.is_empty(),
        "--force must suppress drift findings, got {:?}",
        report.drift
    );
    assert!(
        report.refreshed_files.contains(&target_entry.rel_path),
        "--force must surface the overwritten file as refreshed; got {:?}",
        report.refreshed_files
    );

    // File now matches the embedded source bytes (sha matches the
    // manifest's recorded sha).
    let on_disk = std::fs::read(&target_abs).unwrap();
    assert_eq!(sha256_hex(&on_disk), target_entry.sha256);
}

#[tokio::test]
async fn stale_manifest_entry_emits_finding_but_keeps_file() {
    let td = fresh_home();

    // Hand-roll a manifest containing a fictional entry that doesn't
    // exist in the embedded source. Init should leave the file in
    // place (a "snapshot" the operator chose to keep) and emit a
    // stale_source finding for its rel_path.
    let root = folder_root(td.path());
    let library = root.join("library");
    std::fs::create_dir_all(&library).unwrap();
    let fake_rel = "library/templates/_phantom/ghost.json".to_string();
    let fake_abs = root.join(&fake_rel);
    std::fs::create_dir_all(fake_abs.parent().unwrap()).unwrap();
    std::fs::write(&fake_abs, b"{}\n").unwrap();
    let fake_hash = sha256_hex(b"{}\n");
    let manifest = Manifest {
        version: prepop::MANIFEST_VERSION,
        entries: vec![ManifestEntry {
            rel_path: fake_rel.clone(),
            source: "docs/strategies/templates/_phantom/ghost.json".to_string(),
            sha256: fake_hash.clone(),
            copied_at: "2026-01-01T00:00:00Z".to_string(),
        }],
    };
    let body = serde_json::to_string_pretty(&manifest).unwrap();
    std::fs::write(library.join(MANIFEST_FILENAME), body).unwrap();

    let report = prepop::init(td.path(), InitOptions::default()).await.unwrap();
    assert!(
        report.stale_source.contains(&fake_rel),
        "stale_source missing {fake_rel}: got {:?}",
        report.stale_source
    );

    // File preserved on disk.
    assert!(fake_abs.is_file());

    // Manifest still carries the stale entry (operator may restore
    // the source later).
    let manifest_after = read_manifest_from_disk(td.path());
    assert!(
        manifest_after.entries.iter().any(|e| e.rel_path == fake_rel),
        "stale entry was scrubbed from manifest unexpectedly"
    );
}

#[tokio::test]
async fn new_source_appended_when_manifest_misses_entry() {
    // Simulate: a previous run wrote the manifest with only a subset
    // of entries. The current run must copy in the missing files +
    // append them to the manifest.
    let td = fresh_home();

    // Bootstrap an empty manifest so init treats every embedded source
    // as new.
    let root = folder_root(td.path());
    let library = root.join("library");
    std::fs::create_dir_all(&library).unwrap();
    let empty_manifest = Manifest::default();
    std::fs::write(
        library.join(MANIFEST_FILENAME),
        serde_json::to_string_pretty(&empty_manifest).unwrap(),
    )
    .unwrap();

    let report = prepop::init(td.path(), InitOptions::default()).await.unwrap();
    assert!(
        !report.new_files.is_empty(),
        "expected new_files populated when starting from empty manifest"
    );
    let manifest = read_manifest_from_disk(td.path());
    assert_eq!(
        manifest.entries.len(),
        report.new_files.len(),
        "manifest entries must equal new_files count when bootstrapping"
    );
}

// ---- helpers ----

fn snapshot_strategies_root(xvn_home: &Path) -> Vec<(String, Vec<u8>)> {
    let root = folder_root(xvn_home);
    let mut out: Vec<(String, Vec<u8>)> = Vec::new();
    let mut stack: Vec<PathBuf> = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.is_file() {
                let rel = path
                    .strip_prefix(&root)
                    .unwrap()
                    .components()
                    .filter_map(|c| match c {
                        std::path::Component::Normal(s) => s.to_str().map(|s| s.to_string()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("/");
                let bytes = std::fs::read(&path).unwrap();
                out.push((rel, bytes));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

fn expected_prepop_rel_paths() -> BTreeSet<String> {
    let docs_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/strategies");
    let mut out = BTreeSet::new();
    collect_expected_prepop_rel_paths(&docs_root, &docs_root, &mut out);
    out
}

fn collect_expected_prepop_rel_paths(root: &Path, dir: &Path, out: &mut BTreeSet<String>) {
    for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display())) {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            collect_expected_prepop_rel_paths(root, &path, out);
            continue;
        }
        if !path.is_file() {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .unwrap()
            .components()
            .filter_map(|c| match c {
                std::path::Component::Normal(s) => s.to_str().map(|s| s.to_string()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/");
        if rel == "freqtrade_strategies_playlist.md" || rel.starts_with("templates/") {
            out.insert(format!("library/{rel}"));
        }
    }
}
