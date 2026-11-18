//! 스킬 단위 숨기기(0022 "스킬 단위"): 사용 세기, 제안 기준, 쓰기·되돌리기. 임시 폴더를 HOME으로 쓴다(실제 설정 파일을 쓰지 않는다).

use super::*;
use crate::coach::usage;
use crate::coach::{log, undo as undo_record};
use crate::context::fsutil::testing::TempDir;
use crate::tuning::Tuning;
use serde_json::json;
use time::format_description::well_known::Rfc3339;

const USER: &str = "{\n  \"theme\": \"dark\"\n}\n";

struct Fixture {
    home: TempDir,
    project: PathBuf,
    log: PathBuf,
}

fn fixture(name: &str) -> Fixture {
    let home = TempDir::new(&format!("coach-skills-{name}"));
    home.write(".claude/skills/mine/SKILL.md", "---\nname: mine\n---\nx");
    home.write(".claude/skills/dup/SKILL.md", "x");
    home.write("work/app/.claude/skills/local/SKILL.md", "x");
    let project = home.path().join("work/app");
    let log = home.path().join("app-data").join(log::FILE_NAME);
    Fixture { home, project, log }
}

/// 세션 기록 한 개: 시작 폴더 `cwd`, user 줄의 `/명령`들, `Skill` 도구 호출들.
fn session(f: &Fixture, id: &str, cwd: &Path, commands: &[&str], skill_tool: &[&str]) {
    let now = OffsetDateTime::now_utc().format(&Rfc3339).unwrap();
    let mut text = format!("{}\n", json!({"type": "user", "cwd": cwd}));
    for (i, c) in commands.iter().enumerate() {
        let line = json!({"type": "user", "uuid": format!("{id}-c{i}"), "timestamp": now,
            "message": {"content": format!("<command-name>/{c}</command-name>")}});
        text += &format!("{line}\n");
    }
    for (i, s) in skill_tool.iter().enumerate() {
        let line = json!({"type": "assistant", "timestamp": now, "message": {"content": [
            {"type": "tool_use", "id": format!("{id}-t{i}"), "name": "Skill", "input": {"skill": s}}]}});
        text += &format!("{line}\n");
    }
    f.home
        .write(&format!(".claude/projects/p/{id}.jsonl"), &text);
}

fn index(f: &Fixture, min_sessions: usize) -> UsageIndex {
    let tuning = Tuning {
        diagnose_min_sessions: min_sessions,
        ..Tuning::default()
    };
    usage::read(
        f.home.path(),
        &tuning,
        crate::metrics::unix_secs(OffsetDateTime::now_utc()),
    )
}

fn user_settings(f: &Fixture) -> Option<Value> {
    fsutil::read_json(&paths::claude_root(f.home.path()).join("settings.json"))
}

fn skills(f: &Fixture, index: &UsageIndex) -> Vec<CoachSwitch> {
    list(
        f.home.path(),
        user_settings(f).as_ref(),
        Some(index),
        Some(&f.project),
    )
}

fn get(all: &[CoachSwitch], target: &str) -> CoachSwitch {
    all.iter().find(|s| s.target == target).unwrap().clone()
}

fn total(all: &[CoachSwitch], target: &str) -> u32 {
    get(all, target).usage.unwrap().total
}

fn apply_now(f: &Fixture, switch: &CoachSwitch, enabled: bool) -> Result<AppliedSwitch, AppError> {
    let hash = preview(switch, enabled)?.before_hash;
    apply(switch, &f.log, enabled, &hash, OffsetDateTime::now_utc())
}

fn undo_now(f: &Fixture, id: &str) -> Result<AppliedSwitch, AppError> {
    undo_record(f.home.path(), &f.log, id, OffsetDateTime::now_utc())
}

#[test]
fn bare_skill_name_is_counted_per_starting_folder_and_not_across_projects() {
    let f = fixture("count");
    let other = f.home.path().join("work/other");
    std::fs::create_dir_all(&other).unwrap();
    session(&f, "a", &f.project, &["mine", "local"], &["mine"]);
    session(&f, "b", &other, &["mine", "local"], &[]);
    let all = skills(&f, &index(&f, 0));
    // 개인 스킬은 모든 폴더 합계(명령 2 + Skill 도구 1), 프로젝트 스킬은 그 폴더에서 시작한 것만(다른 폴더에는 그 스킬이 없다).
    assert_eq!(total(&all, "claude/skill:mine"), 3);
    assert_eq!(total(&all, "claude/project-skill:local"), 1);
    assert_eq!(total(&all, "claude/skill:dup"), 0);
}

#[test]
fn skill_wins_over_a_plugin_skill_with_the_same_bare_name() {
    let f = fixture("ambiguity");
    f.home.write(
        ".claude/plugins/installed_plugins.json",
        r#"{"plugins": {"kit@m": [{}]}}"#,
    );
    f.home
        .write(".claude/plugins/cache/m/kit/1.0.0/skills/dup/SKILL.md", "x");
    f.home.write(
        ".claude/plugins/cache/m/kit/1.0.0/skills/only/SKILL.md",
        "x",
    );
    session(&f, "a", &f.project, &["dup", "only"], &["dup"]);
    let index = index(&f, 0);
    let all = skills(&f, &index);
    // Ruling: 같은 이름이면 개인·프로젝트 스킬이 이긴다(플러그인으로는 세지 않는다). `Skill` 도구의 접두사 없는 이름도 스킬.
    assert_eq!(total(&all, "claude/skill:dup"), 2);
    // 스킬과 안 겹치는 `only`는 예전처럼 플러그인 사용이다.
    assert_eq!(index.for_plugin("kit@m", &["kit"]).total, 1);
}

#[test]
fn proposes_only_with_enough_sessions_zero_uses_and_no_existing_override() {
    let f = fixture("proposal");
    for i in 0..3 {
        session(&f, &format!("s{i}"), &f.project, &[], &[]);
    }
    let proposed = |all: &[CoachSwitch], t: &str| get(all, t).is_proposed;
    // 세션 3개 < 기준 10: 0회여도 제안하지 않는다.
    let few = skills(&f, &index(&f, 10));
    assert!(!proposed(&few, "claude/skill:mine") && !proposed(&few, "claude/project-skill:local"));
    // 기준을 채우면 제안한다.
    let enough = skills(&f, &index(&f, 3));
    assert!(
        proposed(&enough, "claude/skill:mine") && proposed(&enough, "claude/project-skill:local")
    );
    // 다른 폴더 세션이 아무리 많아도 이 프로젝트 세션이 적으면 프로젝트 스킬은 제안하지 않는다.
    let other = f.home.path().join("work/other");
    for i in 0..5 {
        session(&f, &format!("o{i}"), &other, &[], &[]);
    }
    let skewed = skills(&f, &index(&f, 5));
    assert!(
        proposed(&skewed, "claude/skill:mine") && !proposed(&skewed, "claude/project-skill:local")
    );
    // 이미 사용자가 정한 값(on이 아닌 값)이면 어느 범위든 제안하지 않고 현재 값을 보인다.
    f.home.write(
        ".claude/settings.json",
        r#"{"skillOverrides": {"mine": "name-only"}}"#,
    );
    f.home.write(
        "work/app/.claude/settings.json",
        r#"{"skillOverrides": {"local": "off"}}"#,
    );
    let chosen = skills(&f, &index(&f, 3));
    for (target, state) in [
        ("claude/skill:mine", "name-only"),
        ("claude/project-skill:local", "off"),
    ] {
        let s = get(&chosen, target);
        assert!(!s.is_proposed && !s.enabled && s.format == SwitchFormat::Manual);
        assert_eq!(s.skill.unwrap().state.as_deref(), Some(state));
        assert!(s.file.is_none() && s.instruction.is_some());
    }
    // `"on"`은 값이 없는 것과 같다.
    f.home.write(
        ".claude/settings.json",
        r#"{"skillOverrides": {"mine": "on"}}"#,
    );
    assert!(proposed(&skills(&f, &index(&f, 3)), "claude/skill:mine"));
}

#[test]
fn personal_skill_hides_in_user_settings_and_undo_restores_exact_bytes() {
    let f = fixture("personal");
    let file = f.home.write(".claude/settings.json", USER);
    let mine = get(&skills(&f, &index(&f, 0)), "claude/skill:mine");
    let applied = apply_now(&f, &mine, false).unwrap();
    let written: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(written["skillOverrides"]["mine"], HIDDEN);
    assert_eq!(written["theme"], "dark");
    // 숨긴 뒤에는 "다시 보이기"(키 지움)만 가능하다.
    let hidden = get(&skills(&f, &index(&f, 0)), "claude/skill:mine");
    assert!(!hidden.enabled && hidden.format == SwitchFormat::Json && !hidden.is_proposed);
    undo_now(&f, &applied.id).unwrap();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), USER);
    // 같은 기록은 두 번 되돌리지 않는다.
    assert!(undo_now(&f, &applied.id).is_err());
}

#[test]
fn personal_skill_creates_user_settings_when_absent_and_undo_removes_it() {
    let f = fixture("personal-absent");
    let file = paths::claude_root(f.home.path()).join("settings.json");
    assert!(!file.exists());
    let mine = get(&skills(&f, &index(&f, 0)), "claude/skill:mine");
    let applied = apply_now(&f, &mine, false).unwrap();
    assert!(file.exists());
    undo_now(&f, &applied.id).unwrap();
    assert!(!file.exists());
}

#[test]
fn turning_back_on_removes_the_key_and_undo_puts_it_back() {
    let f = fixture("restore");
    let before = "{\n  \"skillOverrides\": {\n    \"mine\": \"user-invocable-only\"\n  },\n  \"theme\": \"dark\"\n}\n";
    let file = f.home.write(".claude/settings.json", before);
    let mine = get(&skills(&f, &index(&f, 0)), "claude/skill:mine");
    let applied = apply_now(&f, &mine, true).unwrap();
    let written: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert!(written["skillOverrides"].get("mine").is_none());
    undo_now(&f, &applied.id).unwrap();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
}

#[test]
fn project_skill_writes_local_settings_creating_and_undo_removes_the_file_again() {
    let f = fixture("project");
    let local = f.project.join(".claude/settings.local.json");
    assert!(!local.exists());
    let switch = get(&skills(&f, &index(&f, 0)), "claude/project-skill:local");
    assert_eq!(switch.file.as_deref(), local.to_str());
    let pv = preview(&switch, false).unwrap();
    assert!(pv.before.is_none() && pv.after.contains(HIDDEN));
    let applied = apply_now(&f, &switch, false).unwrap();
    let written: Value = serde_json::from_str(&std::fs::read_to_string(&local).unwrap()).unwrap();
    assert_eq!(written["skillOverrides"]["local"], HIDDEN);
    // 공유 파일(settings.json)은 만들거나 건드리지 않는다.
    assert!(!f.project.join(".claude/settings.json").exists());
    undo_now(&f, &applied.id).unwrap();
    // 없던 키는 없던 파일로 돌아간다. 프로젝트의 `.claude/skills`는 그대로다.
    assert!(!local.exists());
    assert!(f.project.join(".claude/skills/local/SKILL.md").exists());
}

#[test]
fn project_skill_edit_is_not_undone_after_the_file_changed() {
    let f = fixture("project-existing");
    let before = "{\n  \"permissions\": {\n    \"allow\": []\n  }\n}\n";
    let local = f.home.write("work/app/.claude/settings.local.json", before);
    let switch = get(&skills(&f, &index(&f, 0)), "claude/project-skill:local");
    let applied = apply_now(&f, &switch, false).unwrap();
    assert!(
        std::fs::read_to_string(&local)
            .unwrap()
            .contains("permissions")
    );
    f.home.write(
        "work/app/.claude/settings.local.json",
        "{\"edited\": true}\n",
    );
    assert!(undo_now(&f, &applied.id).is_err());
    assert!(std::fs::read_to_string(&local).unwrap().contains("edited"));
}

#[test]
fn rejects_stale_preview_and_does_not_list_plugin_or_renamed_skills() {
    let f = fixture("stale");
    f.home.write(
        ".claude/plugins/cache/m/kit/1.0.0/skills/plug/SKILL.md",
        "x",
    );
    let all = skills(&f, &index(&f, 0));
    assert!(all.iter().all(|s| !s.name.contains("plug")));
    let mine = get(&all, "claude/skill:mine");
    let hash = preview(&mine, false).unwrap().before_hash;
    f.home.write(".claude/settings.json", USER);
    assert!(apply(&mine, &f.log, false, &hash, OffsetDateTime::now_utc()).is_err());
    // 이름이 폴더와 다른 스킬은 어느 이름인지 몰라 목록에 넣지 않는다.
    f.home
        .write(".claude/skills/odd/SKILL.md", "---\nname: other\n---\nx");
    assert!(skills(&f, &index(&f, 0)).iter().all(|s| s.name != "odd"));
}
