//! 0018 작업 B 적용·되돌리기 테스트. 보관본은 Claude 답을 직접 판정해 넣는다(실행 없음). 홈은 모두 TempDir이다.

use time::macros::datetime;

use super::super::rules_handoff::tests::{Fixture, fixture, paths_answer, raw, skill_answer};
use super::super::rules_handoff::{RawRule, preview_of};
use super::super::{CheckFacts, list};
use super::*;
use crate::coach::{log, undo};
use crate::context::{ContextKind, Loading, instructions};

const NOW: OffsetDateTime = datetime!(2026-11-08 12:00:00.000 UTC);

fn log_file(f: &Fixture) -> PathBuf {
    log::file(f.data.path())
}

fn run(
    f: &Fixture,
    state: &HandoffState,
    key: &str,
    before_hash: &str,
) -> Result<AppliedSwitch, AppError> {
    let request = Request {
        id: &f.user_id(),
        key,
        before_hash,
    };
    apply(
        state,
        f.home.path(),
        &f.records(),
        &Tuning::default(),
        &log_file(f),
        request,
        NOW,
    )
}

/// 규칙 하나를 쓰고 그 규칙에 `answer`를 보관한다. (상태, 원본 해시, 규칙 경로)
fn one_rule(
    f: &Fixture,
    name: &str,
    text: &str,
    answer: RawRule,
) -> (HandoffState, String, PathBuf) {
    let file = f.home.write(&format!(".claude/rules/{name}"), text);
    let state = HandoffState::default();
    let target = f.store(&state, &[answer]);
    (state, target.rules[0].before_hash.clone(), file)
}

fn message(error: AppError) -> String {
    match error {
        AppError::InvalidInput(m) => m,
        AppError::Internal => "internal".into(),
    }
}

fn loading_of(f: &Fixture, file: &Path) -> Option<Loading> {
    let real = file.to_string_lossy().into_owned();
    instructions::claude(f.home.path(), Some(f.work.path()))
        .items
        .into_iter()
        .find(|i| i.kind == ContextKind::Rules && i.path.as_deref() == Some(real.as_str()))
        .map(|i| i.loading)
}

// ---------- paths ----------

#[test]
fn paths_become_new_front_matter_and_body_stays_byte_identical() {
    let f = fixture("paths-new");
    let original = "\u{feff}# API\n\n- Validate input.\n";
    let (state, hash, file) = one_rule(
        &f,
        "api.md",
        original,
        paths_answer("r1", &["src/api/**/*.ts", "say \"hi\""]),
    );
    let applied = run(&f, &state, "r1", &hash).unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert_eq!(
        text,
        format!("---\npaths:\n  - \"src/api/**/*.ts\"\n  - \"say \\\"hi\\\"\"\n---\n{original}")
    );
    assert_eq!(body_of(&text).as_bytes(), original.as_bytes());
    assert!(front_matter_key(&text, "paths").is_some());
    assert_eq!(loading_of(&f, &file), Some(Loading::Conditional));
    assert_eq!(applied.name, "api.md 필요할 때만 싣기");
    assert_eq!(applied.created, None);
    // 규칙이 필요할 때만 실리게 되어 카드가 사라진다. 되돌리기는 지금 경로 그대로.
    assert!(
        list(f.home.path(), &f.records(), &Tuning::default())
            .iter()
            .all(|c| c.id != f.user_id())
    );
    undo(f.home.path(), &log_file(&f), &applied.id, NOW).unwrap();
    assert_eq!(std::fs::read(&file).unwrap(), original.as_bytes());
}

#[test]
fn paths_go_inside_existing_front_matter() {
    let f = fixture("paths-inside");
    let original = "---\nname: api\n---\nbody\n";
    let (state, hash, file) = one_rule(&f, "api.md", original, paths_answer("r1", &["a/**"]));
    run(&f, &state, "r1", &hash).unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert_eq!(text, "---\npaths:\n  - \"a/**\"\nname: api\n---\nbody\n");
    assert_eq!(body_of(&text), body_of(original));
    assert_eq!(loading_of(&f, &file), Some(Loading::Conditional));
}

#[test]
fn front_matter_with_crlf_or_existing_paths_line() {
    let crlf = with_paths("---\r\nname: a\r\n---\r\nbody", &["x/**".into()]).unwrap();
    assert_eq!(
        crlf,
        "---\r\npaths:\r\n  - \"x/**\"\r\nname: a\r\n---\r\nbody"
    );
    assert!(front_matter_key(&crlf, "paths").is_some());
    let error = with_paths("---\nname: a\npaths:\n---\nbody", &["x/**".into()]).unwrap_err();
    assert!(message(error).contains("paths 줄"));
}

#[test]
fn rule_over_40000_chars_cannot_get_paths() {
    let f = fixture("paths-long");
    let original = "x".repeat(MAX_CHARS + 1);
    let (state, hash, file) = one_rule(&f, "big.md", &original, paths_answer("r1", &["a/**"]));
    let preview = preview_of(&f.target(), &stored(&state, &f.user_id()).unwrap().unwrap());
    assert!(!preview.rules[0].can_apply);
    assert!(message(run(&f, &state, "r1", &hash).unwrap_err()).contains("40,000자"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), original);
}

#[test]
fn apply_refuses_changed_file_keep_and_unknown_key() {
    let f = fixture("refuse");
    let (state, hash, file) = one_rule(&f, "a.md", "A\n", paths_answer("r1", &["a/**"]));
    assert!(message(run(&f, &state, "r2", &hash).unwrap_err()).contains("없는 규칙"));
    assert!(run(&f, &state, "../x", &hash).is_err());
    std::fs::write(&file, "A changed\n").unwrap();
    assert_eq!(
        message(run(&f, &state, "r1", &hash).unwrap_err()),
        "Claude에게 맡긴 뒤 파일이 바뀌었어요. 다시 맡겨 주세요."
    );
    let (state, hash, _) = one_rule(&f, "a.md", "A\n", raw("r1", "keep"));
    assert!(message(run(&f, &state, "r1", &hash).unwrap_err()).contains("그대로 두기"));
}

#[test]
fn other_rules_use_stored_proposals_after_one_is_applied() {
    let f = fixture("partial");
    let a = f.home.write(".claude/rules/a.md", "A\n");
    let b = f.home.write(".claude/rules/b.md", "B\n");
    let state = HandoffState::default();
    let target = f.store(
        &state,
        &[
            paths_answer("r1", &["a/**"]),
            skill_answer("r2", "bee", "Use for B."),
        ],
    );
    let hashes: Vec<String> = target.rules.iter().map(|r| r.before_hash.clone()).collect();
    run(&f, &state, "r1", &hashes[0]).unwrap();
    // 카드에는 b.md만 남고, 보관본은 그대로 쓴다(열쇠는 맡긴 때 그대로 r2).
    let left = f.target();
    assert_eq!(left.rules.len(), 1);
    assert!(
        super::super::rules_handoff::reusable(&state, &left)
            .unwrap()
            .is_some()
    );
    run(&f, &state, "r2", &hashes[1]).unwrap();
    assert!(front_matter_key(&std::fs::read_to_string(&a).unwrap(), "paths").is_some());
    assert!(!b.exists());
}

// ---------- 스킬로 옮기기 ----------

fn skill_md(f: &Fixture, name: &str) -> PathBuf {
    f.home
        .path()
        .join(".claude/skills")
        .join(name)
        .join("SKILL.md")
}

#[test]
fn skill_move_creates_skill_md_and_undo_restores_rule() {
    let f = fixture("skill");
    let original = "# Release\n\nBump the version.\n";
    let (state, hash, file) = one_rule(
        &f,
        "release.md",
        original,
        skill_answer("r1", "release", "Use when \"releasing\"."),
    );
    let applied = run(&f, &state, "r1", &hash).unwrap();
    let created = skill_md(&f, "release");
    assert_eq!(
        std::fs::read_to_string(&created).unwrap(),
        format!(
            "---\nname: release\ndescription: \"Use when \\\"releasing\\\".\"\n---\n{original}"
        )
    );
    assert!(!file.exists());
    assert_eq!(std::fs::read_to_string(&applied.backup).unwrap(), original);
    assert_eq!(
        applied.created.as_deref(),
        Some(created.to_string_lossy().as_ref())
    );
    assert_eq!(applied.name, "release.md 스킬로 옮기기");
    assert!(applied.undo_command.starts_with("mv ") || cfg!(windows));
    let logged = log::read(&log_file(&f));
    assert_eq!(logged[0].created, applied.created);
    let undone = undo(f.home.path(), &log_file(&f), &applied.id, NOW).unwrap();
    assert_eq!(undone.undo_of.as_deref(), Some(applied.id.as_str()));
    assert_eq!(std::fs::read(&file).unwrap(), original.as_bytes());
    assert!(!created.exists());
    assert!(!created.parent().unwrap().exists());
    assert!(!Path::new(&applied.backup).exists());
}

#[test]
fn skill_move_refuses_existing_name_or_front_matter() {
    let f = fixture("skill-taken");
    f.home.write(".claude/skills/release/notes.txt", "mine");
    let (state, hash, file) = one_rule(&f, "release.md", "R\n", skill_answer("r1", "release", "x"));
    let preview = preview_of(&f.target(), &stored(&state, &f.user_id()).unwrap().unwrap());
    assert!(preview.rules[0].skill_exists && !preview.rules[0].can_apply);
    assert!(message(run(&f, &state, "r1", &hash).unwrap_err()).contains("같은 이름의 스킬"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "R\n");
    assert!(!skill_md(&f, "release").exists());
    let f = fixture("skill-front");
    let (state, hash, _) = one_rule(
        &f,
        "a.md",
        "---\nname: a\n---\nA\n",
        skill_answer("r1", "aaa", "x"),
    );
    assert!(message(run(&f, &state, "r1", &hash).unwrap_err()).contains("머리말이 있는 규칙"));
    assert!(!skill_md(&f, "aaa").exists());
}

#[cfg(unix)]
#[test]
fn skill_move_cleans_up_when_rule_cannot_be_moved() {
    use std::os::unix::fs::PermissionsExt;
    let f = fixture("skill-rename");
    let (state, hash, file) = one_rule(&f, "release.md", "R\n", skill_answer("r1", "release", "x"));
    let rules = f.user_root();
    std::fs::set_permissions(&rules, std::fs::Permissions::from_mode(0o555)).unwrap();
    let result = run(&f, &state, "r1", &hash);
    std::fs::set_permissions(&rules, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(message(result.unwrap_err()).contains("규칙 파일 옮기기"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "R\n");
    assert!(!skill_md(&f, "release").parent().unwrap().exists());
    assert!(log::read(&log_file(&f)).is_empty());
}

// ---------- 스킬 되돌리기 거절 ----------

fn moved(f: &Fixture) -> (AppliedSwitch, PathBuf) {
    let (state, hash, file) = one_rule(f, "release.md", "R\n", skill_answer("r1", "release", "x"));
    (run(f, &state, "r1", &hash).unwrap(), file)
}

#[test]
fn skill_undo_refuses_when_skill_md_changed() {
    let f = fixture("undo-changed");
    let (applied, file) = moved(&f);
    std::fs::write(skill_md(&f, "release"), "edited").unwrap();
    let error = message(undo(f.home.path(), &log_file(&f), &applied.id, NOW).unwrap_err());
    assert!(
        error.contains("스킬 파일이 바뀌어") && error.contains(&applied.undo_command),
        "{error}"
    );
    assert!(!file.exists());
}

#[test]
fn skill_undo_refuses_when_rule_slot_is_taken() {
    let f = fixture("undo-taken");
    let (applied, file) = moved(&f);
    std::fs::write(&file, "new rule").unwrap();
    let error = message(undo(f.home.path(), &log_file(&f), &applied.id, NOW).unwrap_err());
    assert!(error.contains("규칙 자리"), "{error}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "new rule");
    assert!(skill_md(&f, "release").exists());
}

#[test]
fn skill_undo_refuses_created_outside_the_scope_skills_folder() {
    let f = fixture("undo-shape");
    let (applied, file) = moved(&f);
    let elsewhere = f.home.write(
        "elsewhere/release/SKILL.md",
        &std::fs::read_to_string(skill_md(&f, "release")).unwrap(),
    );
    let tampered = AppliedSwitch {
        id: "1".into(),
        created: Some(elsewhere.to_string_lossy().into_owned()),
        ..applied
    };
    log::append(&log_file(&f), &tampered).unwrap();
    let error = message(undo(f.home.path(), &log_file(&f), "1", NOW).unwrap_err());
    assert!(error.contains("kcal이 만든 것이 아니라"), "{error}");
    assert!(elsewhere.exists() && !file.exists());
    let rule = f.user_root().join("a.md");
    let skills = f.home.path().join(".claude/skills");
    assert!(is_skill_of(&rule, &skills.join("ok/SKILL.md")));
    assert!(!is_skill_of(&rule, &skills.join("a/b/SKILL.md")));
    assert!(!is_skill_of(&rule, &skills.join("Bad_Name/SKILL.md")));
    assert!(!is_skill_of(&rule, &skills.join("ok/other.md")));
    assert!(!is_skill_of(&rule, &skills.join("../ok/SKILL.md")));
}

// ---------- 기록 ----------

#[test]
fn old_log_line_without_created_still_reads() {
    let f = fixture("old-log");
    let line = r#"{"id":"1","at":"2026-11-05T12:00:00Z","action":"apply","target":"codex/plugin:x@m","agent":"codex","name":"x@m","file":"/s/config.toml","backup":"/s/config.toml.kcal-1.bak","enabled":false,"tokens":null,"afterHash":"0","undoCommand":"cp","undoOf":null}"#;
    let path = log_file(&f);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, format!("{line}\n")).unwrap();
    let entries = log::read(&path);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].created, None);
    // 다시 쓸 때도 `created`가 없으면 칸을 넣지 않는다.
    assert!(
        !serde_json::to_string(&entries[0])
            .unwrap()
            .contains("created")
    );
}

#[test]
fn rules_card_facts_survive_round_trip_through_list() {
    let f = fixture("facts");
    f.home
        .write(".claude/rules/a.md", "---\nname: a\n---\nAAAA\n");
    let card = list(f.home.path(), &f.records(), &Tuning::default())
        .into_iter()
        .find(|c| c.id == f.user_id())
        .unwrap();
    let CheckFacts::RulesPaths { rules, limit, .. } = card.facts else {
        panic!()
    };
    assert_eq!(limit, MAX_CHARS);
    assert_eq!((rules[0].chars, rules[0].has_front_matter), (5, true));
}
