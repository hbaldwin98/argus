//! Round-trip tests for the managed hook blocks: what gets written
//! into a checkout, and that taking it out leaves the file as it was.

use super::*;
use argus_protocol::{
    PaneStatus, INSTRUCTIONS_VAR, OWNS_SESSION_FLAG, SESSION_KEY_FLAG, TITLE_FLAG, TOKEN_VAR,
    URL_VAR,
};

fn settings_of(dir: &Path, h: &Harness) -> Value {
    let raw = std::fs::read_to_string(h.settings_path(dir).unwrap()).unwrap();
    serde_json::from_str(&raw).unwrap()
}

fn flat_harness() -> Harness {
    Harness {
        name: "herdr".to_string(),
        settings: Some(PathBuf::from("herdr.json")),
        hooks_key: "on".to_string(),
        shape: Shape::Flat,
        events: vec![
            Event {
                name: "turn_start".into(),
                reports: Report::Working,
                matcher: None,
                note_from_stdin: false,
                title_from_stdin: false,
                session_id_key: None,
                owns_session: false,
                claim_only: false,
            },
            Event {
                name: "turn_end".into(),
                reports: Report::Idle,
                matcher: None,
                note_from_stdin: false,
                title_from_stdin: false,
                session_id_key: None,
                owns_session: false,
                claim_only: false,
            },
        ],
        context_event: None,
        plugin: None,
        resume: Vec::new(),
        resume_id: Vec::new(),
        command_string: false,
        bake_command: false,
        rule_file: None,
        skill_dir: None,
        settings_version: None,
    }
}

#[test]
fn the_built_in_harnesses_know_how_to_be_continued() {
    // What restore appends to each template's command. Wrong flags here
    // mean an agent that comes back with nothing behind it, or one that
    // refuses to start at all.
    assert_eq!(Harness::claude().resume, ["--continue"]);
    assert_eq!(Harness::opencode().resume, ["--continue"]);
    assert_eq!(Harness::agy().resume, ["--continue"]);
    assert_eq!(Harness::agent().resume, ["--continue"]);
    assert_eq!(
        Harness::codex().resume,
        ["resume", "--last"],
        "the last session, not the picker a bare `codex resume` opens"
    );
    assert!(
        Harness::generic().resume.is_empty(),
        "a CLI Argus knows nothing about is asked for nothing"
    );
    assert_eq!(Harness::claude().resume_id, ["--resume", "{session_id}"]);
    assert_eq!(Harness::codex().resume_id, ["resume", "{session_id}"]);
    assert_eq!(Harness::opencode().resume_id, ["--session", "{session_id}"]);
    assert_eq!(Harness::agy().resume_id, ["--conversation", "{session_id}"]);
    assert_eq!(Harness::agent().resume_id, ["--resume", "{session_id}"]);
}

#[test]
fn a_harness_with_no_settings_file_touches_nothing() {
    // The default for an unknown CLI. It still gets the environment;
    // what it must not do is scribble in the user's checkout.
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::generic();
    h.install(dir.path(), PaneId(1), 5555, "tok").unwrap();
    h.uninstall(dir.path()).unwrap();
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn a_flat_harness_installs_where_its_config_says() {
    // The point of the feature: a harness Argus has never heard of is
    // described, not coded.
    let dir = tempfile::tempdir().unwrap();
    let h = flat_harness();
    h.install(dir.path(), PaneId(9), 4242, "tok").unwrap();

    let root = settings_of(dir.path(), &h);
    let entry = &root["on"]["turn_start"][0];
    assert!(is_hook_helper(entry["command"].as_str().unwrap()));
    let args: Vec<String> = serde_json::from_value(entry["args"].clone()).unwrap();
    assert_eq!(args[0], "http://127.0.0.1:4242/pane/9/status/working");
}

#[test]
fn each_event_reports_the_status_its_harness_assigned_it() {
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::claude();
    h.install(dir.path(), PaneId(3), 5555, "tok").unwrap();

    let hooks = settings_of(dir.path(), &h)["hooks"].clone();
    for event in &h.events {
        let args: Vec<String> =
            serde_json::from_value(hooks[&event.name][0]["hooks"][0]["args"].clone()).unwrap();
        assert!(
            args[0].ends_with(&format!("/status/{}", event.reports.as_str())),
            "{} should report {:?}, got {}",
            event.name,
            event.reports,
            args[0]
        );
    }
}

#[test]
fn a_prompt_submit_event_asks_the_helper_to_title_the_pane() {
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::claude();
    h.install(dir.path(), PaneId(3), 5555, "tok").unwrap();

    let hooks = settings_of(dir.path(), &h)["hooks"].clone();
    let prompt: Vec<String> =
        serde_json::from_value(hooks["UserPromptSubmit"][0]["hooks"][0]["args"].clone())
            .unwrap();
    assert!(
        prompt.contains(&TITLE_FLAG.to_string()),
        "UserPromptSubmit carries the user's prompt: {prompt:?}"
    );
    let stop: Vec<String> =
        serde_json::from_value(hooks["Stop"][0]["hooks"][0]["args"].clone()).unwrap();
    assert!(
        !stop.contains(&TITLE_FLAG.to_string()),
        "Stop is not a title: {stop:?}"
    );
}

#[test]
fn the_url_carries_the_pane_and_the_token_follows() {
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::claude();
    h.install(dir.path(), PaneId(42), 5555, "sekrit").unwrap();

    let entry = settings_of(dir.path(), &h)["hooks"]["Stop"][0]["hooks"][0].clone();
    assert_eq!(entry["type"], "command");
    assert!(
        is_hook_helper(entry["command"].as_str().unwrap()),
        "must run our own helper, not a general-purpose HTTP client: {entry:?}"
    );
    let args: Vec<String> = serde_json::from_value(entry["args"].clone()).unwrap();
    assert_eq!(
        args,
        vec![
            "http://127.0.0.1:5555/pane/42/status/idle".to_string(),
            "sekrit".to_string(),
            // Every report says which conversation it came from, so a
            // CLI spawned inside the pane is not mistaken for its own.
            "--session-id-from-stdin".to_string(),
            "session_id".to_string()
        ]
    );
}

#[test]
fn the_context_hook_carries_the_instructions_and_calls_nothing() {
    // An agent's starting context must not depend on the status port
    // being up, so this hook only prints.
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::claude();
    h.install(dir.path(), PaneId(1), 5555, "tok").unwrap();

    let starts = settings_of(dir.path(), &h)["hooks"]["SessionStart"]
        .as_array()
        .unwrap()
        .clone();
    let entry = starts
        .iter()
        .find_map(|matcher| {
            matcher["hooks"]
                .as_array()?
                .iter()
                .find(|hook| hook["args"][0] == "say")
        })
        .unwrap()
        .clone();
    let args: Vec<String> = serde_json::from_value(entry["args"].clone()).unwrap();
    assert_eq!(args[0], "say");
    assert!(args[1].contains(&dir.path().join(".claude/skills/argus/SKILL.md").display().to_string()));
    assert!(!args[1].contains("task add"), "workflow details belong in the skill");
    assert!(
        !args[1].contains("http://"),
        "no network in the instruction hook"
    );
}

#[test]
fn a_new_claude_conversation_clears_stale_status_without_idling_on_compaction() {
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::claude();
    h.install(dir.path(), PaneId(1), 5555, "tok").unwrap();

    let starts = settings_of(dir.path(), &h)["hooks"]["SessionStart"]
        .as_array()
        .unwrap()
        .clone();
    let status = starts
        .iter()
        .find(|matcher| {
            matcher["hooks"][0]["args"][0]
                .as_str()
                .is_some_and(|arg| arg.ends_with("/status/idle"))
        })
        .expect("SessionStart should clear the previous conversation's status");
    assert_eq!(status["matcher"], "startup|resume|clear|fork");
    assert_eq!(
        status["hooks"][0]["args"],
        json!([
            "http://127.0.0.1:5555/pane/1/status/idle",
            "tok",
            SESSION_KEY_FLAG,
            "session_id",
            OWNS_SESSION_FLAG
        ])
    );
    let context = starts
        .iter()
        .find(|matcher| matcher["hooks"][0]["args"][0] == "say")
        .expect("the context hook must survive sharing SessionStart with status");
    assert!(
        context.get("matcher").is_none(),
        "the context hook must still run for every SessionStart source"
    );
    assert_eq!(
        starts.len(),
        2,
        "only the status and context hooks belong here"
    );
}

#[test]
fn the_helper_is_an_absolute_path_next_to_the_daemon() {
    // Nothing installs these binaries on PATH, and the exec form has no
    // shell to resolve a bare name for it.
    let cmd = helper_path();
    assert!(Path::new(&cmd).is_absolute(), "got {cmd:?}");
    assert!(is_hook_helper(&cmd));
}

#[test]
fn the_helper_is_recognized_however_it_was_spelled() {
    assert!(is_hook_helper("argus-hook"));
    assert!(is_hook_helper("argus-hook.exe"));
    assert!(
        is_hook_helper(r"C:\old\target\debug\argus-hook.exe"),
        "a Windows path must be recognized whatever platform reads it"
    );
    assert!(is_hook_helper("/usr/local/bin/argus-hook"));
    assert!(
        is_hook_helper("orion-hook.exe"),
        "a block from before the rename is still ours to clean up"
    );
    assert!(!is_hook_helper("curl.exe"));
    assert!(!is_hook_helper("/bin/sh"));
}

#[test]
fn preserves_unrelated_settings_and_unmanaged_hooks() {
    let dir = tempfile::tempdir().unwrap();
    let claude = dir.path().join(".claude");
    std::fs::create_dir_all(&claude).unwrap();
    std::fs::write(
        claude.join("settings.local.json"),
        r#"{"permissions":{"allow":["Bash"]},"hooks":{"PreToolUse":[{"hooks":[]}]}}"#,
    )
    .unwrap();

    let h = Harness::claude();
    h.install(dir.path(), PaneId(1), 1234, "tok").unwrap();

    let root = settings_of(dir.path(), &h);
    assert_eq!(root["permissions"]["allow"][0], "Bash");
    assert!(root["hooks"]["PreToolUse"].is_array());
    assert!(root["hooks"]["Stop"].is_array());
}

#[test]
fn codex_uses_its_project_hook_shape_and_cleans_up_only_its_handler() {
    let dir = tempfile::tempdir().unwrap();
    let codex = dir.path().join(".codex");
    std::fs::create_dir_all(&codex).unwrap();
    std::fs::write(
        codex.join("hooks.json"),
        r#"{"description":"mine","hooks":{"SessionStart":[{"matcher":"startup","hooks":[{"type":"command","command":"my-hook"}]}]}}"#,
    )
    .unwrap();

    let h = Harness::codex();
    h.install(dir.path(), PaneId(8), 4242, "tok").unwrap();
    let root = settings_of(dir.path(), &h);
    assert_eq!(root["description"], "mine");
    let groups = root["hooks"]["SessionStart"].as_array().unwrap();
    assert_eq!(groups.len(), 3, "the user's hook survives beside status and context");
    let ours = groups
        .iter()
        .find(|group| group["matcher"] == "startup|resume|clear")
        .unwrap();
    let command = &ours["hooks"][0];
    assert!(command["command"]
        .as_str()
        .unwrap()
        .contains(SESSION_KEY_FLAG));
    assert!(command["commandWindows"].is_string());
    assert!(
        command.get("args").is_none(),
        "Codex requires one command string"
    );

    h.uninstall(dir.path()).unwrap();
    let root = settings_of(dir.path(), &h);
    assert_eq!(root["description"], "mine");
    assert_eq!(root["hooks"]["SessionStart"].as_array().unwrap().len(), 1);
    assert_eq!(
        root["hooks"]["SessionStart"][0]["hooks"][0]["command"],
        "my-hook"
    );
}

#[test]
fn codex_hook_content_stays_stable_across_panes_and_daemon_boots() {
    // Codex trusts each project hook by a hash of its handler. Pane IDs,
    // ports, and per-boot tokens therefore cannot be baked into it.
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::codex();

    h.install(dir.path(), PaneId(1), 1111, "first-token")
        .unwrap();
    let first = settings_of(dir.path(), &h);

    h.install(dir.path(), PaneId(9), 9999, "second-token")
        .unwrap();
    let second = settings_of(dir.path(), &h);

    assert_eq!(
        first, second,
        "reinstalling must not invalidate Codex trust"
    );
    let context = &second["hooks"]["SessionStart"][1]["hooks"][0];
    assert_eq!(context["command"], format!("\"$ARGUS_HOOK\" {INSTRUCTIONS_COMMAND}"));
    assert_eq!(context["commandWindows"], format!("\"%ARGUS_HOOK%\" {INSTRUCTIONS_COMMAND}"));
    assert!(context.get("args").is_none());
    assert!(second["hooks"]["SessionStart"][1].get("matcher").is_none(), "context also returns after compaction");
    let second = &second["hooks"]["SessionStart"][0]["hooks"][0];
    assert_eq!(
        second["command"],
        r#""$ARGUS_HOOK" "$ARGUS_HOOK_URL/status/idle" "$ARGUS_HOOK_TOKEN" "--session-id-from-stdin" "session_id" "--owns-session""#
    );
    assert_eq!(
        second["commandWindows"],
        r#""%ARGUS_HOOK%" "%ARGUS_HOOK_URL%/status/idle" "%ARGUS_HOOK_TOKEN%" "--session-id-from-stdin" "session_id" "--owns-session""#
    );
}

#[test]
fn reinstalling_replaces_rather_than_appends() {
    // Every agent spawn rewrites these; a stale entry pointing at a dead
    // port/pane must not accumulate across spawns.
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::claude();
    h.install(dir.path(), PaneId(1), 1111, "a").unwrap();
    h.install(dir.path(), PaneId(2), 2222, "b").unwrap();

    let stop = settings_of(dir.path(), &h)["hooks"]["Stop"].clone();
    assert_eq!(stop.as_array().unwrap().len(), 1, "no duplicate matchers");
    let args: Vec<String> =
        serde_json::from_value(stop[0]["hooks"][0]["args"].clone()).unwrap();
    assert!(args[0].contains("/pane/2/"));
    assert!(args[0].contains("2222"));
}

#[test]
fn recovers_from_a_corrupt_settings_file() {
    let dir = tempfile::tempdir().unwrap();
    let claude = dir.path().join(".claude");
    std::fs::create_dir_all(&claude).unwrap();
    std::fs::write(claude.join("settings.local.json"), "not json at all {{{").unwrap();

    let h = Harness::claude();
    h.install(dir.path(), PaneId(1), 1234, "tok").unwrap();
    assert!(settings_of(dir.path(), &h)["hooks"]["Stop"].is_array());
}

#[test]
fn normalizes_a_non_object_hooks_key() {
    let dir = tempfile::tempdir().unwrap();
    let claude = dir.path().join(".claude");
    std::fs::create_dir_all(&claude).unwrap();
    std::fs::write(claude.join("settings.local.json"), r#"{"hooks":[]}"#).unwrap();

    let h = Harness::claude();
    h.install(dir.path(), PaneId(1), 1234, "tok").unwrap();
    assert!(settings_of(dir.path(), &h)["hooks"].is_object());
}

#[test]
fn creates_the_settings_directory_when_missing() {
    let dir = tempfile::tempdir().unwrap();
    assert!(!dir.path().join(".claude").exists());
    Harness::claude()
        .install(dir.path(), PaneId(1), 1234, "tok")
        .unwrap();
    assert!(dir
        .path()
        .join(".claude")
        .join("settings.local.json")
        .is_file());
}

// --- uninstall ----------------------------------------------------------

#[test]
fn uninstall_removes_every_managed_event_including_the_context_one() {
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::claude();
    h.install(dir.path(), PaneId(1), 5555, "tok").unwrap();
    h.uninstall(dir.path()).unwrap();
    assert!(
        !h.settings_path(dir.path()).unwrap().exists(),
        "a file holding nothing but our hooks should be gone, not left as {{}}"
    );
}

#[test]
fn a_daemon_that_exits_leaves_no_hooks_behind_for_the_next_agent() {
    // Regression: hooks naming a dead daemon's ephemeral port stayed in
    // the checkout forever, so every later agent run in that directory
    // — Argus-managed or not — failed its Stop hook on every turn.
    for h in [Harness::claude(), flat_harness()] {
        let dir = tempfile::tempdir().unwrap();
        h.install(dir.path(), PaneId(4), 65140, "tok").unwrap();
        h.uninstall(dir.path()).unwrap();

        let leftover =
            std::fs::read_to_string(h.settings_path(dir.path()).unwrap()).unwrap_or_default();
        assert!(
            !leftover.contains("65140"),
            "{}: the dead daemon's port must not survive: {leftover}",
            h.name
        );
    }
}

#[test]
fn uninstall_keeps_the_users_own_settings_and_hooks() {
    let dir = tempfile::tempdir().unwrap();
    let claude = dir.path().join(".claude");
    std::fs::create_dir_all(&claude).unwrap();
    std::fs::write(
        claude.join("settings.local.json"),
        r#"{"permissions":{"allow":["Bash"]},
            "hooks":{"PreToolUse":[{"hooks":[{"type":"command","command":"echo hi"}]}]}}"#,
    )
    .unwrap();

    let h = Harness::claude();
    h.install(dir.path(), PaneId(1), 5555, "tok").unwrap();
    h.uninstall(dir.path()).unwrap();

    let root = settings_of(dir.path(), &h);
    assert_eq!(root["permissions"]["allow"][0], "Bash");
    assert!(
        root["hooks"]["PreToolUse"].is_array(),
        "the user's hook survives"
    );
    for event in h.managed_events() {
        assert!(root["hooks"].get(event).is_none(), "{event} should be gone");
    }
}

#[test]
fn uninstall_will_not_touch_a_users_hook_on_a_managed_event_name() {
    // The user is entitled to their own Stop hook. Ours is identified by
    // the command it runs, not by the event it sits on.
    let dir = tempfile::tempdir().unwrap();
    let claude = dir.path().join(".claude");
    std::fs::create_dir_all(&claude).unwrap();
    std::fs::write(
        claude.join("settings.local.json"),
        r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"my-own-script.sh"}]}]}}"#,
    )
    .unwrap();

    Harness::claude().uninstall(dir.path()).unwrap();

    let root = settings_of(dir.path(), &Harness::claude());
    assert_eq!(
        root["hooks"]["Stop"][0]["hooks"][0]["command"], "my-own-script.sh",
        "someone else's Stop hook must survive"
    );
}

#[test]
fn uninstall_is_idempotent_and_safe_on_a_checkout_that_never_had_an_agent() {
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::claude();
    h.uninstall(dir.path()).unwrap();
    assert!(
        !dir.path().join(".claude").exists(),
        "must not create anything"
    );

    h.install(dir.path(), PaneId(1), 5555, "tok").unwrap();
    h.uninstall(dir.path()).unwrap();
    h.uninstall(dir.path()).unwrap();
}

#[test]
fn uninstall_survives_a_corrupt_settings_file() {
    let dir = tempfile::tempdir().unwrap();
    let claude = dir.path().join(".claude");
    std::fs::create_dir_all(&claude).unwrap();
    std::fs::write(claude.join("settings.local.json"), "not json {{{").unwrap();
    Harness::claude().uninstall(dir.path()).unwrap();
}

#[test]
fn install_then_uninstall_round_trips_to_the_original_file() {
    let dir = tempfile::tempdir().unwrap();
    let claude = dir.path().join(".claude");
    std::fs::create_dir_all(&claude).unwrap();
    let original = json!({ "permissions": { "allow": ["Bash"] } });
    std::fs::write(
        claude.join("settings.local.json"),
        serde_json::to_string_pretty(&original).unwrap(),
    )
    .unwrap();

    let h = Harness::claude();
    h.install(dir.path(), PaneId(1), 5555, "tok").unwrap();
    h.uninstall(dir.path()).unwrap();

    assert_eq!(
        settings_of(dir.path(), &h),
        original,
        "no residue left behind"
    );
}

#[test]
fn a_block_written_under_the_other_shape_is_still_removable() {
    // A harness whose shape changed in config between runs would
    // otherwise leave a live block pointing at a dead port.
    let dir = tempfile::tempdir().unwrap();
    let mut h = flat_harness();
    h.install(dir.path(), PaneId(1), 5555, "tok").unwrap();
    h.shape = Shape::Matcher;
    h.uninstall(dir.path()).unwrap();
    assert!(!h.settings_path(dir.path()).unwrap().exists());
}

// --- environment --------------------------------------------------------

#[test]
fn every_agent_is_handed_a_url_and_token_whatever_its_harness() {
    // The universal floor: reporting must not require Argus to
    // understand a harness's config file.
    let env = env(PaneId(7), 4242, "tok");
    let get = |k: &str| env.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
    assert_eq!(get(URL_VAR).unwrap(), "http://127.0.0.1:4242/pane/7");
    assert_eq!(get(TOKEN_VAR).unwrap(), "tok");
    assert_eq!(get("ARGUS_PANE").unwrap(), "7");
    assert!(is_hook_helper(&get("ARGUS_HOOK").unwrap()));
}

#[test]
fn the_environment_also_carries_the_instructions() {
    // For a harness with no context event of its own, this is the only
    // way an agent learns it can rename its pane.
    let env = env(PaneId(1), 1, "t");
    let text = env
        .iter()
        .find(|(n, _)| n == INSTRUCTIONS_VAR)
        .map(|(_, v)| v.clone())
        .unwrap();
    assert!(text.contains("title"));
}

// --- the plugin mechanism ----------------------------------------------

fn plugin_path(dir: &Path, h: &Harness) -> PathBuf {
    dir.join(&h.plugin.as_ref().unwrap().path)
}

#[test]
fn opencode_reports_through_a_plugin_rather_than_a_hook_table() {
    // The bug this exists for: opencode has no JSON hooks, so before
    // this it resolved to the generic harness and sat at Idle for its
    // whole life however hard it was working.
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::opencode();
    assert!(h.settings.is_none(), "opencode has no hook file to write");

    h.install(dir.path(), PaneId(3), 4242, "tok").unwrap();
    let body = std::fs::read_to_string(plugin_path(dir.path(), &h)).unwrap();
    assert!(body.contains(PLUGIN_MARKER));
    // Per-pane facts stay in the environment the module reads at run
    // time, so one file is correct for every pane in the checkout.
    assert!(!body.contains("4242"), "a plugin must not bake in a port");
    assert!(!body.contains("tok"), "nor a token");
}

#[test]
fn a_plugin_comes_back_out_with_the_directory_argus_made_for_it() {
    // Same contract as a hook block: it names a dead port the moment
    // this daemon exits, so it must not outlive the panes that need it.
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::opencode();
    h.install(dir.path(), PaneId(3), 4242, "tok").unwrap();
    h.uninstall(dir.path()).unwrap();
    assert!(!plugin_path(dir.path(), &h).exists());
    assert_eq!(
        std::fs::read_dir(dir.path()).unwrap().count(),
        0,
        "an empty .opencode/ left behind is still litter"
    );
}

#[test]
fn uninstalling_a_plugin_is_idempotent_and_safe_on_a_cold_checkout() {
    // It runs at startup across every configured checkout, most of
    // which have never hosted an agent.
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::opencode();
    h.uninstall(dir.path()).unwrap();
    h.install(dir.path(), PaneId(1), 1, "t").unwrap();
    h.uninstall(dir.path()).unwrap();
    h.uninstall(dir.path()).unwrap();
    assert!(!plugin_path(dir.path(), &h).exists());
}

#[test]
fn a_plugin_the_user_wrote_themselves_is_left_alone() {
    // Their file, their directory. Only a module still carrying our
    // marker is ours to delete.
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::opencode();
    let path = plugin_path(dir.path(), &h);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "export const Mine = async () => ({})").unwrap();

    h.uninstall(dir.path()).unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "export const Mine = async () => ({})"
    );
}

#[test]
fn the_opencode_plugin_maps_every_automatic_state() {
    // Completion states are explicit agent reports taught through the
    // injected instructions; lifecycle events supply only these states.
    let source = Harness::opencode().plugin.unwrap().source;
    for r in [
        Report::Working,
        Report::Idle,
        Report::Waiting,
        Report::Failed,
    ] {
        assert!(
            source.contains(&format!("\"{}\"", r.as_str())),
            "the opencode plugin never reports {}",
            r.as_str()
        );
    }
    // What a manual stop relies on: opencode drops the session to idle
    // on abort as well as on a finished turn.
    assert!(source.contains("session.status"));
    assert!(source.contains("chat.message"));
}

#[test]
fn the_opencode_plugin_reports_root_and_child_sessions_without_transferring_ownership() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = dir.path().join("argus-status.mjs");
    let runner = dir.path().join("runner.mjs");
    std::fs::write(&plugin, Harness::opencode().plugin.unwrap().source).unwrap();
    std::fs::write(
        &runner,
        r#"
import { pathToFileURL } from "node:url";

const reports = [];
globalThis.fetch = async (url, init) => {
  reports.push({
url,
session: init.headers["X-Argus-Session"],
authorization: init.headers.authorization,
note: init.body,
  });
};

const { ArgusStatus } = await import(pathToFileURL(process.argv[2]));
const hooks = await ArgusStatus();
await hooks["chat.message"]({ sessionID: "old" });
await hooks.event({
  event: {
type: "session.error",
properties: { sessionID: "old", error: { name: "PermissionDenied" } },
  },
});
await hooks.event({
  event: {
type: "session.created",
properties: { info: { id: "child", parentID: "old" } },
  },
});
await hooks.event({
  event: {
type: "permission.asked",
properties: { sessionID: "child", title: "Approve child tool" },
  },
});
await hooks.event({
  event: {
type: "session.deleted",
properties: { info: { id: "child", parentID: "old" } },
  },
});
await hooks.event({
  event: {
type: "session.created",
properties: { sessionID: "new", info: { id: "new" } },
  },
});
await hooks["chat.message"]({ sessionID: "new" });
process.stdout.write(JSON.stringify(reports));
"#,
    )
    .unwrap();

    let output = match std::process::Command::new("node")
        .arg(&runner)
        .arg(&plugin)
        .env("ARGUS_HOOK_URL", "http://127.0.0.1/pane/1")
        .env("ARGUS_HOOK_TOKEN", "test-token")
        .output()
    {
        Ok(output) => output,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => panic!("could not run opencode plugin test: {e}"),
    };
    assert!(
        output.status.success(),
        "node failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let reports: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        reports,
        json!([
            { "url": "http://127.0.0.1/pane/1/session", "session": "old", "authorization": "Bearer test-token", "note": "old" },
            { "url": "http://127.0.0.1/pane/1/status/working", "session": "old", "authorization": "Bearer test-token", "note": "" },
            { "url": "http://127.0.0.1/pane/1/status/failed", "session": "old", "authorization": "Bearer test-token", "note": "PermissionDenied" },
            { "url": "http://127.0.0.1/pane/1/status/working", "session": "child", "authorization": "Bearer test-token", "note": "" },
            { "url": "http://127.0.0.1/pane/1/status/waiting", "session": "child", "authorization": "Bearer test-token", "note": "Approve child tool" },
            { "url": "http://127.0.0.1/pane/1/status/idle", "session": "child", "authorization": "Bearer test-token", "note": "" },
            { "url": "http://127.0.0.1/pane/1/session", "session": "new", "authorization": "Bearer test-token", "note": "new" },
            { "url": "http://127.0.0.1/pane/1/status/idle", "session": "new", "authorization": "Bearer test-token", "note": "" },
            { "url": "http://127.0.0.1/pane/1/status/working", "session": "new", "authorization": "Bearer test-token", "note": "" },
        ])
    );
}

#[test]
fn the_opencode_plugin_titles_the_pane_from_the_user_prompt() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = dir.path().join("argus-status.mjs");
    let runner = dir.path().join("runner.mjs");
    std::fs::write(&plugin, Harness::opencode().plugin.unwrap().source).unwrap();
    std::fs::write(
        &runner,
        r#"
import { pathToFileURL } from "node:url";

const reports = [];
globalThis.fetch = async (url, init) => {
  reports.push({ url, note: init.body });
};

const { ArgusStatus } = await import(pathToFileURL(process.argv[2]));
const hooks = await ArgusStatus();
await hooks["chat.message"](
  { sessionID: "s1" },
  { parts: [{ type: "text", text: "fixing the pty deadlock\nmore" }] },
);
await hooks["chat.message"]({ sessionID: "s1" }, { parts: [] });
process.stdout.write(JSON.stringify(reports));
"#,
    )
    .unwrap();

    let output = match std::process::Command::new("node")
        .arg(&runner)
        .arg(&plugin)
        .env("ARGUS_HOOK_URL", "http://127.0.0.1/pane/1")
        .env("ARGUS_HOOK_TOKEN", "test-token")
        .output()
    {
        Ok(output) => output,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => panic!("could not run opencode plugin test: {e}"),
    };
    assert!(
        output.status.success(),
        "node failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let reports: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        reports,
        json!([
            { "url": "http://127.0.0.1/pane/1/session", "note": "s1" },
            { "url": "http://127.0.0.1/pane/1/status/working", "note": "" },
            { "url": "http://127.0.0.1/pane/1/title", "note": "fixing the pty deadlock" },
        ])
    );
}

#[test]
fn the_opencode_plugin_calls_the_same_pane_api_the_helper_does() {
    // The module posts for itself rather than shelling out, so nothing
    // but this stops a change to the route or the environment's names
    // from leaving it talking to an endpoint that no longer exists.
    let source = Harness::opencode().plugin.unwrap().source;
    for var in [URL_VAR, TOKEN_VAR, INSTRUCTIONS_VAR] {
        assert!(source.contains(var), "the plugin never reads {var}");
    }
    assert!(source.contains("/status/${status}"), "wrong pane route");
    assert!(
        source.contains("${BASE}/title"),
        "the plugin never titles the pane"
    );
    assert!(source.contains("Bearer ${TOKEN}"), "wrong authorization");
}

#[test]
fn a_report_round_trips_through_its_wire_name() {
    for r in Report::ALL {
        assert_eq!(Report::parse(r.as_str()), Some(r));
    }
    assert_eq!(
        Report::parse("exited"),
        None,
        "only the daemon decides that"
    );
    assert_eq!(Report::parse(""), None);
}

#[test]
fn every_report_maps_to_its_pane_status() {
    let expected = [
        PaneStatus::Working,
        PaneStatus::Idle,
        PaneStatus::Waiting,
        PaneStatus::NeedsReview,
        PaneStatus::Done,
        PaneStatus::Failed,
    ];
    assert_eq!(Report::ALL.map(Report::status), expected);
}

#[test]
fn agy_installs_into_agents_hooks_json_and_cleans_up() {
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::agy();
    h.install(dir.path(), PaneId(5), 4242, "tok").unwrap();

    let hooks_file = dir.path().join(".agents").join("hooks.json");
    assert!(hooks_file.is_file(), "should write .agents/hooks.json");

    let rule_file = dir.path().join(".agents").join("rules").join("argus.md");
    assert!(rule_file.is_file(), "should write .agents/rules/argus.md");
    let rule_content = std::fs::read_to_string(&rule_file).unwrap();
    assert!(rule_content.contains("SKILL.md"));

    let raw = std::fs::read_to_string(&hooks_file).unwrap();
    let root: Value = serde_json::from_str(&raw).unwrap();

    let argus = &root["argus"];
    assert!(
        argus.is_object(),
        "should be nested under 'argus' hook name"
    );

    let pre_inv = argus["PreInvocation"].as_array().unwrap();
    assert_eq!(pre_inv.len(), 1);
    let pre_entry = &pre_inv[0];
    assert_eq!(pre_entry["type"], "command");
    let pre_args: Vec<String> = serde_json::from_value(pre_entry["args"].clone()).unwrap();
    assert_eq!(pre_args[0], "http://127.0.0.1:4242/pane/5/status/working");
    assert_eq!(pre_args[1], "tok");
    assert_eq!(pre_args[2], TITLE_FLAG);
    assert_eq!(pre_args[3], SESSION_KEY_FLAG);
    assert_eq!(pre_args[4], "conversationId");

    let stop = argus["Stop"].as_array().unwrap();
    assert_eq!(stop.len(), 1);
    let stop_entry = &stop[0];
    assert_eq!(stop_entry["type"], "command");
    let stop_args: Vec<String> = serde_json::from_value(stop_entry["args"].clone()).unwrap();
    assert_eq!(stop_args[0], "http://127.0.0.1:4242/pane/5/status/idle");
    assert_eq!(stop_args[1], "tok");

    // Uninstall sweeps .agents/hooks.json, rules, and prunes the empty .agents directory
    h.uninstall(dir.path()).unwrap();
    assert!(!hooks_file.exists(), ".agents/hooks.json should be removed");
    assert!(
        !rule_file.exists(),
        ".agents/rules/argus.md should be removed"
    );
    assert!(
        !dir.path().join(".agents").exists(),
        "empty .agents dir should be pruned"
    );
}

#[test]
fn cursor_agent_installs_into_hooks_json_and_cleans_up() {
    let dir = tempfile::tempdir().unwrap();
    let h = Harness::agent();
    h.install(dir.path(), PaneId(5), 4242, "tok").unwrap();

    let hooks_file = dir.path().join(".cursor").join("hooks.json");
    assert!(hooks_file.is_file(), "should write .cursor/hooks.json");

    let rule_file = dir.path().join(".cursor").join("rules").join("argus.mdc");
    assert!(rule_file.is_file(), "should write .cursor/rules/argus.mdc");
    let rule_content = std::fs::read_to_string(&rule_file).unwrap();
    assert!(rule_content.contains("alwaysApply: true"));
    assert!(rule_content.contains("SKILL.md"));

    let raw = std::fs::read_to_string(&hooks_file).unwrap();
    let root: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(root["version"], 1);

    let hooks = &root["hooks"];
    assert!(hooks.is_object());

    let start = hooks["sessionStart"].as_array().unwrap();
    assert_eq!(start.len(), 1);
    let start_cmd = start[0]["command"].as_str().unwrap();
    assert!(
        start_cmd.contains("http://127.0.0.1:4242/pane/5/session"),
        "sessionStart claims identity without posting idle:\n{start_cmd}"
    );
    assert!(
        !start_cmd.contains("/status/idle"),
        "a late sessionStart idle would snap a working pane back:\n{start_cmd}"
    );
    assert!(start_cmd.contains("tok"));
    assert!(start_cmd.contains(SESSION_KEY_FLAG));
    assert!(start_cmd.contains("conversation_id"));
    assert!(start_cmd.contains(OWNS_SESSION_FLAG));
    assert!(
        !start_cmd.contains("ARGUS_HOOK"),
        "env-based routing silently fails under Cursor"
    );

    let working = hooks["beforeSubmitPrompt"].as_array().unwrap();
    assert_eq!(working.len(), 1);
    let working_cmd = working[0]["command"].as_str().unwrap();
    assert!(working_cmd.contains("/status/working"));
    assert!(
        working_cmd.contains(TITLE_FLAG),
        "beforeSubmitPrompt carries the user's prompt:\n{working_cmd}"
    );

    // Tool-start is the CLI-reliable working signal when lifecycle hooks
    // do not fire; neither event may clear idle on its own.
    for event in ["preToolUse", "beforeShellExecution"] {
        let entries = hooks[event].as_array().unwrap();
        assert_eq!(entries.len(), 1, "{event}");
        let cmd = entries[0]["command"].as_str().unwrap();
        assert!(cmd.contains("/status/working"), "{event}");
        assert!(cmd.contains("conversation_id"), "{event}");
        assert!(
            !cmd.contains(TITLE_FLAG),
            "a tool-start event is not a title:\n{cmd}"
        );
        assert!(!cmd.contains("ARGUS_HOOK"), "{event}");
    }

    let stop = hooks["stop"].as_array().unwrap();
    assert_eq!(stop.len(), 1);
    let stop_cmd = stop[0]["command"].as_str().unwrap();
    assert!(stop_cmd.contains("/status/idle"));

    h.uninstall(dir.path()).unwrap();
    assert!(!hooks_file.exists(), ".cursor/hooks.json should be removed");
    assert!(
        !rule_file.exists(),
        ".cursor/rules/argus.mdc should be removed"
    );
    assert!(
        !dir.path().join(".cursor").exists(),
        "empty .cursor dir should be pruned"
    );
}
