//! The stable context hook transfers pane instructions as data without contacting the daemon.

use std::process::Command;

use argus_protocol::{INSTRUCTIONS_COMMAND, INSTRUCTIONS_VAR};

#[test]
fn the_helper_prints_the_inherited_bootstrap_without_a_daemon() {
    let text = "Load `C:\\work with spaces\\SKILL.md`.\nLiteral: $(exit 9) `exit 9` \"quoted\" %PATH%.";
    let output = Command::new(env!("CARGO_BIN_EXE_argus-hook"))
        .arg(INSTRUCTIONS_COMMAND)
        .env(INSTRUCTIONS_VAR, text)
        .env_remove(argus_protocol::URL_VAR)
        .env_remove(argus_protocol::TOKEN_VAR)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim_end(), text);
    assert!(output.stderr.is_empty());
}

#[test]
fn outside_argus_has_no_instructions() {
    let output = Command::new(env!("CARGO_BIN_EXE_argus-hook"))
        .arg(INSTRUCTIONS_COMMAND)
        .env_remove(INSTRUCTIONS_VAR)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout).unwrap().trim().is_empty());
}

#[cfg(unix)]
#[test]
fn the_shell_hook_handles_executable_paths_with_spaces_and_literal_context() {
    let dir = tempfile::tempdir().unwrap();
    let helper = dir.path().join("argus hook");
    std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_argus-hook"), &helper).unwrap();
    let text = "Load the skill.\n$(exit 9) `exit 9` \"quoted\"";
    let output = Command::new("sh")
        .args(["-c", &format!("\"$ARGUS_HOOK\" {INSTRUCTIONS_COMMAND}")])
        .env(argus_protocol::HELPER_VAR, helper)
        .env(INSTRUCTIONS_VAR, text)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim_end(), text);
}
