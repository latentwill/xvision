//! Resolution precedence for `xvn obs retention show`:
//! **CLI flag > env var > config file > default**.
//!
//! Also locks the startup WARN line wording (`full_debug retention
//! enabled`) — Phase B emission code greps for it.

use std::fs;
use std::sync::Mutex;
use tempfile::TempDir;
use tracing::subscriber::with_default;
use tracing_subscriber::fmt::MakeWriter;
use xvision_observability::{
    resolve_retention, CliOverrides, RetentionMode, Source, ENV_OVERRIDE_PREFIX,
};

const MODE_KEY: &str = "XVISION_OBSERVABILITY_RETENTION";
const TTL_KEY: &str = "XVISION_OBSERVABILITY_RETENTION_PAYLOAD_TTL_DAYS";

// All four precedence tests touch the same env vars. Run them
// strictly sequentially so they don't observe each other's writes.
fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|p| p.into_inner())
}

fn clean_env() {
    // SAFETY: env mutation; serialized by `env_lock()`.
    unsafe {
        std::env::remove_var(MODE_KEY);
        std::env::remove_var(TTL_KEY);
        std::env::remove_var("XVISION_OBSERVABILITY_RETENTION_MODE");
    }
}

#[test]
fn default_wins_when_nothing_else_set() {
    let _guard = env_lock();
    clean_env();
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("observability.toml");
    let view = resolve_retention(&path, &CliOverrides::default()).unwrap();
    assert_eq!(view.mode.value, RetentionMode::HashOnly);
    assert_eq!(view.mode.source, Source::Default);
    assert_eq!(view.payload_ttl_days.value, 7);
    assert_eq!(view.payload_ttl_days.source, Source::Default);
    assert!(!view.config_file_present);
}

#[test]
fn config_file_overrides_default() {
    let _guard = env_lock();
    clean_env();
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("observability.toml");
    fs::write(
        &path,
        r#"
[observability.retention]
mode = "redacted"
payload_ttl_days = 30
"#,
    )
    .unwrap();
    let view = resolve_retention(&path, &CliOverrides::default()).unwrap();
    assert_eq!(view.mode.value, RetentionMode::Redacted);
    assert_eq!(view.mode.source, Source::ConfigFile);
    assert_eq!(view.payload_ttl_days.value, 30);
    assert_eq!(view.payload_ttl_days.source, Source::ConfigFile);
}

#[test]
fn env_overrides_config_file() {
    let _guard = env_lock();
    clean_env();
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("observability.toml");
    fs::write(
        &path,
        r#"
[observability.retention]
mode = "redacted"
payload_ttl_days = 30
"#,
    )
    .unwrap();
    // SAFETY: env mutation; serialized by `env_lock()`.
    unsafe {
        std::env::set_var(MODE_KEY, "full_debug");
        std::env::set_var(TTL_KEY, "90");
    }
    let view = resolve_retention(&path, &CliOverrides::default()).unwrap();
    assert_eq!(view.mode.value, RetentionMode::FullDebug);
    assert_eq!(view.mode.source, Source::Env);
    assert_eq!(view.payload_ttl_days.value, 90);
    assert_eq!(view.payload_ttl_days.source, Source::Env);
    clean_env();
}

#[test]
fn cli_flag_overrides_env_and_file() {
    let _guard = env_lock();
    clean_env();
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("observability.toml");
    fs::write(
        &path,
        r#"
[observability.retention]
mode = "redacted"
payload_ttl_days = 30
"#,
    )
    .unwrap();
    // SAFETY: env mutation; serialized by `env_lock()`.
    unsafe {
        std::env::set_var(MODE_KEY, "full_debug");
        std::env::set_var(TTL_KEY, "90");
    }
    let overrides = CliOverrides {
        mode: Some(RetentionMode::HashOnly),
        payload_ttl_days: Some(3),
        ..CliOverrides::default()
    };
    let view = resolve_retention(&path, &overrides).unwrap();
    assert_eq!(view.mode.value, RetentionMode::HashOnly);
    assert_eq!(view.mode.source, Source::CliFlag);
    assert_eq!(view.payload_ttl_days.value, 3);
    assert_eq!(view.payload_ttl_days.source, Source::CliFlag);
    clean_env();
}

// -------- startup WARN line --------

#[derive(Clone, Default)]
struct VecWriter {
    buf: std::sync::Arc<Mutex<Vec<u8>>>,
}

impl std::io::Write for VecWriter {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.buf.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for VecWriter {
    type Writer = Self;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[test]
fn full_debug_emits_startup_warn() {
    let _guard = env_lock();
    clean_env();
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("observability.toml");
    fs::write(
        &path,
        r#"
[observability.retention]
mode = "full_debug"
"#,
    )
    .unwrap();

    let writer = VecWriter::default();
    let buf_for_assert = writer.buf.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_writer(writer)
        .without_time()
        .with_ansi(false)
        .finish();

    with_default(subscriber, || {
        let _ = resolve_retention(&path, &CliOverrides::default()).unwrap();
    });

    let logged = String::from_utf8(buf_for_assert.lock().unwrap().clone()).unwrap();
    assert!(
        logged.contains("full_debug retention enabled"),
        "expected startup WARN line, got: {logged}"
    );
}

#[test]
fn hash_only_does_not_emit_full_debug_warn() {
    let _guard = env_lock();
    clean_env();
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("observability.toml"); // missing => default hash_only
    let writer = VecWriter::default();
    let buf_for_assert = writer.buf.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_writer(writer)
        .without_time()
        .with_ansi(false)
        .finish();
    with_default(subscriber, || {
        let _ = resolve_retention(&path, &CliOverrides::default()).unwrap();
    });
    let logged = String::from_utf8(buf_for_assert.lock().unwrap().clone()).unwrap();
    assert!(
        !logged.contains("full_debug retention enabled"),
        "did not expect full_debug WARN, got: {logged}"
    );
}

#[test]
fn env_prefix_matches_documented_value() {
    // Sanity check — if someone renames the prefix, the retention CLI's
    // operator-facing docs and this test both need to move together.
    assert_eq!(ENV_OVERRIDE_PREFIX, "XVISION_OBSERVABILITY");
}
