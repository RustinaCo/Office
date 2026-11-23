//! `--help` is a request, not a failure (#1525): it goes to stdout with exit 0, like `--version`.
//! Bare `cadcraft-cli` keeps reporting on stderr with exit 1.

use std::process::Command;

#[test]
fn help_flags_print_usage_to_stdout_and_exit_zero() {
    for args in [&["--help"][..], &["-h"], &["help"]] {
        let out = Command::new(env!("CARGO_BIN_EXE_cadcraft-cli")).args(args).output().expect("run cadcraft-cli");
        assert!(out.status.success(), "{args:?}: {:?}", out.status);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.starts_with("usage:"), "{args:?}: {stdout:?}");
        assert!(String::from_utf8_lossy(&out.stderr).is_empty(), "{args:?}: stderr should be empty");
    }
}

#[test]
fn bare_invocation_still_reports_on_stderr() {
    let out = Command::new(env!("CARGO_BIN_EXE_cadcraft-cli")).output().expect("run cadcraft-cli");
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty(), "bare invocation keeps its usage on stderr");
    assert!(String::from_utf8_lossy(&out.stderr).starts_with("usage:"));
}
