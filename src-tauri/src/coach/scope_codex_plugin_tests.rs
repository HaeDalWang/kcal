//! 같은 임시 HOME·rollout·transaction helper로 Codex plugin 경로를 검증한다.
use super::*;

const ID: &str = "browser@openai-bundled";
const SECTION: &str = "plugins.\"browser@openai-bundled\"";

fn plugin(f: &Fixture) -> CoachSwitch {
    switches::find(
        switches::list(f.home.path(), None, Some(&index(f)), &Tuning::default()),
        &format!("codex/plugin:{ID}"),
    )
    .unwrap()
}

fn setup(name: &str, trusted: bool) -> Fixture {
    let f = fixture(&format!("plugin-{name}"), trusted);
    let config = std::fs::read_to_string(&f.user).unwrap();
    std::fs::write(&f.user, format!("{config}\n[{SECTION}]\nenabled = true\n")).unwrap();
    let skill = f
        .home
        .path()
        .join(".codex/plugins/cache/openai-bundled/browser/1/skills/web/SKILL.md");
    let command = format!("cat '{}'", skill.display());
    let exec = format!("await tools.exec_command({})", json!({"cmd": command}));
    let mut direct = direct_call("p2", "exec_command");
    direct["payload"]["arguments"] = json!(json!({"cmd": command}).to_string());
    rollout(
        &f.home,
        "a",
        &f.a.join("src"),
        &[
            code_call("p1", &exec),
            code_call("p1", &exec),
            // MCP 완료 기록은 plugin 읽기 근거를 대체하면 안 된다.
            completed_call("m1", "other"),
            code_call("p3", &exec),
        ],
    );
    rollout(&f.home, "b", &f.b, &[direct]);
    f
}

#[test]
fn counts_exec_and_function_reads_once_per_call_and_keeps_markets_separate() {
    let f = setup("counts", true);
    let index = index(&f).codex.unwrap();
    let used = index.for_codex_plugin(ID);
    assert_eq!(used.total, 3);
    assert_eq!(
        used.projects
            .iter()
            .map(|p| (&*p.name, p.count))
            .collect::<Vec<_>>(),
        [("a", 2), ("b", 1)]
    );
    assert_eq!(index.for_codex_plugin("browser@other").total, 0);
    assert_eq!(used.sessions, 2);
    assert!(plugin(&f).scope_move.is_some());
    assert!(!plugin(&f).is_proposed);
}

#[test]
fn requires_explicit_enabled_trust_complete_history_and_usage_thresholds() {
    let f = setup("gates", true);
    assert!(plugin(&f).scope_move.is_some());
    let used = plugin(&f).usage.unwrap();
    let trusted =
        crate::context::scan::codex_trusted_projects(&std::fs::read_to_string(&f.user).unwrap());
    for changed in [
        usage::SwitchUsage {
            total: 2,
            ..used.clone()
        },
        usage::SwitchUsage {
            sessions: 0,
            min_sessions: 1,
            ..used.clone()
        },
        usage::SwitchUsage {
            is_partial: true,
            ..used.clone()
        },
        usage::SwitchUsage {
            project_count: 4,
            ..used.clone()
        },
    ] {
        assert!(scope::codex::proposal(f.home.path(), true, &changed, &trusted).is_none());
    }
    assert!(plugin(&setup("untrusted", false)).scope_move.is_none());
    let text = std::fs::read_to_string(&f.user).unwrap();
    std::fs::write(
        &f.user,
        text.replace(
            &format!("[{SECTION}]\nenabled = true"),
            &format!("[plugins._default]\nenabled = true\n[{SECTION}]"),
        ),
    )
    .unwrap();
    assert!(plugin(&f).enabled);
    assert!(plugin(&f).scope_move.is_none());
    std::fs::write(
        &f.user,
        text.replace(
            &format!("[{SECTION}]\nenabled = true"),
            &format!("[{SECTION}]\nenabled = false"),
        ),
    )
    .unwrap();
    assert!(plugin(&f).scope_move.is_none());
}

#[test]
fn applies_new_and_existing_sections_and_missing_files_then_undoes_every_byte() {
    for existing in [
        B_OWN.to_owned(),
        format!("{B_OWN}[{SECTION}]\nenabled = false # keep\n"),
    ] {
        let f = setup(
            if existing == B_OWN {
                "new-section"
            } else {
                "existing"
            },
            true,
        );
        std::fs::write(f.b.join(".codex/config.toml"), &existing).unwrap();
        let user = std::fs::read(&f.user).unwrap();
        let p = preview(&plugin(&f)).unwrap();
        let entry = apply(
            &plugin(&f),
            &f.log,
            &p.before_hash,
            OffsetDateTime::now_utc(),
        )
        .unwrap();
        assert_eq!(entry.target, format!("codex/plugin-scope:{ID}"));
        assert_eq!(
            std::fs::read_to_string(f.a.join(".codex/config.toml")).unwrap(),
            format!("[{SECTION}]\nenabled = true\n")
        );
        assert!(
            std::fs::read_to_string(f.b.join(".codex/config.toml"))
                .unwrap()
                .contains("enabled = true")
        );
        assert!(
            std::fs::read_to_string(&f.user)
                .unwrap()
                .contains(&format!("[{SECTION}]\nenabled = false"))
        );
        assert_eq!(log::read(&f.log), std::slice::from_ref(&entry));
        undo_record(f.home.path(), &f.log, &entry.id, OffsetDateTime::now_utc()).unwrap();
        assert_eq!(std::fs::read(&f.user).unwrap(), user);
        assert_eq!(
            std::fs::read_to_string(f.b.join(".codex/config.toml")).unwrap(),
            existing
        );
        assert!(!f.a.join(".codex").exists());
    }
}

#[test]
fn refuses_inline_dotted_tables_preview_hash_and_modified_undo_without_partial_writes() {
    let f = setup("refusals", true);
    for text in [
        "plugins = {}\n",
        "plugins.x.enabled = false\n",
        "[plugins]\nx.enabled = false\n",
    ] {
        std::fs::write(f.b.join(".codex/config.toml"), text).unwrap();
        assert!(preview(&plugin(&f)).is_err());
        assert!(!f.a.join(".codex").exists());
    }
    std::fs::write(f.b.join(".codex/config.toml"), B_OWN).unwrap();
    let p = preview(&plugin(&f)).unwrap();
    std::fs::write(f.b.join(".codex/config.toml"), "# changed\n").unwrap();
    assert!(
        apply(
            &plugin(&f),
            &f.log,
            &p.before_hash,
            OffsetDateTime::now_utc()
        )
        .is_err()
    );
    assert!(!f.a.join(".codex").exists());
    let p = preview(&plugin(&f)).unwrap();
    let entry = apply(
        &plugin(&f),
        &f.log,
        &p.before_hash,
        OffsetDateTime::now_utc(),
    )
    .unwrap();
    let user = std::fs::read(&f.user).unwrap();
    std::fs::write(f.b.join(".codex/config.toml"), "# edited\n").unwrap();
    assert!(undo_record(f.home.path(), &f.log, &entry.id, OffsetDateTime::now_utc()).is_err());
    assert_eq!(std::fs::read(&f.user).unwrap(), user);
}

#[cfg(unix)]
#[test]
fn refuses_symlinked_plugin_project_folder_and_config() {
    for directory in [true, false] {
        let f = setup(if directory { "dir-link" } else { "file-link" }, true);
        let outside = f.home.write("outside/config.toml", B_OWN);
        if directory {
            std::os::unix::fs::symlink(outside.parent().unwrap(), f.a.join(".codex")).unwrap();
        } else {
            let file = f.b.join(".codex/config.toml");
            std::fs::remove_file(&file).unwrap();
            std::os::unix::fs::symlink(&outside, file).unwrap();
        }
        assert!(preview(&plugin(&f)).is_err());
        assert_eq!(std::fs::read_to_string(outside).unwrap(), B_OWN);
    }
}

#[test]
fn ignores_old_calls_plain_mentions_and_other_skill_roots() {
    let f = setup("ignored", true);
    let path = f
        .home
        .path()
        .join(".codex/plugins/cache/openai-bundled/browser/1/skills/web/SKILL.md");
    let mut old = code_call("old", &format!("cat '{}'", path.display()));
    old["timestamp"] = json!("2020-01-01T00:00:00Z");
    rollout(
        &f.home,
        "irrelevant",
        &f.a,
        &[
            old,
            code_call("mention", &format!("echo '{}'", path.display())),
            code_call("other-root", "cat '/other/skills/web/SKILL.md'"),
            code_call("not-skill", &format!("cat '{}.backup'", path.display())),
        ],
    );
    assert_eq!(plugin(&f).usage.unwrap().total, 3);
}

#[test]
fn excludes_home_usage_and_skips_vanished_projects() {
    let f = setup("home", true);
    let mut used = plugin(&f).usage.unwrap();
    let trusted =
        crate::context::scan::codex_trusted_projects(&std::fs::read_to_string(&f.user).unwrap());
    std::fs::remove_dir_all(&f.b).unwrap();
    let moving = scope::codex::proposal(f.home.path(), true, &used, &trusted).unwrap();
    assert_eq!(moving.projects.len(), 1);
    assert_eq!(moving.skipped, ["b"]);
    used.projects[0].path = f.home.path().to_string_lossy().into_owned();
    assert!(scope::codex::proposal(f.home.path(), true, &used, &trusted).is_none());
}

#[test]
fn never_proposes_turning_off_a_codex_plugin_even_with_complete_zero_use_history() {
    let f = setup("no-off", true);
    let tuning = Tuning {
        diagnose_min_sessions: 0,
        ..Tuning::default()
    };
    let history = index(&f);
    // A configured plugin with no matching reads still receives usage evidence, never an off proposal.
    f.home.write(
        ".codex/config.toml",
        "[plugins.\"unused@m\"]\nenabled = true\n",
    );
    let measured = switches::find(
        switches::list(f.home.path(), None, Some(&history), &tuning),
        "codex/plugin:unused@m",
    )
    .unwrap();
    assert_eq!(measured.usage.unwrap().total, 0);
    assert!(!measured.is_proposed);
    assert!(measured.scope_move.is_none());
}
