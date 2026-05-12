use std::process::Command;

#[test]
fn top_level_help_and_eval_help_describe_eval_run_as_available() {
    let top = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .arg("--help")
        .output()
        .expect("xvn --help");
    assert!(top.status.success());
    let top_stdout = String::from_utf8(top.stdout).unwrap();
    assert!(top_stdout.contains("Eval"), "top-level help should list eval");

    let eval = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(["eval", "--help"])
        .output()
        .expect("xvn eval --help");
    assert!(eval.status.success());
    let eval_stdout = String::from_utf8(eval.stdout).unwrap();
    assert!(eval_stdout.contains("Run an eval"), "eval help should expose run");
    assert!(
        !eval_stdout.contains("deferred to a follow-up"),
        "stale deferred wording must be removed"
    );
}
