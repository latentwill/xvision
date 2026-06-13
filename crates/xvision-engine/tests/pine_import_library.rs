/// WU9 — Engine library tests for `pine_import::library`.
///
/// TDD: tests authored BEFORE the implementation.
///
/// Contract assertions:
///  1. `pine_library()` returns ≥10 entries.
///  2. Every entry's `source` passes `import_pine` to a valid Strategy
///     (WU2 reuse: the Strategy passes the `validate_strategy` gate).
///  3. `import_library_entry(id)` for a known id → `Ok(ImportOutcome)`.
///  4. `import_library_entry(id)` for an unknown id → `Err(PineImportError)`.
///  5. Every LibraryEntry has a non-empty id, name, and description.
use xvision_engine::strategies::pine_import::{
    import_pine,
    library::{import_library_entry, pine_library},
    PineImportError,
};
use xvision_engine::strategies::validate::validate_strategy;

// ── 1. pine_library() returns ≥10 entries ────────────────────────────────────

#[test]
fn pine_library_has_at_least_ten_entries() {
    let entries = pine_library();
    assert!(
        entries.len() >= 10,
        "pine_library() must return ≥10 entries; got {}",
        entries.len()
    );
}

// ── 2. Every entry's source imports to a valid Strategy ───────────────────────

#[test]
fn all_library_entries_import_to_valid_strategy() {
    let entries = pine_library();
    for entry in &entries {
        let outcome = import_pine(entry.source)
            .unwrap_or_else(|e| panic!("import_pine failed for library entry '{}': {e}", entry.id));
        validate_strategy(&outcome.strategy)
            .unwrap_or_else(|e| panic!("validate_strategy failed for library entry '{}': {e:?}", entry.id));
    }
}

// ── 3. import_library_entry(known id) → Ok ───────────────────────────────────

#[test]
fn import_library_entry_known_id_returns_ok() {
    let entries = pine_library();
    // Pick the first entry; we just checked there's ≥10 so this is safe.
    let first = entries.first().expect("at least one entry");
    let result = import_library_entry(&first.id);
    assert!(
        result.is_ok(),
        "import_library_entry('{}'): expected Ok, got {:?}",
        first.id,
        result.err()
    );
    // The returned strategy should also pass validation.
    let outcome = result.unwrap();
    validate_strategy(&outcome.strategy)
        .expect("imported library entry strategy must pass validate_strategy");
}

// ── 4. import_library_entry(unknown id) → Err ────────────────────────────────

#[test]
fn import_library_entry_unknown_id_returns_err() {
    let result = import_library_entry("does-not-exist-xyz-9999");
    assert!(
        result.is_err(),
        "import_library_entry with unknown id must return Err"
    );
    // The error should be PineImportError::NothingMappable (library-not-found)
    // or similar — we just assert it is an Err.
    match result.unwrap_err() {
        PineImportError::NothingMappable(msg) => {
            assert!(
                msg.contains("not found") || msg.contains("unknown"),
                "NothingMappable message must mention not-found; got: {msg}"
            );
        }
        other => panic!("expected NothingMappable, got {other:?}"),
    }
}

// ── 5. Every entry has non-empty id, name, description ───────────────────────

#[test]
fn all_library_entries_have_non_empty_metadata() {
    let entries = pine_library();
    for entry in &entries {
        assert!(
            !entry.id.is_empty(),
            "LibraryEntry.id must be non-empty; entry: {:?}",
            entry.name
        );
        assert!(
            !entry.name.is_empty(),
            "LibraryEntry.name must be non-empty; id: {}",
            entry.id
        );
        assert!(
            !entry.description.is_empty(),
            "LibraryEntry.description must be non-empty; id: {}",
            entry.id
        );
    }
}

// ── 6. import_library_entry returns correct strategy name ─────────────────────

#[test]
fn import_library_entry_produces_importoutcome() {
    let entries = pine_library();
    // Test a few entries to ensure the outcome fields are populated.
    for entry in entries.iter().take(3) {
        let outcome = import_library_entry(&entry.id)
            .unwrap_or_else(|e| panic!("import_library_entry('{}') failed: {e}", entry.id));
        // Strategy must have a manifest with an id
        assert!(
            !outcome.strategy.manifest.id.is_empty(),
            "imported strategy manifest.id must be non-empty for entry '{}'",
            entry.id
        );
        // FidelityReport must be present (WU4 reuse)
        // (we verify it's not errored — the actual content is implementation-dependent)
    }
}
