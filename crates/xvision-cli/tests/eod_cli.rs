//! Integration test for the `xvn eod` subcommand. Runs the binary against
//! a fresh tempdir XVN_HOME and asserts the markdown report renders all
//! expected sections in the empty-state.

use std::process::Command;

#[test]
fn eod_renders_empty_state_against_fresh_xvn_home() {
    let tmp = tempfile::tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(["eod", "--xvn-home"])
        .arg(tmp.path())
        .output()
        .expect("running xvn eod");

    assert!(
        out.status.success(),
        "xvn eod failed: stderr = {}",
        String::from_utf8_lossy(&out.stderr),
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("# xvision EOD report"));
    assert!(stdout.contains("## Eval runs"));
    assert!(stdout.contains("No eval runs in the window."));
    assert!(stdout.contains("## Audit activity"));
    assert!(stdout.contains("No engine API calls in the window."));
    assert!(stdout.contains("## Errors"));
    assert!(stdout.contains("Zero errors"));
    // Deferred-stub sections should always render so the layout stays
    // stable when the wallet plan + Plan 2c ship.
    assert!(stdout.contains("## Halt status"));
    assert!(stdout.contains("## Positions"));
    assert!(stdout.contains("## Reservation hygiene"));
}
