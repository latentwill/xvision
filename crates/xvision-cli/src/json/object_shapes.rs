//! `ObjectFormat` — output format for `xvn <object> get`.
//!
//! v1 surface: pretty JSON (default) and compact JSON (single-line,
//! pipe-friendly). The variants are kept narrow on purpose; any future
//! format (TOML, YAML, NDJSON, …) gets its own enum variant and a
//! `match` arm in [`emit_object`].
//!
//! The JSON shape itself comes straight from each object's
//! `serde::Serialize` impl, which is also what `EvalRunExport` embeds
//! in its `strategy` / `scenario` / `agents` slots. That gives the
//! contract acceptance ("Each `--format json` output matches the shape
//! used inside `EvalRunExport`") for free — no separate shape
//! conversion lives here, so it can't drift.

use clap::ValueEnum;
use serde::Serialize;

use crate::exit::{CliResult, ResultExt, XvnExit};

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[clap(rename_all = "kebab-case")]
pub enum ObjectFormat {
    /// Pretty-printed JSON. Default — matches the `xvn` convention of
    /// human-readable output that's still machine-parseable.
    Json,
    /// Single-line compact JSON. Suitable for shell pipes / `jq`.
    JsonCompact,
}

impl Default for ObjectFormat {
    fn default() -> Self {
        Self::Json
    }
}

/// Serialize `value` per [`ObjectFormat`] and write the bytes to stdout
/// with a trailing newline. Maps any serialization error to
/// `XvnExit::Upstream`.
pub fn emit_object<T: Serialize>(value: &T, format: ObjectFormat) -> CliResult<()> {
    let bytes = match format {
        ObjectFormat::Json => serde_json::to_vec_pretty(value).exit_with(XvnExit::Upstream)?,
        ObjectFormat::JsonCompact => serde_json::to_vec(value).exit_with(XvnExit::Upstream)?,
    };
    use std::io::Write;
    std::io::stdout()
        .write_all(&bytes)
        .exit_with(XvnExit::Upstream)?;
    // Trailing newline keeps `xvn <obj> get > out.json` shell-friendly
    // and matches what `xvn eval export` emits.
    println!();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize)]
    struct Demo {
        id: u32,
        name: String,
    }

    #[test]
    fn json_format_round_trips_through_to_string() {
        // The CLI emits via stdout; checking the bytes here would
        // capture stdout. Instead assert that the same writer produces
        // valid JSON for both formats — the integration tests cover the
        // stdout path end-to-end via subprocess.
        let demo = Demo {
            id: 7,
            name: "alpha".into(),
        };

        let pretty = serde_json::to_string_pretty(&demo).unwrap();
        let compact = serde_json::to_string(&demo).unwrap();

        assert!(pretty.contains('\n'), "Json format must be pretty-printed");
        assert!(!compact.contains('\n'), "JsonCompact format must be single-line");

        // Round-trip both: structural equality even though the wire
        // bytes differ.
        let from_pretty: serde_json::Value = serde_json::from_str(&pretty).unwrap();
        let from_compact: serde_json::Value = serde_json::from_str(&compact).unwrap();
        assert_eq!(from_pretty, from_compact);
    }

    #[test]
    fn default_is_pretty_json() {
        assert_eq!(ObjectFormat::default(), ObjectFormat::Json);
    }
}
