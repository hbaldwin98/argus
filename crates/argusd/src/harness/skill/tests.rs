//! Skill installation preserves user files and never points an agent at a partial package.

use super::*;
use argus_protocol::PaneId;

#[test]
fn builtins_install_a_complete_skill_and_remove_only_their_package() {
    for harness in Harness::builtins().into_iter().filter(|h| h.skill_dir.is_some()) {
        let checkout = tempfile::tempdir().unwrap();
        let root = checkout.path().join(harness.skill_dir.as_ref().unwrap());
        harness.install(checkout.path(), PaneId(1), 1234, "token").unwrap();
        for (name, source) in FILES {
            assert_eq!(std::fs::read_to_string(root.join(name)).unwrap(), *source);
        }
        let instructions = harness.instructions(checkout.path());
        assert!(instructions.contains(&root.join("SKILL.md").display().to_string()));
        assert!(!instructions.contains("task add"));
        std::fs::write(root.join("personal.md"), "keep this").unwrap();
        harness.uninstall(checkout.path()).unwrap();
        for (name, _) in FILES {
            assert!(!root.join(name).exists());
        }
        assert_eq!(std::fs::read_to_string(root.join("personal.md")).unwrap(), "keep this");
    }
}

#[test]
fn a_skill_without_a_user_file_leaves_no_directories_behind() {
    let checkout = tempfile::tempdir().unwrap();
    let h = Harness::codex();
    h.install_skill(checkout.path()).unwrap();
    h.uninstall_skill(checkout.path()).unwrap();
    h.uninstall_skill(checkout.path()).unwrap();
    assert_eq!(std::fs::read_dir(checkout.path()).unwrap().count(), 0);
}

#[test]
fn a_user_owned_skill_or_reference_is_never_overwritten() {
    for (name, _) in FILES {
        let checkout = tempfile::tempdir().unwrap();
        let h = Harness::claude();
        let root = checkout.path().join(h.skill_dir.as_ref().unwrap());
        let user_file = root.join(name);
        std::fs::create_dir_all(user_file.parent().unwrap()).unwrap();
        std::fs::write(&user_file, "user content").unwrap();
        assert!(h.install(checkout.path(), PaneId(1), 1234, "token").is_err());
        assert!(h.settings_path(checkout.path()).unwrap().exists(), "hooks still install");
        assert_eq!(h.instructions(checkout.path()), fallback());
        h.uninstall(checkout.path()).unwrap();
        assert_eq!(std::fs::read_to_string(user_file).unwrap(), "user content");
    }
}

#[test]
fn a_file_replaced_by_the_user_survives_cleanup() {
    let checkout = tempfile::tempdir().unwrap();
    let h = Harness::codex();
    h.install_skill(checkout.path()).unwrap();
    let path = checkout.path().join(h.skill_dir.as_ref().unwrap()).join("SKILL.md");
    std::fs::write(&path, "replacement skill").unwrap();
    h.uninstall_skill(checkout.path()).unwrap();
    assert_eq!(std::fs::read_to_string(path).unwrap(), "replacement skill");
}

#[test]
fn reinstall_updates_managed_files_and_repairs_a_missing_reference() {
    let checkout = tempfile::tempdir().unwrap();
    let h = Harness::codex();
    h.install_skill(checkout.path()).unwrap();
    let root = checkout.path().join(h.skill_dir.as_ref().unwrap());
    std::fs::write(root.join("SKILL.md"), format!("{MARKER}\nold version")).unwrap();
    std::fs::remove_file(root.join("references/work.md")).unwrap();
    assert_eq!(h.instructions(checkout.path()), fallback());
    h.install_skill(checkout.path()).unwrap();
    for (name, source) in FILES {
        assert_eq!(std::fs::read_to_string(root.join(name)).unwrap(), *source);
    }
}

#[test]
fn unsupported_and_uninstalled_harnesses_use_the_compact_fallback() {
    let checkout = tempfile::tempdir().unwrap();
    for h in [Harness::generic(), Harness::codex()] {
        assert_eq!(h.instructions(checkout.path()), fallback());
    }
    Harness::generic().install_skill(checkout.path()).unwrap();
    assert_eq!(std::fs::read_dir(checkout.path()).unwrap().count(), 0);
}

#[test]
fn configured_skill_directories_cannot_escape_the_checkout() {
    let checkout = tempfile::tempdir().unwrap();
    for dir in [PathBuf::from("../elsewhere"), checkout.path().join("absolute")] {
        let mut h = Harness::generic();
        h.skill_dir = Some(dir);
        assert!(h.install_skill(checkout.path()).is_err());
        assert!(h.uninstall_skill(checkout.path()).is_err());
        assert_eq!(h.instructions(checkout.path()), fallback());
    }
}

#[cfg(unix)]
#[test]
fn symlinked_skill_roots_and_files_are_left_alone() {
    for name in [".agents", ".agents/skills/argus/SKILL.md"] {
        let checkout = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let target = outside.path().join("target");
        std::fs::write(&target, format!("{MARKER}\nkeep this")).unwrap();
        let link = checkout.path().join(name);
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(if name == ".agents" { outside.path() } else { &target }, &link).unwrap();
        let h = Harness::codex();
        assert!(h.install_skill(checkout.path()).is_err());
        let _ = h.uninstall_skill(checkout.path());
        assert!(std::fs::symlink_metadata(link).unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(target).unwrap(), format!("{MARKER}\nkeep this"));
        assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 1);
    }
}
