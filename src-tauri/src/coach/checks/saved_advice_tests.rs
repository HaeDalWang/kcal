use super::*;
use serde_json::json;

fn fixture() -> (TempDir, RecentRecords) {
    static CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let h = TempDir::new(&format!(
        "saved-advice-{}",
        CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    h.write("project/.git/HEAD", "ref: refs/heads/main");
    let mut r = records(&[]);
    r.workspace = Some(h.path().join("project"));
    (h, r)
}
fn has(h: &TempDir, r: &RecentRecords, topic: &str) -> bool {
    super::super::saved_advice::list(&Inputs {
        home: h.path(),
        file: settings_file(h),
        settings: Value::Null,
        records: r,
        tuning: &Tuning::default(),
    })
    .iter()
    .any(|c| matches!(&c.facts, CheckFacts::SavedAdvice {topic: t, ..} if t == topic))
}

#[test]
fn codex_effort_shows_for_max_and_ultra_in_both_top_level_keys() {
    for key in ["model_reasoning_effort", "plan_mode_reasoning_effort"] {
        for value in ["max", "ultra"] {
            let (h, r) = fixture();
            h.write(".codex/config.toml", &format!("{key} = \"{value}\""));
            assert!(has(&h, &r, "reasoning-effort"));
        }
    }
}
#[test]
fn codex_effort_does_not_show_for_other_efforts_profiles_or_invalid_toml() {
    for config in [
        "",
        "model_reasoning_effort = \"xhigh\"",
        "model_reasoning_effort = \"high\"",
        "model_reasoning_effort = true",
        "[profiles.a]\nmodel_reasoning_effort = \"max\"",
        "model_reasoning_effort = \"max\"\nbroken",
        "model_reasoning_effort = \"max\"\nmodel_reasoning_effort = \"low\"",
    ] {
        let (h, r) = fixture();
        h.write(".codex/config.toml", config);
        assert!(!has(&h, &r, "reasoning-effort"), "{config}");
    }
}
#[test]
fn codex_instructions_shows_individual_and_combined_utf8_bytes() {
    let (h, mut r) = fixture();
    h.write(".codex/config.toml", "project_doc_max_bytes = 6");
    h.write("project/AGENTS.md", "가나다");
    assert!(has(&h, &r, "instructions-size"));
    h.write("project/AGENTS.md", "abcd");
    h.write("project/sub/AGENTS.md", "efgh");
    r.workspace = Some(h.path().join("project/sub"));
    assert!(has(&h, &r, "instructions-size"));
    let cards = checks(&h, &r);
    assert!(
        serde_json::to_string(&cards)
            .unwrap()
            .contains("combined-only")
    );
}
#[test]
fn codex_instructions_does_not_show_for_budget_boundary_override_empty_or_unknown() {
    for config in [
        "project_doc_max_bytes = 6",
        "project_doc_max_bytes = -1",
        "project_doc_max_bytes = \"6\"",
        "profile = \"a\"\nproject_doc_max_bytes = 2",
        "project_doc_max_bytes = 2\nproject_root_markers = []",
    ] {
        let (h, mut r) = fixture();
        h.write(".codex/config.toml", config);
        h.write("project/AGENTS.md", "123456");
        h.write("project/sub/empty", "");
        r.workspace = Some(h.path().join("project/sub"));
        assert!(!has(&h, &r, "instructions-size"), "{config}");
    }
    let (h, r) = fixture();
    h.write(".codex/config.toml", "project_doc_max_bytes = 2");
    h.write("project/AGENTS.md", "large");
    h.write("project/AGENTS.override.md", "ok");
    assert!(!has(&h, &r, "instructions-size"));
    h.write("project/AGENTS.override.md", "   ");
    assert!(has(&h, &r, "instructions-size"));
}
#[test]
fn codex_instructions_shows_fallback_custom_root_and_trusted_project_budget() {
    let (h, mut r) = fixture();
    let project = h.path().join("project");
    h.write(".codex/config.toml", &format!("project_root_markers = [\"ROOT\"]\nproject_doc_fallback_filenames = [\"RULES.md\"]\n[projects.{}]\ntrust_level = \"trusted\"", serde_json::to_string(&project.to_string_lossy()).unwrap()));
    h.write("project/ROOT", "");
    h.write("project/RULES.md", "123456");
    h.write("project/.codex/config.toml", "project_doc_max_bytes = 2");
    h.write("project/sub/empty", "");
    r.workspace = Some(project.join("sub"));
    assert!(has(&h, &r, "instructions-size"));
    h.write("project/.codex/config.toml", "project_doc_max_bytes = 9");
    assert!(!has(&h, &r, "instructions-size"));
}
const TOML_HOOK: &str = "[features]\nhooks = true\n[[hooks.SessionStart]]\nmatcher = \"startup\"\n[[hooks.SessionStart.hooks]]\ntype = \"command\"\ncommand = \"echo hello\"\ntimeout = 10\n";
fn hook_json() -> Value {
    json!({"hooks":{"SessionStart":[{"matcher":"startup","hooks":[{"type":"command","command":"echo hello","timeout":10}]}]}})
}
#[test]
fn codex_hooks_shows_identical_complete_handlers_without_exposing_commands() {
    let (h, r) = fixture();
    h.write(".codex/config.toml", TOML_HOOK);
    h.write(".codex/hooks.json", &hook_json().to_string());
    assert!(has(&h, &r, "duplicate-hooks"));
    let serialized = serde_json::to_string(&checks(&h, &r)).unwrap();
    assert!(!serialized.contains("echo hello"));
}
#[test]
fn codex_hooks_does_not_show_for_different_event_matcher_command_fields_or_unknowns() {
    for (pointer, value) in [
        ("/hooks/SessionStart/0/matcher", json!("other")),
        ("/hooks/SessionStart/0/hooks/0/command", json!("echo hell")),
        ("/hooks/SessionStart/0/hooks/0/timeout", json!(11)),
        ("/hooks/SessionStart/0/hooks/0/type", json!("prompt")),
    ] {
        let (h, r) = fixture();
        h.write(".codex/config.toml", TOML_HOOK);
        let mut j = hook_json();
        *j.pointer_mut(pointer).unwrap() = value;
        h.write(".codex/hooks.json", &j.to_string());
        assert!(!has(&h, &r, "duplicate-hooks"));
    }
    for config in [
        TOML_HOOK.replace("hooks = true", "hooks = false"),
        TOML_HOOK.replace("SessionStart", "Stop"),
        format!("{TOML_HOOK}unknown = true\n"),
        TOML_HOOK.replace("type = \"command\"", "type = \"prompt\""),
    ] {
        let (h, r) = fixture();
        h.write(".codex/config.toml", &config);
        h.write(".codex/hooks.json", &hook_json().to_string());
        assert!(!has(&h, &r, "duplicate-hooks"));
    }
}
#[test]
fn codex_hooks_excludes_managed_only_untrusted_projects_and_linked_worktrees() {
    let (h, r) = fixture();
    h.write(".codex/config.toml", TOML_HOOK);
    h.write(".codex/hooks.json", &hook_json().to_string());
    h.write(
        "etc/codex/requirements.toml",
        "allow_managed_hooks_only = true",
    );
    assert!(!has(&h, &r, "duplicate-hooks"));
    h.write(
        "etc/codex/requirements.toml",
        "allow_managed_hooks_only = false",
    );
    h.write(".codex/config.toml", "[features]\nhooks = true");
    h.write("project/.codex/config.toml", TOML_HOOK);
    h.write("project/.codex/hooks.json", &hook_json().to_string());
    assert!(!has(&h, &r, "duplicate-hooks"));
    let trust = format!(
        "[projects.{}]\ntrust_level = \"trusted\"\n",
        serde_json::to_string(&h.path().join("project").to_string_lossy()).unwrap()
    );
    h.write(
        ".codex/config.toml",
        &format!("{trust}\n[features]\nhooks = true"),
    );
    assert!(has(&h, &r, "duplicate-hooks"));
    std::fs::remove_dir_all(h.path().join("project/.git")).unwrap();
    h.write("project/.git", "gitdir: elsewhere");
    assert!(!has(&h, &r, "duplicate-hooks"));
}

fn kiro(h: &TempDir, r: &RecentRecords, v: Option<(u32, u32, u32)>) -> Vec<CoachCheck> {
    super::super::kiro_saved::evaluate(h.path(), r.workspace.as_deref(), v).unwrap_or_default()
}
fn setup_kiro(h: &TempDir, engine: &str) {
    h.write(".kiro/settings/cli.json", &json!({"chat.agentEngine":engine,"toolSearch.enabled":false,"chat.disableAutoCompaction":true}).to_string());
    h.write(
        ".kiro/settings/mcp.json",
        r#"{"mcpServers":{"one":{"command":"unused","env":{"TOKEN":"never materialized"}}}}"#,
    );
}
fn topic(cards: &[CoachCheck], name: &str) -> bool {
    cards
        .iter()
        .any(|c| matches!(&c.facts, CheckFacts::SavedAdvice {topic,..} if topic == name))
}
#[test]
fn kiro_v3_search_shows_for_false_with_mcp_at_version_floor() {
    let (h, r) = fixture();
    setup_kiro(&h, "v3");
    assert!(topic(&kiro(&h, &r, Some((2, 31, 1))), "tool-search-v3"));
}
#[test]
fn kiro_v3_search_does_not_show_below_floor_unknown_version_or_true_unset() {
    let (h, r) = fixture();
    setup_kiro(&h, "v3");
    for v in [None, Some((2, 31, 0))] {
        assert!(!topic(&kiro(&h, &r, v), "tool-search-v3"));
    }
    for value in [json!(true), json!("false"), Value::Null] {
        let mut j = json!({"chat.agentEngine":"v3"});
        if !value.is_null() {
            j["toolSearch.enabled"] = value;
        }
        h.write(".kiro/settings/cli.json", &j.to_string());
        assert!(!topic(&kiro(&h, &r, Some((3, 5, 0))), "tool-search-v3"));
    }
}
#[test]
fn kiro_v2_search_shows_for_false_or_unset_and_uses_workspace_target() {
    let (h, r) = fixture();
    setup_kiro(&h, "v2");
    assert!(topic(&kiro(&h, &r, None), "tool-search-v2"));
    h.write(".kiro/settings/cli.json", r#"{"chat.agentEngine":"v2"}"#);
    let c = kiro(&h, &r, None);
    assert!(topic(&c, "tool-search-v2"));
    assert!(std::path::Path::new(&c[0].file).starts_with(h.path().join("project/.kiro")));
}
#[test]
fn kiro_v2_search_does_not_show_for_true_unselected_engine_or_invalid_type() {
    let (h, r) = fixture();
    setup_kiro(&h, "v2");
    for config in [
        r#"{"chat.agentEngine":"v2","toolSearch.enabled":true}"#,
        r#"{}"#,
        r#"{"chat.agentEngine":"V2"}"#,
        r#"{"chat.agentEngine":"v2","toolSearch.enabled":"false"}"#,
        "not json",
    ] {
        h.write(".kiro/settings/cli.json", config);
        assert!(kiro(&h, &r, Some((3, 5, 0))).is_empty());
    }
}
#[test]
fn kiro_compaction_shows_true_at_floor_with_effective_threshold() {
    let (h, r) = fixture();
    setup_kiro(&h, "v3");
    h.write(
        "project/.kiro/settings/cli.json",
        r#"{"compaction.autoCompactionThreshold":72}"#,
    );
    let cards = kiro(&h, &r, Some((3, 3, 0)));
    assert!(topic(&cards, "auto-compaction"));
    assert!(serde_json::to_string(&cards).unwrap().contains("72%"));
}
#[test]
fn kiro_compaction_does_not_show_for_false_unset_v2_invalid_type_or_old_unknown_version() {
    let (h, r) = fixture();
    setup_kiro(&h, "v3");
    for v in [None, Some((3, 2, 99))] {
        assert!(!topic(&kiro(&h, &r, v), "auto-compaction"));
    }
    for config in [
        r#"{"chat.agentEngine":"v3"}"#,
        r#"{"chat.agentEngine":"v3","chat.disableAutoCompaction":false}"#,
        r#"{"chat.agentEngine":"v3","chat.disableAutoCompaction":"true"}"#,
        r#"{"chat.agentEngine":"v2","chat.disableAutoCompaction":true}"#,
    ] {
        h.write(".kiro/settings/cli.json", config);
        assert!(!topic(&kiro(&h, &r, Some((3, 5, 0))), "auto-compaction"));
    }
}
#[test]
fn kiro_cards_respect_workspace_booleans_global_only_engine_and_invalid_layers() {
    let (h, r) = fixture();
    setup_kiro(&h, "v3");
    h.write(
        "project/.kiro/settings/cli.json",
        r#"{"chat.agentEngine":"v2","toolSearch.enabled":true,"chat.disableAutoCompaction":false}"#,
    );
    assert!(kiro(&h, &r, Some((3, 5, 0))).is_empty());
    for config in [
        "broken",
        "[]",
        r#"{"toolSearch.enabled":0}"#,
        r#"{"compaction.autoCompactionThreshold":"80"}"#,
        r#"{"chat.defaultAgent":"custom"}"#,
        r#"{"cloud.sync":true}"#,
    ] {
        h.write("project/.kiro/settings/cli.json", config);
        assert!(kiro(&h, &r, Some((3, 5, 0))).is_empty(), "{config}");
    }
}
#[test]
fn kiro_search_excludes_absent_disabled_malformed_mcp_and_unresolved_agents() {
    for data in [
        r#"{}"#,
        r#"{"mcpServers":{"one":{"disabled":true}}}"#,
        "bad",
        r#"{"mcpServers":{"one":{"disabled":"false"}}}"#,
    ] {
        let (h, r) = fixture();
        setup_kiro(&h, "v2");
        h.write(".kiro/settings/mcp.json", data);
        assert!(kiro(&h, &r, None).is_empty());
    }
    let (h, r) = fixture();
    setup_kiro(&h, "v2");
    h.write(
        "project/.kiro/settings/mcp.json",
        r#"{"mcpServers":{"one":{"disabled":true}}}"#,
    );
    assert!(kiro(&h, &r, None).is_empty());
    h.write("project/.kiro/settings/mcp.json", r#"{}"#);
    h.write("project/.kiro/agents/custom.md", "unknown");
    assert!(kiro(&h, &r, None).is_empty());
}
#[test]
fn all_saved_advice_options_reject_apply_without_writes() {
    let (h, r) = fixture();
    setup_kiro(&h, "v2");
    h.write(
        ".codex/config.toml",
        "model_reasoning_effort = \"max\"\nproject_doc_max_bytes = 1",
    );
    h.write("project/AGENTS.md", "long");
    for card in checks(&h, &r)
        .into_iter()
        .filter(|c| matches!(c.facts, CheckFacts::SavedAdvice { .. }))
    {
        assert!(
            card.options
                .iter()
                .all(|o| !o.is_writable && o.ops.is_empty() && o.changes.is_empty())
        );
        let before = std::fs::read(&card.file).ok();
        assert!(
            apply(
                h.path(),
                &r,
                &Tuning::default(),
                &h.path().join("log"),
                &card.id,
                "guide",
                &edit::content_hash(b"x"),
                NOW
            )
            .is_err()
        );
        assert_eq!(std::fs::read(&card.file).ok(), before);
        assert!(!h.path().join("log").exists());
    }
}

#[test]
fn saved_toml_defers_invalid_or_unsupported_syntax_without_panicking() {
    for text in [
        "x = '",
        "x = null",
        "x = {}",
        "not a key = true",
        "[not a table]\nx = true",
        "x = \"open",
        "x = [\n 1,\n]",
        "[[hooks.SessionStart.hooks]]\ntype = \"command\"",
    ] {
        assert!(super::super::saved_toml::parse(text).is_none(), "{text}");
    }
}
#[test]
fn codex_hooks_keeps_managed_system_layer_and_excludes_include_files() {
    let (h, r) = fixture();
    h.write(".codex/config.toml", TOML_HOOK);
    h.write(".codex/hooks.json", &hook_json().to_string());
    h.write(
        "etc/codex/requirements.toml",
        "allow_managed_hooks_only = true",
    );
    h.write("etc/codex/config.toml", TOML_HOOK);
    h.write("etc/codex/hooks.json", &hook_json().to_string());
    assert!(has(&h, &r, "duplicate-hooks"));
    let cards = checks(&h, &r);
    let hook = cards
        .iter()
        .find(|c| c.id.contains("duplicate-hooks"))
        .unwrap();
    assert!(std::path::Path::new(&hook.file).starts_with(h.path().join("etc/codex")));
    let mut json = hook_json();
    json["include"] = json!("other.json");
    h.write("etc/codex/hooks.json", &json.to_string());
    assert!(!has(&h, &r, "duplicate-hooks"));
}
#[test]
fn codex_hooks_compare_async_output_limits_and_windows_overrides_in_full() {
    for (key, value) in [
        ("async", json!(true)),
        ("additionalContextLimit", json!(1024)),
        ("commandWindows", json!("echo win")),
    ] {
        let (h, r) = fixture();
        let toml_value = value.to_string();
        h.write(
            ".codex/config.toml",
            &format!("{TOML_HOOK}{key} = {toml_value}\n"),
        );
        let mut json = hook_json();
        json["hooks"]["SessionStart"][0]["hooks"][0][key] = value;
        h.write(".codex/hooks.json", &json.to_string());
        assert!(has(&h, &r, "duplicate-hooks"));
        h.write(".codex/hooks.json", &hook_json().to_string());
        assert!(!has(&h, &r, "duplicate-hooks"));
    }
}
#[test]
fn codex_instructions_default_budget_and_empty_files_are_not_assumed_oversized() {
    let (h, r) = fixture();
    h.write("project/AGENTS.md", &"x".repeat(32768));
    assert!(!has(&h, &r, "instructions-size"));
    h.write("project/AGENTS.md", &"x".repeat(32769));
    assert!(has(&h, &r, "instructions-size"));
    h.write("project/AGENTS.md", "  \n");
    assert!(!has(&h, &r, "instructions-size"));
}

#[test]
fn kiro_version_parses_semver_on_any_stdout_line() {
    use super::super::kiro_saved::parse_version;
    for text in [
        "kiro-cli 3.5.0",
        "notice\ninstalled version: 3.5.0\n",
        "v3.5.0",
    ] {
        assert_eq!(parse_version(text), Some((3, 5, 0)), "{text}");
    }
    for text in [
        "",
        "kiro-cli unknown",
        "3.5",
        "3.5.0.1",
        "9999999999999.5.0",
    ] {
        assert_eq!(parse_version(text), None, "{text}");
    }
}

#[test]
fn kiro_version_probe_is_skipped_without_eligible_v3_saved_settings() {
    use super::super::kiro_saved::evaluate_with_probe;
    for engine in ["v2", "v3", "unknown"] {
        let (h, r) = fixture();
        setup_kiro(&h, engine);
        if engine == "v3" {
            h.write(
                "project/.kiro/settings/cli.json",
                r#"{"toolSearch.enabled":true,"chat.disableAutoCompaction":false}"#,
            );
        }
        let _ = evaluate_with_probe(h.path(), r.workspace.as_deref(), || panic!("no V3 card"));
    }
    let (h, r) = fixture();
    setup_kiro(&h, "v3");
    h.write(
        ".kiro/settings/cli.json",
        r#"{"chat.agentEngine":"v3","toolSearch.enabled":false}"#,
    );
    h.write(
        "project/.kiro/settings/mcp.json",
        r#"{"mcpServers":{"one":{"disabled":true}}}"#,
    );
    assert!(
        evaluate_with_probe(h.path(), r.workspace.as_deref(), || panic!("disabled MCP"))
            .unwrap()
            .is_empty()
    );
    h.write(".kiro/agents/custom.json", "{}");
    assert!(
        evaluate_with_probe(h.path(), r.workspace.as_deref(), || panic!("custom agent")).is_none()
    );
}

#[cfg(unix)]
fn fake_kiro(h: &TempDir, relative: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    h.write(
        relative,
        &format!("#!/bin/sh\n[ \"$#\" -eq 1 ] && [ \"$1\" = '--version' ] || exit 9\n{body}\n"),
    );
    let file = h.path().join(relative);
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o700)).unwrap();
    file
}

#[cfg(unix)]
#[test]
fn kiro_version_probe_uses_first_executable_and_enforces_card_floors() {
    use super::super::kiro_saved::{evaluate_with_probe, installed_version};
    let (h, r) = fixture();
    setup_kiro(&h, "v3");
    fake_kiro(&h, "bin/kiro-cli", "echo 'kiro-cli 9.0.0'");
    let path = h.path().join("bin").into_os_string();
    for (body, search, compaction) in [
        ("echo 'notice'; echo 'kiro-cli 3.5.0'", true, true),
        ("echo 'kiro-cli 2.31.1'", true, false),
        ("echo 'kiro-cli 2.31.0'", false, false),
        ("echo 'unknown version'", false, false),
        ("echo 'kiro-cli 3.5.0'; exit 1", false, false),
        ("echo 'kiro-cli 3.5.0' >&2", false, false),
    ] {
        fake_kiro(&h, ".local/bin/kiro-cli", body);
        let cards = evaluate_with_probe(h.path(), r.workspace.as_deref(), || {
            installed_version(h.path(), Some(&path), std::time::Duration::from_secs(2))
        })
        .unwrap();
        assert_eq!(topic(&cards, "tool-search-v3"), search, "{body}");
        assert_eq!(topic(&cards, "auto-compaction"), compaction, "{body}");
    }
    // Skip a non-executable installed candidate and use only the supplied PATH.
    std::fs::set_permissions(
        h.path().join(".local/bin/kiro-cli"),
        std::os::unix::fs::PermissionsExt::from_mode(0o600),
    )
    .unwrap();
    assert_eq!(
        installed_version(h.path(), Some(&path), std::time::Duration::from_secs(2)),
        Some((9, 0, 0))
    );
}

#[cfg(unix)]
#[test]
fn kiro_version_probe_missing_cli_or_timeout_returns_unknown_and_kills_child() {
    use super::super::kiro_saved::installed_version;
    use std::time::{Duration, Instant};
    let (h, _) = fixture();
    assert_eq!(
        installed_version(h.path(), None, Duration::from_secs(1)),
        None
    );
    fake_kiro(
        &h,
        ".local/bin/kiro-cli",
        "echo $$ > \"$0.pid\"; exec /bin/sleep 30",
    );
    let start = Instant::now();
    assert_eq!(
        installed_version(h.path(), None, Duration::from_secs(1)),
        None
    );
    assert!(start.elapsed() < Duration::from_secs(5));
    let pid = std::fs::read_to_string(h.path().join(".local/bin/kiro-cli.pid")).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if !std::process::Command::new("/bin/kill")
            .args(["-0", pid.trim()])
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap()
            .success()
        {
            break;
        }
        assert!(Instant::now() < deadline, "timed-out CLI still running");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(unix)]
#[test]
fn kiro_version_cache_reuses_success_failure_timeout_and_reprobes_changed_mtime() {
    use super::super::kiro_saved::installed_version;
    use std::time::{Duration, SystemTime};
    for body in ["echo '3.5.0'", "exit 1", "exec /bin/sleep 30"] {
        let (h, _) = fixture();
        let file = fake_kiro(
            &h,
            ".local/bin/kiro-cli",
            &format!("echo call >> \"$0.calls\"; {body}"),
        );
        let timeout = Duration::from_secs(1);
        let first = installed_version(h.path(), None, timeout);
        assert_eq!(first, (body == "echo '3.5.0'").then_some((3, 5, 0)));
        assert_eq!(installed_version(h.path(), None, timeout), first);
        assert_eq!(
            std::fs::read_to_string(file.with_extension("calls"))
                .unwrap()
                .lines()
                .count(),
            1
        );
        std::fs::File::options()
            .write(true)
            .open(&file)
            .unwrap()
            .set_times(
                std::fs::FileTimes::new().set_modified(SystemTime::now() + Duration::from_secs(10)),
            )
            .unwrap();
        assert_eq!(installed_version(h.path(), None, timeout), first);
        assert_eq!(
            std::fs::read_to_string(file.with_extension("calls"))
                .unwrap()
                .lines()
                .count(),
            2
        );
        fake_kiro(&h, "bin/kiro-cli", "echo '9.0.0'");
        std::fs::remove_file(&file).unwrap();
        assert_eq!(
            installed_version(h.path(), Some(h.path().join("bin").as_os_str()), timeout),
            Some((9, 0, 0))
        );
    }
}

#[test]
fn kiro_prerelease_versions_are_unknown_and_hide_v3_cards() {
    use super::super::kiro_saved::parse_version;
    let (h, r) = fixture();
    setup_kiro(&h, "v3");
    for text in ["kiro-cli 3.3.0-beta", "kiro-cli 3.3.0-rc.1"] {
        assert_eq!(parse_version(text), None);
        assert!(kiro(&h, &r, parse_version(text)).is_empty());
    }
}

fn project_trust(h: &TempDir, dir: &str, level: &str) -> String {
    format!(
        "[projects.{}]\ntrust_level = \"{level}\"\n",
        serde_json::to_string(&h.path().join(dir).to_string_lossy()).unwrap()
    )
}

#[test]
fn codex_effort_hides_selected_profile_and_uses_trusted_project_effort_source() {
    let (h, r) = fixture();
    h.write(
        ".codex/config.toml",
        "model_reasoning_effort = \"max\"\nprofile = \"work\"",
    );
    assert!(!has(&h, &r, "reasoning-effort"));
    let trust = project_trust(&h, "project", "trusted");
    h.write(
        ".codex/config.toml",
        &format!("model_reasoning_effort = \"max\"\n{trust}"),
    );
    for key in ["model_reasoning_effort", "plan_mode_reasoning_effort"] {
        for value in ["low", "xhigh", "max", "ultra"] {
            h.write(
                "project/.codex/config.toml",
                &format!("{key} = \"{value}\""),
            );
            let expected = matches!(value, "max" | "ultra");
            assert_eq!(has(&h, &r, "reasoning-effort"), expected, "{key}: {value}");
            if expected {
                let cards = checks(&h, &r);
                let card = cards
                    .iter()
                    .find(|c| c.id == "D-codex-reasoning-effort")
                    .unwrap();
                assert_eq!(
                    PathBuf::from(&card.file),
                    h.path().join("project/.codex/config.toml")
                );
            }
        }
    }
    h.write(
        "project/.codex/config.toml",
        "profile = \"work\"\nmodel_reasoning_effort = \"max\"",
    );
    assert!(!has(&h, &r, "reasoning-effort"));
}

#[test]
fn codex_trust_is_per_directory_and_explicit_untrusted_child_blocks_inheritance() {
    let (h, mut r) = fixture();
    r.workspace = Some(h.path().join("project/sub"));
    h.write("project/.codex/config.toml", "project_doc_max_bytes = 2");
    h.write(
        "project/sub/.codex/config.toml",
        "project_doc_max_bytes = 100",
    );
    h.write("project/AGENTS.md", "long instructions");
    h.write(
        ".codex/config.toml",
        &format!(
            "{}{}",
            project_trust(&h, "project", "trusted"),
            project_trust(&h, "project/sub", "untrusted")
        ),
    );
    assert!(has(&h, &r, "instructions-size"));
    // No child decision: ancestor trust permits the child layer.
    h.write(
        ".codex/config.toml",
        &project_trust(&h, "project", "trusted"),
    );
    assert!(!has(&h, &r, "instructions-size"));
    // Trust in a child must not load an untrusted parent's layer.
    h.write(
        ".codex/config.toml",
        &project_trust(&h, "project/sub", "trusted"),
    );
    h.write("project/sub/.codex/config.toml", "");
    assert!(!has(&h, &r, "instructions-size"));
}

#[test]
fn codex_effort_survives_unknown_array_tables_but_rejects_duplicate_or_invalid_keys() {
    use super::super::saved_toml::parse;
    let (h, r) = fixture();
    let config = "model_reasoning_effort = \"max\"\n[[skills.config]]\npath = \"one\"\nenabled = true\n[[skills.config]]\npath = \"two\"\n[features]\nhooks = false";
    h.write(".codex/config.toml", config);
    assert!(has(&h, &r, "reasoning-effort"));
    let parsed = parse(config).unwrap();
    assert!(!parsed.values.contains_key("skills.config.path"));
    assert_eq!(parsed.values.get("features.hooks"), Some(&json!(false)));
    for invalid in [
        "[[skills.config]]\npath = \"a\"\npath = \"b\"",
        "[[skills.config]]\nbad key = true",
        "[[skills.config]]\nenabled = null",
        "[[skills.config]\nenabled = true",
    ] {
        assert!(parse(invalid).is_none(), "{invalid}");
    }
}

#[test]
fn codex_sessions_search_newest_dates_with_limit_and_falls_back_for_other_layouts() {
    use super::super::saved_workspace::session_files;
    let (h, _) = fixture();
    for path in [
        "sessions/2025/12/31/old.jsonl",
        "sessions/2026/10/31/older.jsonl",
        "sessions/2026/11/21/previous.jsonl",
        "sessions/2026/11/22/new.jsonl",
    ] {
        h.write(path, "{}");
    }
    // An unvisited older subtree with a different layout must not trigger a full walk.
    h.write("sessions/2024/legacy.jsonl", "{}");
    let root = h.path().join("sessions");
    assert_eq!(
        session_files(&root, 1),
        vec![root.join("2026/11/22/new.jsonl")]
    );
    assert_eq!(
        session_files(&root, 2),
        vec![
            root.join("2026/11/22/new.jsonl"),
            root.join("2026/11/21/previous.jsonl")
        ]
    );
    assert!(session_files(&root, 0).is_empty());
    h.write("flat/legacy/session.jsonl", "{}");
    h.write("flat/root.jsonl", "{}");
    assert_eq!(session_files(&h.path().join("flat"), 10).len(), 2);
}
