//! `tests.rs`의 C(git 지침)·B(AGENTS.md 훅·같은 스킬) 점검 테스트.

use super::*;

fn git_check(home: &TempDir) -> Option<CoachCheck> {
    find_check(&checks(home, &records(&[])), "C-claude-git-instructions").cloned()
}

fn git_facts(check: &CoachCheck) -> (Vec<(String, usize)>, bool, Vec<String>) {
    let CheckFacts::GitInstructions {
        matches,
        has_own_trailer,
        pre_commit_skills,
    } = &check.facts
    else {
        panic!()
    };
    let lines = matches
        .iter()
        .map(|m| {
            (
                Path::new(&m.file)
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                m.line,
            )
        })
        .collect();
    (lines, *has_own_trailer, pre_commit_skills.clone())
}

#[test]
fn git_rules_in_user_instructions_are_caught_and_policy_sentences_are_not() {
    let home = home_with("git", "{}\n");
    home.write(
        ".claude/CLAUDE.md",
        "커밋·푸시·배포는 사람이 요청했을 때만 한다.\n",
    );
    assert!(git_check(&home).is_none());
    home.write(".claude/rules/common/git-workflow.md", GIT_RULE);
    let check = git_check(&home).unwrap();
    assert_eq!(
        git_facts(&check),
        (vec![("git-workflow.md".into(), 3)], false, vec![])
    );
    // 서명 줄이 빠지므로 그대로 두기 추천.
    assert_eq!(
        option_ids(&check),
        [
            ("disable-git-instructions", false, true),
            ("keep", true, false)
        ]
    );
    let after = round_trip(
        &home,
        &records(&[]),
        "C-claude-git-instructions",
        "disable-git-instructions",
    );
    assert_eq!(after, serde_json::json!({"includeGitInstructions": false}));
}

#[test]
fn git_card_recommends_disabling_only_with_own_trailer_and_no_pre_commit_skill() {
    let home = home_with("git-trailer", "{}\n");
    home.write(
        ".claude/rules/git.md",
        &format!("{GIT_RULE}\nCo-Authored-By: Me <me@example.com>\n"),
    );
    let check = git_check(&home).unwrap();
    assert!(git_facts(&check).1);
    assert_eq!(
        option_ids(&check)[0],
        ("disable-git-instructions", true, true)
    );
    home.write(
        ".claude/skills/verify/SKILL.md",
        "---\nname: verify\n---\nrun tests",
    );
    home.write(
        ".claude/commands/simplify.md",
        "---\ndisable-model-invocation: true\n---\nx",
    );
    let check = git_check(&home).unwrap();
    assert_eq!(git_facts(&check).2, ["verify"]);
    assert_eq!(option_ids(&check)[1], ("keep", true, false));
}

#[test]
fn git_card_is_not_shown_when_built_in_instructions_are_already_off() {
    for settings in [
        "{\"includeGitInstructions\": false}",
        "{\"env\": {\"CLAUDE_CODE_DISABLE_GIT_INSTRUCTIONS\": \"1\"}}",
    ] {
        let home = home_with("git-off", settings);
        home.write(".claude/rules/git.md", GIT_RULE);
        assert!(git_check(&home).is_none(), "{settings}");
    }
}

// ---------- B ----------

const HOOK_SETTINGS: &str = "{\n  \"hooks\": {\n    \"SessionStart\": [\n      {\n        \"matcher\": \"startup\",\n        \"hooks\": [\n          {\n            \"type\": \"command\",\n            \"command\": \"cat AGENTS.md\"\n          }\n        ]\n      }\n    ],\n    \"Stop\": []\n  }\n}\n";

fn hook_records(workspace: &TempDir) -> RecentRecords {
    RecentRecords {
        workspace: Some(workspace.path().to_path_buf()),
        ..records(&[])
    }
}

#[test]
fn agents_md_hook_in_user_settings_is_removed_with_its_empty_group_and_array() {
    let home = home_with("hook", HOOK_SETTINGS);
    let work = TempDir::new("checks-hook-work");
    work.write("AGENTS.md", &"x".repeat(400));
    let recent = hook_records(&work);
    let all = checks(&home, &recent);
    let check = find_check(&all, "B-claude-agents-md-hook:0").unwrap();
    assert_eq!(check.tokens, Some(100));
    assert_eq!(option_ids(check), [("remove-hook", true, true)]);
    assert_eq!(check.options[0].changes[0].location, "hooks.SessionStart");
    let after = round_trip(&home, &recent, "B-claude-agents-md-hook:0", "remove-hook");
    assert_eq!(after, serde_json::json!({"hooks": {"Stop": []}}));
}

#[test]
fn agents_md_hook_in_project_settings_is_instruction_only() {
    let home = home_with("hook-project", "{}\n");
    let work = TempDir::new("checks-hook-project-work");
    work.write("AGENTS.md", "rules");
    work.write(".claude/settings.json", HOOK_SETTINGS);
    let recent = hook_records(&work);
    let all = checks(&home, &recent);
    let check = find_check(&all, "B-claude-agents-md-hook:0").unwrap();
    let CheckFacts::AgentsMdHook { scope, .. } = &check.facts else {
        panic!()
    };
    assert_eq!(*scope, SettingsScope::Project);
    assert_eq!(option_ids(check), [("remove-hook", true, false)]);
    assert!(
        check.options[0]
            .instruction
            .as_deref()
            .unwrap()
            .contains("cat AGENTS.md")
    );
    let log_file = home.path().join("log.jsonl");
    let error = apply(
        home.path(),
        &recent,
        &Tuning::default(),
        &log_file,
        &check.id,
        "remove-hook",
        "cbf29ce484222325",
        NOW,
    );
    assert!(message(error.unwrap_err()).contains("지우세요"));
}

#[test]
fn agents_md_hook_is_not_flagged_where_claude_does_not_load_agents_md() {
    let home = home_with("hook-claude-md", HOOK_SETTINGS);
    let work = TempDir::new("checks-hook-claude-md-work");
    work.write("AGENTS.md", "rules");
    work.write("CLAUDE.md", "my own rules");
    assert!(
        checks(&home, &hook_records(&work))
            .iter()
            .all(|c| !c.id.starts_with("B-claude-agents-md-hook"))
    );
    // CLAUDE.md가 `@AGENTS.md`로 가져오면 실린다.
    work.write("CLAUDE.md", "@AGENTS.md\n");
    assert!(
        find_check(
            &checks(&home, &hook_records(&work)),
            "B-claude-agents-md-hook:0"
        )
        .is_some()
    );
    assert!(
        checks(&home, &records(&[]))
            .iter()
            .all(|c| !c.id.starts_with("B-"))
    );
}

const SKILL: &str = "---\nname: foo\ndescription: does foo\n---\nbody\n";

fn skill_home(name: &str, plugin_skill: &str, settings: &str) -> TempDir {
    let home = home_with(name, settings);
    home.write(".claude/skills/foo/SKILL.md", SKILL);
    home.write(
        ".claude/plugins/cache/m/p/1.0.0/skills/foo/SKILL.md",
        plugin_skill,
    );
    home
}

const PLUGIN_ON: &str = "{\n  \"enabledPlugins\": {\n    \"p@m\": true\n  }\n}\n";

#[test]
fn same_skill_with_same_content_recommends_hiding_the_personal_copy() {
    let home = skill_home("skill-same", &SKILL.replace('\n', "\r\n"), PLUGIN_ON);
    let all = checks(&home, &records(&[]));
    let check = find_check(&all, "B-claude-same-skill:foo").unwrap();
    let CheckFacts::SameSkill {
        is_same_content,
        plugin_id,
        ..
    } = &check.facts
    else {
        panic!()
    };
    assert!(*is_same_content && plugin_id == "p@m");
    assert_eq!(
        check.tokens,
        Some(fsutil::estimate(
            SKILL.find("\n---\n").unwrap() as u64 + 5,
            4
        ))
    );
    assert_eq!(
        option_ids(check),
        [("hide-personal", true, true), ("keep", false, false)]
    );
    let after = round_trip(
        &home,
        &records(&[]),
        "B-claude-same-skill:foo",
        "hide-personal",
    );
    assert_eq!(after["skillOverrides"]["foo"], "user-invocable-only");
    assert_eq!(after["enabledPlugins"]["p@m"], true);
}

#[test]
fn same_skill_with_different_content_recommends_keep_and_existing_override_hides_card() {
    let home = skill_home("skill-diff", "---\nname: foo\n---\nother\n", PLUGIN_ON);
    let all = checks(&home, &records(&[]));
    assert_eq!(
        option_ids(find_check(&all, "B-claude-same-skill:foo").unwrap())[1],
        ("keep", true, false)
    );
    let decided = skill_home(
        "skill-decided",
        SKILL,
        "{\"enabledPlugins\": {\"p@m\": true}, \"skillOverrides\": {\"foo\": \"name-only\"}}",
    );
    assert!(find_check(&checks(&decided, &records(&[])), "B-claude-same-skill:foo").is_none());
    let on = skill_home(
        "skill-on",
        SKILL,
        "{\"enabledPlugins\": {\"p@m\": true}, \"skillOverrides\": {\"foo\": \"on\"}}",
    );
    assert!(find_check(&checks(&on, &records(&[])), "B-claude-same-skill:foo").is_some());
    let off = skill_home(
        "skill-plugin-off",
        SKILL,
        "{\"enabledPlugins\": {\"p@m\": false}}",
    );
    assert!(find_check(&checks(&off, &records(&[])), "B-claude-same-skill:foo").is_none());
}
