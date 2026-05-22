//! Stdout/stderr channel discipline for the `xvn` CLI.
//!
//! The contract (`team/contracts/cli-json-stdout-contract.md`) is binary:
//! when a verb is invoked with `--json`, **stdout contains exactly one
//! valid JSON value and nothing else**. All other operator-facing text —
//! progress banners, completion summaries, deprecation warnings, export
//! path notices — routes to **stderr** via [`human`] / [`progress`].
//!
//! This module provides the three primitives every CLI verb should use:
//!
//! - [`human!`] / [`progress!`] — print operator-visible text to **stderr**.
//!   Both are aliases; `progress` reads better when describing a
//!   long-running step ("Starting eval run..."), `human` reads better for
//!   a one-shot completion line ("Run completed.").
//! - [`print_json`] — serialize a `Serialize` value to **stdout** as a
//!   single pretty-printed JSON value, followed by one trailing newline.
//! - [`print_json_compact`] — same channel, but compact one-line form.
//!   Use when the JSON is expected to be piped (`| jq`, `| xargs`).
//!
//! ## Why a dedicated module
//!
//! Pre-2026-05-22 the CLI mixed `println!` for both human progress and
//! JSON payloads. `xvn eval run --json` was a representative offender —
//! it printed "Starting eval run — strategy=... scenario=..." on stdout
//! before the JSON, so downstream tooling had to grep for `^{` to find
//! the start of the JSON. Routing the banners through `human!` (stderr)
//! fixes that without changing the human-friendly default output.
//!
//! ## What counts as "human" vs "JSON"
//!
//! - JSON output (what `--json` makes shell-parseable): stdout via
//!   [`print_json`] / [`print_json_compact`].
//! - Tabular default output (no `--json`, e.g. the `eval list` columns
//!   or the `eval show` summary): stdout via `println!`. That's the
//!   pre-existing contract; we don't touch it.
//! - Banners, progress lines, deprecation notices, "Run completed."
//!   summaries: stderr via [`human!`] / [`progress!`].
//!
//! A small mental rule: if removing `--json` would make this line useful
//! to a human operator, it belongs on stderr (so it still shows with
//! `--json`); otherwise it's part of the structured payload and belongs
//! on stdout.

use std::io::Write;

use serde::Serialize;

use crate::exit::{CliResult, ResultExt, XvnExit};

/// Emit an operator-visible line to **stderr** (always).
///
/// Use for completion summaries, error context, deprecation notices —
/// anything a human should see whether or not `--json` was passed.
///
/// Same signature as `eprintln!`; thin wrapper kept for grep-ability so
/// a future audit can find every banner site with one query.
#[macro_export]
macro_rules! human {
    () => {
        eprintln!()
    };
    ($($arg:tt)*) => {
        eprintln!($($arg)*)
    };
}

/// Alias of [`human!`] that reads better when the line describes a
/// long-running step (`progress!("Starting eval run...")`). Same channel,
/// same semantics — pick whichever name communicates intent.
#[macro_export]
macro_rules! progress {
    () => {
        eprintln!()
    };
    ($($arg:tt)*) => {
        eprintln!($($arg)*)
    };
}

/// Serialize `value` as pretty-printed JSON and write it to **stdout**
/// with a single trailing newline. The bytes written are exactly one
/// JSON value (object, array, scalar) — the trailing `\n` makes the
/// output friendly to redirects (`> out.json`) and `jq` pipes.
///
/// Maps any serialization error to `XvnExit::Upstream`.
pub fn print_json<T: Serialize>(value: &T) -> CliResult<()> {
    let bytes = serde_json::to_vec_pretty(value).exit_with(XvnExit::Upstream)?;
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(&bytes).exit_with(XvnExit::Upstream)?;
    stdout.write_all(b"\n").exit_with(XvnExit::Upstream)?;
    Ok(())
}

/// Same as [`print_json`] but compact (single-line) form. Use when the
/// output is expected to be piped through `jq` or `xargs`.
pub fn print_json_compact<T: Serialize>(value: &T) -> CliResult<()> {
    let bytes = serde_json::to_vec(value).exit_with(XvnExit::Upstream)?;
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(&bytes).exit_with(XvnExit::Upstream)?;
    stdout.write_all(b"\n").exit_with(XvnExit::Upstream)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_json_serializes_value() {
        // We can't easily intercept stdout here, but we can verify the
        // serialization path doesn't choke on common shapes. The
        // end-to-end channel discipline check lives in
        // `tests/json_stdout_contract.rs`.
        let val = serde_json::json!({"ok": true, "n": 7});
        assert!(print_json(&val).is_ok());
        assert!(print_json_compact(&val).is_ok());
    }
}
