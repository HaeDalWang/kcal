//! "프로젝트에서만 켜기" 적용·되돌리기. 임시 폴더를 HOME으로 쓴다(실제 설정 파일을 쓰지 않는다).

use super::*;
use crate::coach::usage::{self, UsageIndex};
use crate::coach::{AppliedSwitch as Entry, log, undo as undo_record};
use crate::context::fsutil::testing::TempDir;
use crate::tuning::Tuning;
use serde_json::json;
use time::format_description::well_known::Rfc3339;

const USER: &str = "{\n  \"enabledPlugins\": {\n    \"kit@m\": true,\n    \"other@m\": true\n  },\n  \"theme\": \"dark\"\n}\n";
const B_LOCAL: &str = "{\n  \"permissions\": {\n    \"allow\": []\n  }\n}\n";
const TARGET: &str = "claude/plugin:kit@m";

struct Fixture {
    home: TempDir,
    user: PathBuf,
    a: PathBuf,
    b_local: PathBuf,
    log: PathBuf,
}

fn session(home: &TempDir, project: &Path, name: &str, tool: &str) {
    let now = OffsetDateTime::now_utc().format(&Rfc3339).unwrap();
    let line = json!({"type": "assistant", "cwd": project, "timestamp": now,
        "message": {"content": [{"type": "tool_use", "id": format!("{name}-1"), "name": tool, "input": {}}]}});
    home.write(
        &format!(".claude/projects/{name}/s.jsonl"),
        &format!("{line}\n"),
    );
}

/// kit을 A·B·C(없어진 폴더)에서 썼다. B에는 local 설정이 이미 있다.
fn fixture(name: &str) -> Fixture {
    let home = TempDir::new(&format!("coach-scope-{name}"));
    let user = home.write(".claude/settings.json", USER);
    let a = home.path().join("work/a");
    std::fs::create_dir_all(&a).unwrap();
    let b_local = home.write("work/b/.claude/settings.local.json", B_LOCAL);
    let b = home.path().join("work/b");
    let c = home.path().join("work/c-gone");
    for (project, dir) in [(&a, "a"), (&b, "b"), (&c, "c")] {
        session(&home, project, dir, "mcp__plugin_kit_srv__go");
    }
    let log = home.path().join("app-data").join(log::FILE_NAME);
    Fixture {
        home,
        user,
        a,
        b_local,
        log,
    }
}

fn index(f: &Fixture) -> UsageIndex {
    let now = crate::metrics::unix_secs(OffsetDateTime::now_utc());
    // 세션 수 기준은 끈다(이 파일의 세션은 3개뿐). 기준 자체는 `needs_enough_sessions_and_uses_to_propose`가 본다.
    let tuning = Tuning {
        diagnose_min_sessions: 0,
        ..Tuning::default()
    };
    usage::read(f.home.path(), &tuning, now)
}

fn kit(f: &Fixture) -> CoachSwitch {
    let all = switches::list(f.home.path(), None, Some(&index(f)), &Tuning::default());
    switches::find(all, TARGET).unwrap()
}

fn apply_now(f: &Fixture, hash: &str) -> Result<Entry, AppError> {
    apply(&kit(f), &f.log, hash, OffsetDateTime::now_utc())
}

fn message(error: AppError) -> String {
    match error {
        AppError::InvalidInput(m) => m,
        AppError::Internal => "internal".into(),
    }
}

#[test]
fn proposes_moving_a_user_plugin_used_in_up_to_three_projects_and_skips_gone_folders() {
    let f = fixture("propose");
    let switch = kit(&f);
    let moving = switch.scope_move.as_ref().unwrap();
    assert_eq!(
        moving
            .projects
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
    assert_eq!(moving.skipped, ["c-gone"]);
    assert_eq!(switch.usage.as_ref().map(|u| u.total), Some(3));
    assert!(!switch.is_proposed);
    assert!(switch.scope.as_ref().unwrap().user);
    // 쓰지 않은 플러그인은 옮길 곳이 없다.
    let all = switches::list(f.home.path(), None, Some(&index(&f)), &Tuning::default());
    let other = switches::find(all, "claude/plugin:other@m").unwrap();
    assert_eq!(
        (other.usage.map(|u| u.total), other.scope_move),
        (Some(0), None)
    );
}

#[test]
fn does_not_propose_when_used_in_more_projects_or_from_home() {
    let f = fixture("many");
    session(
        &f.home,
        &f.home.path().join("work/d"),
        "d",
        "mcp__plugin_kit_srv__go",
    );
    std::fs::create_dir_all(f.home.path().join("work/d")).unwrap();
    session(
        &f.home,
        &f.home.path().join("work/e"),
        "e",
        "mcp__plugin_kit_srv__go",
    );
    assert_eq!(kit(&f).scope_move, None);
    let g = fixture("home");
    session(&g.home, g.home.path(), "home", "mcp__plugin_kit_srv__go");
    assert_eq!(kit(&g).scope_move, None);
}

#[test]
fn needs_enough_sessions_and_uses_to_propose() {
    let f = fixture("thresholds");
    let used = kit(&f).usage.unwrap();
    assert_eq!((used.total, used.scope_min_uses), (3, 3));
    let home = f.home.path();
    assert!(proposal(home, true, &used).is_some());
    // 2회는 자주 쓰는지 알 수 없다(기본 3회 미만).
    let few = SwitchUsage {
        total: 2,
        ..used.clone()
    };
    assert_eq!(proposal(home, true, &few), None);
    // 1회: 플러그인 스킬 1번 쓴 것만으로 제안하던 사건.
    let once = SwitchUsage {
        total: 1,
        projects: used.projects[..1].to_vec(),
        project_count: 1,
        ..used.clone()
    };
    assert_eq!(proposal(home, true, &once), None);
    // 기준을 낮추면 1회도 제안한다.
    let lowered = SwitchUsage {
        scope_min_uses: 1,
        ..once
    };
    assert!(proposal(home, true, &lowered).is_some());
    // 세션이 기준 미만이면 횟수가 충분해도 제안하지 않는다.
    let thin = SwitchUsage {
        sessions: 2,
        min_sessions: 10,
        ..used
    };
    assert_eq!(proposal(home, true, &thin), None);
}

#[test]
fn apply_writes_all_files_once_and_undo_restores_every_byte() {
    let f = fixture("apply");
    let p = preview(&kit(&f)).unwrap();
    assert_eq!(
        p.changes
            .iter()
            .map(|c| (c.is_new_file, c.after.as_str()))
            .collect::<Vec<_>>(),
        [(true, "true"), (false, "true"), (false, "\"kit@m\": false")]
    );
    assert_eq!(p.skipped, ["c-gone"]);
    // 미리보기는 쓰지 않는다.
    assert!(!f.a.join(".claude").exists());

    let applied = apply_now(&f, &p.before_hash).unwrap();
    let a_local = f.a.join(".claude/settings.local.json");
    // Claude Code `plugin enable --scope local`이 만드는 모양과 같다(evidence 0단계 c).
    assert_eq!(
        std::fs::read_to_string(&a_local).unwrap(),
        "{\n  \"enabledPlugins\": {\n    \"kit@m\": true\n  }\n}\n"
    );
    let b: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&f.b_local).unwrap()).unwrap();
    assert_eq!(b["enabledPlugins"]["kit@m"], true);
    assert_eq!(b["permissions"], json!({"allow": []}));
    assert_eq!(
        std::fs::read_to_string(&f.user).unwrap(),
        USER.replace("\"kit@m\": true", "\"kit@m\": false")
    );
    assert!(applied.target.starts_with(SCOPE_TARGET_PREFIX));
    assert_eq!(applied.files.len(), 2);
    let (rm, cp) = if cfg!(windows) {
        ("del ", "copy ")
    } else {
        ("rm ", "cp ")
    };
    assert!(applied.undo_command.contains(rm) && applied.undo_command.contains(cp));
    assert_eq!(log::read(&f.log), std::slice::from_ref(&applied));
    // 옮긴 뒤: 사용자 범위는 꺼지고, 그 프로젝트들에서 켜져 있다.
    let after = kit(&f);
    let scope = after.scope.unwrap();
    assert!(!scope.user && after.scope_move.is_none());
    assert!(
        scope
            .projects
            .iter()
            .all(|p| p.enabled && p.file == ScopeFile::Local)
    );

    let undone = undo_record(
        f.home.path(),
        &f.log,
        &applied.id,
        OffsetDateTime::now_utc(),
    )
    .unwrap();
    assert_eq!(undone.undo_of.as_deref(), Some(applied.id.as_str()));
    assert_eq!(std::fs::read_to_string(&f.user).unwrap(), USER);
    assert_eq!(std::fs::read_to_string(&f.b_local).unwrap(), B_LOCAL);
    assert!(!a_local.exists() && !f.a.join(".claude").exists());
    assert!(
        message(
            undo_record(
                f.home.path(),
                &f.log,
                &applied.id,
                OffsetDateTime::now_utc()
            )
            .unwrap_err()
        )
        .contains("이미")
    );
}

#[test]
fn apply_refuses_when_any_file_changed_after_preview() {
    let f = fixture("stale");
    let p = preview(&kit(&f)).unwrap();
    std::fs::write(&f.b_local, "{}\n").unwrap();
    assert!(message(apply_now(&f, &p.before_hash).unwrap_err()).contains("바뀌었어요"));
    assert_eq!(std::fs::read_to_string(&f.user).unwrap(), USER);
    assert!(!f.a.join(".claude").exists());
    assert!(log::read(&f.log).is_empty());
}

#[test]
fn apply_refuses_and_writes_nothing_when_a_project_file_is_not_plain_json() {
    let f = fixture("jsonc");
    std::fs::write(&f.b_local, "{ // note\n}\n").unwrap();
    assert!(preview(&kit(&f)).is_err());
    assert_eq!(std::fs::read_to_string(&f.user).unwrap(), USER);
    assert!(!f.a.join(".claude").exists());
}

#[test]
fn undo_refuses_when_a_project_file_changed_after_apply() {
    let f = fixture("undo-stale");
    let p = preview(&kit(&f)).unwrap();
    let applied = apply_now(&f, &p.before_hash).unwrap();
    let edited = "{\n  \"enabledPlugins\": {}\n}\n";
    std::fs::write(&f.b_local, edited).unwrap();
    let error = message(
        undo_record(
            f.home.path(),
            &f.log,
            &applied.id,
            OffsetDateTime::now_utc(),
        )
        .unwrap_err(),
    );
    assert!(error.contains(&applied.undo_command));
    assert_eq!(std::fs::read_to_string(&f.b_local).unwrap(), edited);
    assert_eq!(
        std::fs::read_to_string(&f.user).unwrap(),
        USER.replace("\"kit@m\": true", "\"kit@m\": false")
    );
}

#[test]
fn undo_refuses_records_pointing_outside_project_settings() {
    let f = fixture("undo-path");
    let p = preview(&kit(&f)).unwrap();
    let applied = apply_now(&f, &p.before_hash).unwrap();
    let mut tampered = applied.clone();
    tampered.id = "1".into();
    tampered.files[0].file = f
        .home
        .path()
        .join("work/a/notes.txt")
        .to_string_lossy()
        .into_owned();
    log::append(&f.log, &tampered).unwrap();
    assert!(
        message(undo_record(f.home.path(), &f.log, "1", OffsetDateTime::now_utc()).unwrap_err())
            .contains("경로")
    );
    assert!(f.a.join(".claude/settings.local.json").exists());
}

#[cfg(unix)]
#[test]
fn apply_refuses_when_project_claude_dir_links_outside() {
    let f = fixture("link-dir");
    let outside = f.home.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, f.a.join(".claude")).unwrap();
    let error = message(apply_now(&f, "0000000000000000").unwrap_err());
    assert!(error.contains("링크"), "{error}");
    assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
    assert_eq!(std::fs::read_to_string(&f.user).unwrap(), USER);
    assert!(log::read(&f.log).is_empty());
}

#[cfg(unix)]
#[test]
fn apply_refuses_when_project_settings_file_links_outside() {
    let f = fixture("link-file");
    let outside = f.home.write("outside/victim.json", B_LOCAL);
    std::fs::remove_file(&f.b_local).unwrap();
    std::os::unix::fs::symlink(&outside, &f.b_local).unwrap();
    let error = message(apply_now(&f, "0000000000000000").unwrap_err());
    assert!(error.contains("링크"), "{error}");
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), B_LOCAL);
    assert_eq!(std::fs::read_to_string(&f.user).unwrap(), USER);
    assert!(!f.a.join(".claude").exists());
}

#[cfg(unix)]
#[test]
fn undo_refuses_when_a_project_file_became_a_link_after_apply() {
    let f = fixture("undo-link");
    let p = preview(&kit(&f)).unwrap();
    let applied = apply_now(&f, &p.before_hash).unwrap();
    let outside = f.home.write("outside/victim.json", "{}\n");
    std::fs::remove_file(&f.b_local).unwrap();
    std::os::unix::fs::symlink(&outside, &f.b_local).unwrap();
    assert!(
        undo_record(
            f.home.path(),
            &f.log,
            &applied.id,
            OffsetDateTime::now_utc()
        )
        .is_err()
    );
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "{}\n");
}

#[cfg(unix)]
#[test]
fn apply_and_undo_still_follow_a_linked_user_settings_file() {
    let f = fixture("link-user");
    let dotfiles = f.home.write("dotfiles/settings.json", USER);
    std::fs::remove_file(&f.user).unwrap();
    std::os::unix::fs::symlink(&dotfiles, &f.user).unwrap();
    let p = preview(&kit(&f)).unwrap();
    let applied = apply_now(&f, &p.before_hash).unwrap();
    assert!(std::fs::symlink_metadata(&f.user).unwrap().is_symlink());
    assert_eq!(
        std::fs::read_to_string(&dotfiles).unwrap(),
        USER.replace("\"kit@m\": true", "\"kit@m\": false")
    );
    undo_record(
        f.home.path(),
        &f.log,
        &applied.id,
        OffsetDateTime::now_utc(),
    )
    .unwrap();
    assert_eq!(std::fs::read_to_string(&dotfiles).unwrap(), USER);
}

#[cfg(unix)]
#[test]
fn apply_rolls_back_created_file_and_dir_when_a_later_project_write_fails() {
    use std::os::unix::fs::PermissionsExt;
    let f = fixture("rollback-created");
    let a_local = f.a.join(".claude/settings.local.json");
    let before_b = std::fs::read(&f.b_local).unwrap();
    let before_user = std::fs::read(&f.user).unwrap();
    let p = preview(&kit(&f)).unwrap();
    assert!(p.changes[0].is_new_file);
    assert_eq!(Path::new(&p.changes[0].file), a_local);
    assert!(!f.a.join(".claude").exists());
    // A를 새로 쓴 다음 B의 백업 만들기에서 실패한다.
    let b_dir = f.b_local.parent().unwrap();
    let permissions = std::fs::metadata(b_dir).unwrap().permissions();
    std::fs::set_permissions(b_dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    let result = apply_now(&f, &p.before_hash);
    std::fs::set_permissions(b_dir, permissions).unwrap();
    assert!(message(result.unwrap_err()).contains("백업 만들기"));
    assert!(!a_local.exists());
    assert!(!f.a.join(".claude").exists());
    assert_eq!(std::fs::read(&f.b_local).unwrap(), before_b);
    assert_eq!(std::fs::read(&f.user).unwrap(), before_user);
    assert!(log::read(&f.log).is_empty());
    assert!(!f.log.exists());
}

#[cfg(not(windows))]
#[test]
fn undo_command_removes_created_file_and_only_a_created_dir() {
    let path = PathBuf::from("/work/it's a/.claude/settings.local.json");
    for (created_dir, expected) in [
        (false, "rm '/work/it'\\''s a/.claude/settings.local.json'"),
        (
            true,
            "rm '/work/it'\\''s a/.claude/settings.local.json' && rmdir '/work/it'\\''s a/.claude'",
        ),
    ] {
        assert_eq!(
            undo_command(&[Done::Created {
                path: path.clone(),
                created_dir,
            }]),
            expected
        );
    }
}

#[cfg(unix)]
#[test]
fn apply_rolls_back_files_already_written_when_a_later_project_file_cannot_be_replaced() {
    use std::os::unix::fs::PermissionsExt;
    let f = fixture("rollback");
    // A도 기존 파일이 있어 "바꾸기"로 쓰인다(되돌림이 백업에서 원래 바이트를 읽어야 한다).
    let a_original = "{\n  \"theme\": \"x\"\n}\n";
    let a_local = f
        .home
        .write("work/a/.claude/settings.local.json", a_original);
    let p = preview(&kit(&f)).unwrap();
    // 두 번째 프로젝트(B)는 폴더에 쓸 수 없어 백업부터 실패한다.
    let b_dir = f.b_local.parent().unwrap().to_path_buf();
    std::fs::set_permissions(&b_dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    let result = apply_now(&f, &p.before_hash);
    std::fs::set_permissions(&b_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(message(result.unwrap_err()).contains("백업 만들기"));
    assert_eq!(std::fs::read_to_string(&a_local).unwrap(), a_original);
    assert_eq!(std::fs::read_to_string(&f.b_local).unwrap(), B_LOCAL);
    assert_eq!(std::fs::read_to_string(&f.user).unwrap(), USER);
    assert!(log::read(&f.log).is_empty());
}
