//! Non-TTY fallback tests for the opt-in `--animate` intro.
//!
//! When stdout is not an interactive terminal (pipe or redirection),
//! `--animate` must produce byte-identical output to the regular static
//! run, with no cursor-control or frame-stream escape sequences.

use std::process::Command;

fn astrofetch(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_astrofetch"))
        .args(args)
        .output()
        .expect("failed to spawn astrofetch")
}

#[test]
fn static_piped_output_contains_no_escape_sequences() {
    let out = astrofetch(&["--logo-only", "--seed", "42", "--no-update-check"]);
    assert!(out.status.success(), "static run must succeed");
    assert!(
        !out.stdout.contains(&0x1b),
        "static piped output must not contain escape sequences"
    );
}

#[test]
fn animate_with_piped_stdout_is_byte_identical_to_static_output() {
    let static_out = astrofetch(&["--logo-only", "--seed", "42", "--no-update-check"]);
    assert!(static_out.status.success(), "static run must succeed");

    let animated_out = astrofetch(&[
        "--logo-only",
        "--seed",
        "42",
        "--no-update-check",
        "--animate",
    ]);
    assert!(
        animated_out.status.success(),
        "--animate run must succeed with piped stdout"
    );
    assert!(
        animated_out.stdout.len() > 100,
        "output should be the normal static frame"
    );
    assert!(
        !animated_out.stdout.contains(&0x1b),
        "piped --animate output must not contain cursor-control/frame-stream sequences"
    );
    assert_eq!(
        static_out.stdout, animated_out.stdout,
        "non-TTY --animate output must be byte-identical to the static output"
    );
}

#[test]
fn animate_conflicts_with_info_only_at_the_cli() {
    let out = astrofetch(&["--animate", "--info-only"]);
    assert!(
        !out.status.success(),
        "--animate --info-only must be rejected"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("cannot be used with"),
        "clap should report the conflict: {stderr}"
    );
}
