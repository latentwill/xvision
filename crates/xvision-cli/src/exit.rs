//! Typed exit codes following the Printing Press convention.
//!
//! Agents calling `xvn` programmatically can dispatch on the exit code
//! without parsing error text:
//!
//! ```text
//!   0  Success     command completed
//!   2  Usage       caller-fixable: bad flag, malformed input, unknown enum variant
//!   3  Auth        missing / invalid credential (e.g. ANTHROPIC_API_KEY)
//!   4  NotFound    referenced resource does not exist (strategy id, skill name, run id)
//!   5  Upstream    LLM API / broker / network / file system / database error
//!   7  Conflict    state collision (e.g. attaching a skill to an empty slot)
//! ```
//!
//! Commands carry the category through `CliError`. `From<anyhow::Error>`
//! defaults unattributed failures to `Upstream` so untyped commands keep
//! compiling. Use the `ResultExt::exit_with` helper at error sites to
//! attach a category to a typed `Result`.

use std::process::ExitCode;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XvnExit {
    Success  = 0,
    Usage    = 2,
    Auth     = 3,
    NotFound = 4,
    Upstream = 5,
    Conflict = 7,
}

impl From<XvnExit> for ExitCode {
    fn from(e: XvnExit) -> ExitCode {
        ExitCode::from(e as u8)
    }
}

#[derive(Debug)]
pub struct CliError {
    pub exit: XvnExit,
    pub source: anyhow::Error,
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:#}", self.source)
    }
}

impl std::error::Error for CliError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.source()
    }
}

impl CliError {
    pub fn usage(e: impl Into<anyhow::Error>) -> Self {
        Self { exit: XvnExit::Usage, source: e.into() }
    }
    pub fn auth(e: impl Into<anyhow::Error>) -> Self {
        Self { exit: XvnExit::Auth, source: e.into() }
    }
    pub fn not_found(e: impl Into<anyhow::Error>) -> Self {
        Self { exit: XvnExit::NotFound, source: e.into() }
    }
    pub fn upstream(e: impl Into<anyhow::Error>) -> Self {
        Self { exit: XvnExit::Upstream, source: e.into() }
    }
    pub fn conflict(e: impl Into<anyhow::Error>) -> Self {
        Self { exit: XvnExit::Conflict, source: e.into() }
    }
}

/// Default categorization for untyped failures bubbling up through `?`.
/// Without this, every untyped command's anyhow error would have no exit
/// category. Defaulting to Upstream is the conservative choice — it tells
/// the agent "external system failure, retry might help" rather than the
/// stronger "not found" or "auth", which would mislead retry logic.
impl From<anyhow::Error> for CliError {
    fn from(e: anyhow::Error) -> Self {
        Self { exit: XvnExit::Upstream, source: e }
    }
}

pub type CliResult<T> = Result<T, CliError>;

/// Extension trait letting commands attach an exit category at the error
/// site:
///
/// ```ignore
/// let bundle = store().load(id).await.exit_with(XvnExit::NotFound)?;
/// ```
pub trait ResultExt<T> {
    fn exit_with(self, code: XvnExit) -> CliResult<T>;
}

impl<T, E: Into<anyhow::Error>> ResultExt<T> for Result<T, E> {
    fn exit_with(self, code: XvnExit) -> CliResult<T> {
        self.map_err(|e| CliError { exit: code, source: e.into() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xvn_exit_code_values() {
        assert_eq!(XvnExit::Success as u8,  0);
        assert_eq!(XvnExit::Usage as u8,    2);
        assert_eq!(XvnExit::Auth as u8,     3);
        assert_eq!(XvnExit::NotFound as u8, 4);
        assert_eq!(XvnExit::Upstream as u8, 5);
        assert_eq!(XvnExit::Conflict as u8, 7);
    }

    #[test]
    fn anyhow_to_cli_error_defaults_to_upstream() {
        let e: anyhow::Error = anyhow::anyhow!("boom");
        let c: CliError = e.into();
        assert_eq!(c.exit, XvnExit::Upstream);
    }

    #[test]
    fn result_ext_attaches_category() {
        fn fails() -> anyhow::Result<()> {
            Err(anyhow::anyhow!("missing"))
        }
        let r: CliResult<()> = fails().exit_with(XvnExit::NotFound);
        let err = r.unwrap_err();
        assert_eq!(err.exit, XvnExit::NotFound);
        assert!(err.source.to_string().contains("missing"));
    }

    #[test]
    fn cli_error_helpers_set_correct_category() {
        assert_eq!(CliError::usage(anyhow::anyhow!("x")).exit,    XvnExit::Usage);
        assert_eq!(CliError::auth(anyhow::anyhow!("x")).exit,     XvnExit::Auth);
        assert_eq!(CliError::not_found(anyhow::anyhow!("x")).exit, XvnExit::NotFound);
        assert_eq!(CliError::upstream(anyhow::anyhow!("x")).exit, XvnExit::Upstream);
        assert_eq!(CliError::conflict(anyhow::anyhow!("x")).exit, XvnExit::Conflict);
    }
}
