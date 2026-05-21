//! Post-2026-05-21: the marketplace baseline `ma_crossover_template()`
//! was removed alongside the strategy `template_registry`. The
//! operator-readable starter content for this baseline migrates to a
//! prepop seed entry surfaced via `xvn strategies init`.
//!
//! The deterministic `Algorithm` implementation used by A/B compare
//! arms (`MaCrossover`) lives separately in
//! `crates/xvision-eval/src/baselines/ma_crossover.rs` and is
//! unaffected by this change.
//!
//! File retained as a historical breadcrumb (see
//! `team/contracts/strategy-template-registry-removal.md`).

#[tokio::test]
async fn ma_crossover_template_migrated_to_prepop_seed_surface() {
    use xvision_engine::strategies_folder::prepop::{self, InitOptions};

    let td = tempfile::tempdir().expect("tempdir");
    let report = prepop::init(td.path(), InitOptions::default())
        .await
        .expect("prepop init");

    let replacement_seed = "library/templates/EMA/ema_50_200_golden_cross.json";
    assert!(
        report.new_files.iter().any(|path| path == replacement_seed),
        "prepop init must surface the moving-average crossover starter in the folder library; got: {:?}",
        report.new_files
    );

    let seed = tokio::fs::read_to_string(td.path().join("strategies").join(replacement_seed))
        .await
        .expect("read copied crossover seed");
    assert!(seed.contains("EMA 50 200 Golden Cross"), "seed: {seed}");
    assert!(
        !report
            .new_files
            .iter()
            .any(|path| path.contains("ma_crossover_template")),
        "old binary template name must not be reintroduced as a folder seed: {:?}",
        report.new_files
    );
}
