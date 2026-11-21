//! Codex MCP "프로젝트에서만 켜기": 사용 읽기·신뢰 필터·적용·되돌리기. 임시 폴더를 HOME으로 쓴다.

use std::path::{Path, PathBuf};

use serde_json::json;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::coach::scope::{self, apply, preview};
use crate::coach::switches::{self, CoachSwitch};
use crate::coach::usage::{self, UsageIndex};
use crate::coach::{AppliedSwitch as Entry, log, undo as undo_record};
use crate::context::fsutil::testing::TempDir;
use crate::error::AppError;
use crate::tuning::Tuning;

const TARGET: &str = "codex/mcp:code-review-graph";
const B_OWN: &str = "model = \"x\"\n";

struct Fixture {
    home: TempDir,
    user: PathBuf,
    a: PathBuf,
    b: PathBuf,
    log: PathBuf,
}

fn user_config(home: &Path, trusted: &[&Path]) -> String {
    let mut text = String::from(
        "model = \"m\"\n\n[mcp_servers.code-review-graph]\ncommand = \"crg\"\n\n[mcp_servers.other]\ncommand = \"o\"\nenabled = false\n",
    );
    for dir in trusted {
        text.push_str(&format!(
            "\n[projects.\"{}\"]\ntrust_level = \"trusted\"\n",
            // Codex가 쓰는 것처럼 TOML 기본 문자열로 이스케이프한다(Windows 경로의 `\`).
            dir.display().to_string().replace('\\', "\\\\")
        ));
    }
    let _ = home;
    text
}

fn now() -> String {
    OffsetDateTime::now_utc().format(&Rfc3339).unwrap()
}

/// rollout 한 파일: 첫 줄 session_meta(cwd), 그 뒤 호출 줄들.
fn rollout(home: &TempDir, name: &str, cwd: &Path, calls: &[serde_json::Value]) {
    let meta = json!({"timestamp": now(), "type": "session_meta", "payload": {"cwd": cwd}});
    let lines: Vec<String> = std::iter::once(meta)
        .chain(calls.iter().cloned())
        .map(|v| v.to_string())
        .collect();
    home.write(
        &format!(".codex/sessions/2026/11/19/rollout-{name}.jsonl"),
        &(lines.join("\n") + "\n"),
    );
}

fn code_call(id: &str, code: &str) -> serde_json::Value {
    json!({"timestamp": now(), "type": "response_item",
        "payload": {"type": "custom_tool_call", "name": "exec", "call_id": id, "input": code}})
}

fn direct_call(id: &str, name: &str) -> serde_json::Value {
    json!({"timestamp": now(), "type": "response_item",
        "payload": {"type": "function_call", "name": name, "call_id": id, "arguments": "{}"}})
}

/// 서버를 A(코드 모드 2번, `src` 하위 폴더에서 시작)·B(직접 호출 1번)에서 썼다. 둘 다 신뢰한 프로젝트다.
fn fixture(name: &str, trust_b: bool) -> Fixture {
    let home = TempDir::new(&format!("coach-scope-codex-{name}"));
    let a = home.path().join("work/a");
    std::fs::create_dir_all(a.join("src")).unwrap();
    std::fs::create_dir_all(a.join(".git")).unwrap();
    let b = home.path().join("work/b");
    home.write("work/b/.codex/config.toml", B_OWN);
    let trusted: Vec<&Path> = if trust_b { vec![&a, &b] } else { vec![&a] };
    let user = home.write(".codex/config.toml", &user_config(home.path(), &trusted));
    rollout(
        &home,
        "a",
        &a.join("src"),
        &[code_call(
            "c1",
            "const r = await tools.mcp__code_review_graph__semantic_search_tool({});\nawait tools.mcp__code_review_graph__other_tool ( {} );",
        )],
    );
    rollout(
        &home,
        "b",
        &b,
        &[direct_call(
            "c2",
            "mcp__code_review_graph__semantic_search_tool",
        )],
    );
    let log = home.path().join("app-data").join(log::FILE_NAME);
    Fixture {
        home,
        user,
        a,
        b,
        log,
    }
}

fn index(f: &Fixture) -> UsageIndex {
    let tuning = Tuning {
        diagnose_min_sessions: 0,
        ..Tuning::default()
    };
    let at = crate::metrics::unix_secs(OffsetDateTime::now_utc());
    usage::read(f.home.path(), &tuning, at)
}

fn server(f: &Fixture) -> CoachSwitch {
    let all = switches::list(f.home.path(), None, Some(&index(f)), &Tuning::default());
    switches::find(all, TARGET).unwrap()
}

fn apply_now(f: &Fixture, hash: &str) -> Result<Entry, AppError> {
    apply(&server(f), &f.log, hash, OffsetDateTime::now_utc())
}

fn message(error: AppError) -> String {
    match error {
        AppError::InvalidInput(m) => m,
        AppError::Internal => "internal".into(),
    }
}

#[test]
fn counts_code_mode_and_direct_calls_by_project_root_matching_hyphenated_names() {
    let f = fixture("count", true);
    let codex = index(&f).codex.unwrap();
    // 설정 이름 `code-review-graph`는 도구 이름의 `code_review_graph`와 같은 서버다.
    let used = codex.for_server(&usage::codex_server_key("code-review-graph"));
    // A: 코드 모드 2번(`src`에서 시작해도 `.git`이 있는 A로 묶는다), B: 직접 호출 1번.
    assert_eq!(used.total, 3);
    assert_eq!(
        used.projects
            .iter()
            .map(|p| (p.name.as_str(), p.count))
            .collect::<Vec<_>>(),
        [("a", 2), ("b", 1)]
    );
    assert_eq!(codex.sessions, 2);
    assert_eq!(codex.for_server("other").total, 0);
}

#[test]
fn ignores_mentions_without_a_call_old_lines_and_repeated_call_ids() {
    let f = fixture("ignore", true);
    let old = json!({"timestamp": "2020-01-01T00:00:00Z", "type": "response_item",
        "payload": {"type": "custom_tool_call", "name": "exec", "call_id": "old",
            "input": "await tools.mcp__code_review_graph__x({})"}});
    rollout(
        &f.home,
        "c",
        &f.b,
        &[
            // 호출이 아니라 글자만 있다.
            code_call("m1", "// tools.mcp__code_review_graph__x is slow"),
            old,
            // 이어 하기로 다시 적힌 같은 호출(c2)은 한 번만.
            direct_call("c2", "mcp__code_review_graph__semantic_search_tool"),
        ],
    );
    let codex = index(&f).codex.unwrap();
    assert_eq!(codex.for_server("code_review_graph").total, 3);
}

#[test]
fn proposes_only_when_every_used_project_is_trusted() {
    let f = fixture("trust", true);
    let switch = server(&f);
    let moving = switch.scope_move.as_ref().unwrap();
    assert_eq!(
        moving
            .projects
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
    assert_eq!(switch.usage.as_ref().map(|u| u.total), Some(3));
    // 실제 사용한 서버는 끄기를 제안하지 않는다.
    assert!(!switch.is_proposed);
    let g = fixture("untrusted", false);
    assert_eq!(server(&g).scope_move, None);
}

#[test]
fn does_not_propose_for_disabled_servers() {
    let f = fixture("disabled", true);
    let text = std::fs::read_to_string(&f.user)
        .unwrap()
        .replace("command = \"crg\"", "command = \"crg\"\nenabled = false");
    std::fs::write(&f.user, text).unwrap();
    assert_eq!(server(&f).scope_move, None);
}

#[test]
fn apply_writes_both_files_and_undo_restores_every_byte() {
    let f = fixture("apply", true);
    let before_user = std::fs::read_to_string(&f.user).unwrap();
    let p = preview(&server(&f)).unwrap();
    assert_eq!(
        p.changes
            .iter()
            .map(|c| (c.is_new_file, c.after.as_str()))
            .collect::<Vec<_>>(),
        [
            (true, "enabled = true"),
            (false, "enabled = true"),
            (false, "enabled = false")
        ]
    );
    // 미리보기는 쓰지 않는다.
    assert!(!f.a.join(".codex").exists());

    let applied = apply_now(&f, &p.before_hash).unwrap();
    assert_eq!(
        std::fs::read_to_string(f.a.join(".codex/config.toml")).unwrap(),
        "[mcp_servers.code-review-graph]\nenabled = true\n"
    );
    assert_eq!(
        std::fs::read_to_string(f.b.join(".codex/config.toml")).unwrap(),
        format!("{B_OWN}\n[mcp_servers.code-review-graph]\nenabled = true\n")
    );
    assert_eq!(
        std::fs::read_to_string(&f.user).unwrap(),
        before_user.replace(
            "[mcp_servers.code-review-graph]\n",
            "[mcp_servers.code-review-graph]\nenabled = false\n"
        )
    );
    assert!(applied.target.starts_with(scope::codex::TARGET_PREFIX));
    assert_eq!(applied.agent, crate::usage::Agent::Codex);
    assert_eq!(log::read(&f.log), std::slice::from_ref(&applied));
    // 옮긴 뒤에는 이미 꺼져 있어 다시 제안하지 않는다.
    assert_eq!(server(&f).scope_move, None);

    undo_record(
        f.home.path(),
        &f.log,
        &applied.id,
        OffsetDateTime::now_utc(),
    )
    .unwrap();
    assert_eq!(std::fs::read_to_string(&f.user).unwrap(), before_user);
    assert_eq!(
        std::fs::read_to_string(f.b.join(".codex/config.toml")).unwrap(),
        B_OWN
    );
    assert!(!f.a.join(".codex").exists());
}

#[test]
fn apply_refuses_when_any_file_changed_after_preview() {
    let f = fixture("stale", true);
    let p = preview(&server(&f)).unwrap();
    std::fs::write(f.b.join(".codex/config.toml"), "model = \"y\"\n").unwrap();
    assert!(message(apply_now(&f, &p.before_hash).unwrap_err()).contains("바뀌었어요"));
    assert!(!f.a.join(".codex").exists());
    assert!(log::read(&f.log).is_empty());
}

#[cfg(unix)]
#[test]
fn apply_rolls_back_created_file_and_dir_when_a_later_project_write_fails() {
    use std::os::unix::fs::PermissionsExt;
    let f = fixture("rollback-created", true);
    let a_config = f.a.join(".codex/config.toml");
    let b_config = f.b.join(".codex/config.toml");
    let before_b = std::fs::read(&b_config).unwrap();
    let before_user = std::fs::read(&f.user).unwrap();
    let p = preview(&server(&f)).unwrap();
    assert!(p.changes[0].is_new_file);
    assert_eq!(Path::new(&p.changes[0].file), a_config);
    assert!(!f.a.join(".codex").exists());
    // A를 새로 쓴 다음 B의 백업 만들기에서 실패한다.
    let b_dir = b_config.parent().unwrap();
    let permissions = std::fs::metadata(b_dir).unwrap().permissions();
    std::fs::set_permissions(b_dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    let result = apply_now(&f, &p.before_hash);
    std::fs::set_permissions(b_dir, permissions).unwrap();
    assert!(message(result.unwrap_err()).contains("백업 만들기"));
    assert!(!a_config.exists());
    assert!(!f.a.join(".codex").exists());
    assert_eq!(std::fs::read(&b_config).unwrap(), before_b);
    assert_eq!(std::fs::read(&f.user).unwrap(), before_user);
    assert!(log::read(&f.log).is_empty());
    assert!(!f.log.exists());
}

#[test]
fn apply_refuses_a_project_file_that_defines_mcp_servers_another_way() {
    let f = fixture("inline", true);
    std::fs::write(
        f.b.join(".codex/config.toml"),
        "mcp_servers = { x = { command = \"c\" } }\n",
    )
    .unwrap();
    assert!(preview(&server(&f)).is_err());
    assert!(!f.a.join(".codex").exists());
}

#[test]
fn undo_refuses_when_a_project_file_changed_after_apply() {
    let f = fixture("undo-stale", true);
    let p = preview(&server(&f)).unwrap();
    let applied = apply_now(&f, &p.before_hash).unwrap();
    std::fs::write(f.b.join(".codex/config.toml"), "model = \"z\"\n").unwrap();
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
}

#[test]
fn undo_refuses_records_pointing_outside_project_codex_config() {
    let f = fixture("undo-path", true);
    let p = preview(&server(&f)).unwrap();
    let applied = apply_now(&f, &p.before_hash).unwrap();
    let mut tampered = applied.clone();
    tampered.id = "1".into();
    tampered.files[0].file = f.a.join("notes.txt").to_string_lossy().into_owned();
    log::append(&f.log, &tampered).unwrap();
    assert!(
        message(undo_record(f.home.path(), &f.log, "1", OffsetDateTime::now_utc()).unwrap_err())
            .contains("경로")
    );
    assert!(f.a.join(".codex/config.toml").exists());
}

#[cfg(unix)]
#[test]
fn apply_refuses_when_project_codex_dir_or_file_links_outside() {
    let f = fixture("link-dir", true);
    let outside = f.home.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, f.a.join(".codex")).unwrap();
    let error = message(apply_now(&f, "0000000000000000").unwrap_err());
    assert!(error.contains("링크"), "{error}");
    assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
    assert!(log::read(&f.log).is_empty());

    let g = fixture("link-file", true);
    let victim = g.home.write("outside/victim.toml", B_OWN);
    let file = g.b.join(".codex/config.toml");
    std::fs::remove_file(&file).unwrap();
    std::os::unix::fs::symlink(&victim, &file).unwrap();
    let error = message(apply_now(&g, "0000000000000000").unwrap_err());
    assert!(error.contains("링크"), "{error}");
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), B_OWN);
    assert!(!g.a.join(".codex").exists());
}

#[test]
fn proposal_gate_uses_the_same_use_and_session_thresholds_as_claude() {
    let f = fixture("gate", true);
    let used = server(&f).usage.unwrap();
    let trusted =
        crate::context::scan::codex_trusted_projects(&std::fs::read_to_string(&f.user).unwrap());
    let home = f.home.path();
    assert!(scope::codex::proposal(home, true, &used, &trusted).is_some());
    let few = usage::SwitchUsage {
        total: 2,
        ..used.clone()
    };
    assert_eq!(scope::codex::proposal(home, true, &few, &trusted), None);
    assert_eq!(scope::codex::proposal(home, false, &used, &trusted), None);
}

fn completed_call(id: &str, server: &str) -> serde_json::Value {
    json!({"timestamp": now(), "type": "event_msg", "payload": {
        "type": "item_completed", "item": {"type": "McpToolCall", "id": id,
        "server": server, "tool": "read"}}})
}

#[test]
fn counts_completed_mcp_calls_once_and_prefers_them_over_outer_code_ids() {
    let f = fixture("completed", true);
    rollout(
        &f.home,
        "a",
        &f.a,
        &[
            code_call("same", "await tools.mcp__code_review_graph__read({})"),
            completed_call("same", "code-review-graph"),
            completed_call("same", "code-review-graph"),
            code_call("outer", "await tools.mcp__code_review_graph__read({})"),
            completed_call("inner", "code-review-graph"),
            completed_call("other", "other"),
        ],
    );
    let codex = index(&f).codex.unwrap();
    // A 완료 2회 + B 기존 function_call 1회. 바깥 ID가 달라도 중복 집계하지 않는다.
    assert_eq!(codex.for_server("code_review_graph").total, 3);
    assert_eq!(codex.for_server("other").total, 1);
}

#[test]
fn old_or_invalid_completed_events_do_not_replace_recent_code_calls() {
    let f = fixture("invalid-completed", true);
    let mut old = completed_call("old", "code-review-graph");
    old["timestamp"] = json!("2020-01-01T00:00:00Z");
    let mut wrong = completed_call("wrong", "code-review-graph");
    wrong["payload"]["type"] = json!("item_started");
    rollout(
        &f.home,
        "a",
        &f.a,
        &[
            old,
            wrong,
            code_call("recent", "await tools.mcp__code_review_graph__read({})"),
        ],
    );
    assert_eq!(
        index(&f)
            .codex
            .unwrap()
            .for_server("code_review_graph")
            .total,
        2
    );
}

#[test]
fn recently_modified_old_sessions_are_not_evidence_and_broken_records_are_partial() {
    let f = fixture("old-session", true);
    let old = json!({"timestamp": "2020-01-01T00:00:00Z", "type": "session_meta",
        "payload": {"cwd": f.a}});
    f.home.write(
        ".codex/sessions/rollout-old.jsonl",
        &(old.to_string() + "\n"),
    );
    assert_eq!(index(&f).codex.unwrap().sessions, 2);
    f.home
        .write(".codex/sessions/rollout-broken.jsonl", "{broken\n");
    assert!(index(&f).codex.unwrap().is_partial);
}

#[test]
fn ignores_unrelated_completed_items_without_marking_history_partial() {
    let f = fixture("other-item", true);
    rollout(
        &f.home,
        "a",
        &f.a,
        &[
            json!({"timestamp": now(), "type": "event_msg", "payload": {
            "type": "item_completed", "item": {"type": "AgentMessage", "text": "done"}}}),
            code_call("recent", "await tools.mcp__code_review_graph__read({})"),
        ],
    );
    let codex = index(&f).codex.unwrap();
    assert!(!codex.is_partial);
    assert_eq!(codex.for_server("code_review_graph").total, 2);
}

#[test]
fn unreadable_compressed_rollout_marks_history_partial() {
    let f = fixture("bad-compression", true);
    f.home
        .write(".codex/sessions/rollout-bad.jsonl.zst", "not zstd");
    assert!(index(&f).codex.unwrap().is_partial);
}
