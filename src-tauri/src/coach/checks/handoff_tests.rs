//! Claude에게 맡기기 테스트. 실제 `claude`는 실행하지 않는다(가짜 CLI = TempDir의 실행 스크립트).
//! 실제 실행은 `#[ignore]` 하나뿐이다(`real_claude_rewrites_overlapping_claude_md`, 조율자가 돌린다).

#[cfg(unix)]
use std::sync::Arc;

#[cfg(unix)]
use serde_json::json;

use super::super::rules_paths::RulesScope;
use super::super::tests::records;
use super::*;
use crate::coach::{log, undo};
use crate::context::fsutil::testing::TempDir;
use time::macros::datetime;

const NOW: OffsetDateTime = datetime!(2026-11-07 12:00:00.000 UTC);

fn rules(n: usize) -> String {
    (1..=n).map(|i| format!("- rule {i}\n")).collect()
}

struct Fixture {
    home: TempDir,
    work: TempDir,
    data: TempDir,
}

fn fixture(name: &str, text: &str) -> Fixture {
    let f = Fixture {
        home: TempDir::new(&format!("handoff-{name}-home")),
        work: TempDir::new(&format!("handoff-{name}-work")),
        data: TempDir::new(&format!("handoff-{name}-data")),
    };
    f.work.write("CLAUDE.md", text);
    f
}

fn recent(workspaces: &[&Path]) -> RecentRecords {
    RecentRecords {
        workspaces: workspaces.iter().map(|p| p.to_path_buf()).collect(),
        ..records(&[])
    }
}

impl Fixture {
    fn records(&self) -> RecentRecords {
        recent(&[self.work.path()])
    }

    fn id(&self) -> String {
        format!(
            "{ID_PREFIX}{}",
            self.work.path().join("CLAUDE.md").display()
        )
    }

    fn target(&self) -> Target {
        find_target(
            self.home.path(),
            &self.records(),
            &Tuning::default(),
            &self.id(),
        )
        .unwrap()
    }

    fn spec(&self, cli: Option<PathBuf>, timeout: Duration) -> RunSpec {
        RunSpec {
            cli,
            dir: self.data.path().join(DIR_NAME),
            timeout,
        }
    }

    fn log_file(&self) -> PathBuf {
        log::file(self.data.path())
    }
}

/// 이 테스트가 만든 폴더 안의 긴 안내문 카드 파일(Windows 임시 폴더가 실제 홈 아래여도 실제 파일이 끼지 않게).
fn long_cards(home: &Path, records: &RecentRecords, under: &Path) -> Vec<String> {
    list(home, records, &Tuning::default())
        .into_iter()
        .filter(|c| matches!(c.facts, CheckFacts::LongInstructions { .. }))
        .filter(|c| Path::new(&c.file).starts_with(under))
        .map(|c| c.file)
        .collect()
}

fn store(state: &HandoffState, target: &Target, text: &str) {
    state
        .keep(Stored {
            id: target.id.clone(),
            before_hash: target.before_hash.clone(),
            original: target.original.clone(),
            text: text.into(),
            removed: vec!["겹치는 규칙을 합쳤어요".into()],
        })
        .unwrap();
}

// ---------- 감지 ----------

#[test]
fn file_of_200_lines_has_no_card_and_201_lines_has_one() {
    let f = fixture("boundary", &rules(200));
    assert!(long_cards(f.home.path(), &f.records(), f.work.path()).is_empty());
    f.work.write("CLAUDE.md", &rules(201));
    let cards = list(f.home.path(), &f.records(), &Tuning::default());
    let card = cards.iter().find(|c| c.id == f.id()).unwrap();
    let CheckFacts::LongInstructions { lines, limit, .. } = &card.facts else {
        panic!("{:?}", card.facts)
    };
    assert_eq!((*lines, *limit), (201, 200));
    let ids: Vec<(&str, bool)> = card
        .options
        .iter()
        .map(|o| (o.id.as_str(), o.is_recommended))
        .collect();
    assert_eq!(ids, [(HANDOFF, true), ("keep", false)]);
}

#[test]
fn user_and_project_files_get_different_scope_and_folder() {
    let f = fixture("scope", &rules(201));
    f.home.write(".claude/CLAUDE.md", &rules(202));
    f.work.write("CLAUDE.local.md", &rules(203));
    let real = |p: &Path| {
        std::fs::canonicalize(p)
            .unwrap()
            .to_string_lossy()
            .into_owned()
    };
    let (home, work) = (real(f.home.path()), real(f.work.path()));
    let mut found: Vec<(String, RulesScope, String)> =
        list(f.home.path(), &f.records(), &Tuning::default())
            .into_iter()
            .filter_map(|c| match c.facts {
                CheckFacts::LongInstructions {
                    name,
                    scope,
                    folder,
                    ..
                } => Some((name, scope, folder)),
                _ => None,
            })
            .collect();
    found.sort_by_key(|(n, s, _)| (n.clone(), *s == RulesScope::User));
    let want = |n: &str, s, d: &String| (n.to_string(), s, d.clone());
    assert_eq!(
        found,
        [
            want("CLAUDE.local.md", RulesScope::Project, &work),
            want("CLAUDE.md", RulesScope::Project, &work),
            want("CLAUDE.md", RulesScope::User, &home),
        ]
    );
}

#[test]
fn same_file_from_several_workspaces_is_one_card() {
    let home = TempDir::new("handoff-multi-home");
    let base = TempDir::new("handoff-multi-base");
    base.write("CLAUDE.md", &rules(250));
    base.write("a/.keep", "");
    base.write("b/.keep", "");
    let (a, b) = (base.path().join("a"), base.path().join("b"));
    let cards = long_cards(home.path(), &recent(&[&a, &b]), base.path());
    assert_eq!(
        cards,
        [base.path().join("CLAUDE.md").to_string_lossy().into_owned()]
    );
}

#[test]
fn imported_file_is_not_a_target() {
    let f = fixture("import", "@AGENTS.md\n");
    f.work.write("AGENTS.md", &rules(300));
    assert!(long_cards(f.home.path(), &f.records(), f.work.path()).is_empty());
}

#[test]
fn claude_md_in_plugin_cache_is_not_a_target() {
    let home = TempDir::new("handoff-plugin-home");
    let plugin = home.write(".claude/plugins/cache/m/p/1/CLAUDE.md", &rules(250));
    let dir = plugin.parent().unwrap().to_path_buf();
    assert!(long_cards(home.path(), &recent(&[&dir]), home.path()).is_empty());
}

// ---------- 한도 문·결과 판정(실행 없이) ----------

#[test]
fn limit_below_threshold_or_unknown_is_refused() {
    assert_eq!(
        check_limit(Some(12.0), 20),
        Err(HandoffError::LimitTooLow {
            remaining: 12.0,
            min_remaining: 20
        })
    );
    assert_eq!(check_limit(None, 20), Err(HandoffError::LimitUnknown));
    assert_eq!(check_limit(Some(20.0), 20), Ok(()));
    assert_eq!(check_limit(Some(0.0), 0), Ok(()));
}

#[test]
fn empty_longer_or_unchanged_result_cannot_be_applied() {
    let f = fixture("can-apply", &rules(210));
    let target = f.target();
    let preview = |text: &str| {
        let stored = Stored {
            id: target.id.clone(),
            before_hash: target.before_hash.clone(),
            original: target.original.clone(),
            text: text.into(),
            removed: vec![],
        };
        preview_of(&target.file, &stored).can_apply
    };
    assert!(preview(&rules(150)));
    assert!(!preview("  \n"));
    assert!(!preview(&rules(211)));
    assert!(!preview(&target.original));
}

// ---------- 적용·되돌리기 ----------

#[test]
fn apply_writes_stored_text_and_undo_works_after_card_disappears() {
    let f = fixture("undo", &rules(230));
    let state = HandoffState::default();
    let target = f.target();
    store(&state, &target, &rules(150));
    let applied = apply(
        &state,
        f.home.path(),
        &f.records(),
        &Tuning::default(),
        &f.log_file(),
        &f.id(),
        &target.before_hash,
        NOW,
    )
    .unwrap();
    assert_eq!(applied.name, "CLAUDE.md 줄이기");
    assert_eq!(
        applied.target,
        format!("{CHECK_TARGET_PREFIX}{}:handoff", f.id())
    );
    assert_eq!(std::fs::read_to_string(&target.file).unwrap(), rules(150));
    // 200줄 아래가 되어 카드가 사라졌다.
    assert!(long_cards(f.home.path(), &f.records(), f.work.path()).is_empty());
    let undone = undo(f.home.path(), &f.log_file(), &applied.id, NOW).unwrap();
    assert_eq!(undone.undo_of.as_deref(), Some(applied.id.as_str()));
    assert_eq!(std::fs::read_to_string(&target.file).unwrap(), rules(230));
}

#[test]
fn apply_refuses_when_original_changed_after_handoff() {
    let f = fixture("changed", &rules(230));
    let state = HandoffState::default();
    let target = f.target();
    store(&state, &target, &rules(150));
    f.work.write("CLAUDE.md", &rules(231));
    let error = apply(
        &state,
        f.home.path(),
        &f.records(),
        &Tuning::default(),
        &f.log_file(),
        &f.id(),
        &target.before_hash,
        NOW,
    )
    .unwrap_err();
    assert_eq!(
        error,
        refuse("Claude에게 맡긴 뒤 파일이 바뀌었어요. 다시 맡겨 주세요.")
    );
    assert_eq!(std::fs::read_to_string(&target.file).unwrap(), rules(231));
}

#[test]
fn undo_refuses_record_whose_file_is_not_instruction_shaped() {
    let f = fixture("undo-shape", &rules(10));
    let file = f.work.write("notes.md", "after");
    let backup = f
        .work
        .write("notes.md.kcal-20261107T120000000Z.bak", "before");
    let entry = AppliedSwitch {
        id: "1".into(),
        at: NOW,
        action: CoachAction::Apply,
        target: format!("{CHECK_TARGET_PREFIX}x:handoff"),
        agent: Agent::Claude,
        name: "notes.md 줄이기".into(),
        file: file.to_string_lossy().into_owned(),
        backup: backup.to_string_lossy().into_owned(),
        enabled: false,
        tokens: None,
        after_hash: edit::content_hash(b"after"),
        undo_command: String::new(),
        undo_of: None,
        created: None,
        files: Vec::new(),
    };
    log::append(&f.log_file(), &entry).unwrap();
    let error = undo(f.home.path(), &f.log_file(), "1", NOW).unwrap_err();
    assert!(matches!(error, AppError::InvalidInput(m) if m.contains("kcal이 만든 것이 아니라")));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "after");
}

#[test]
fn instruction_shape_accepts_claude_md_and_rules_only() {
    let base = std::env::temp_dir();
    assert!(is_instruction_file(&base.join("w/CLAUDE.md")));
    assert!(is_instruction_file(&base.join("w/CLAUDE.local.md")));
    assert!(is_instruction_file(
        &base.join("h/.claude/rules/common/a.md")
    ));
    assert!(!is_instruction_file(&base.join("h/.claude/settings.json")));
    assert!(!is_instruction_file(&base.join("w/AGENTS.md")));
    assert!(!is_instruction_file(Path::new("CLAUDE.md")));
    assert!(!is_undoable("codex/plugin:x", &base.join("w/CLAUDE.md")));
}

#[cfg(unix)]
#[test]
fn link_file_is_written_through_and_link_stays() {
    let f = fixture("link", "");
    let dotfiles = TempDir::new("handoff-link-dotfiles");
    let real = dotfiles.write("claude/CLAUDE.md", &rules(230));
    let link = f.work.path().join("CLAUDE.md");
    std::fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let id = format!("{ID_PREFIX}{}", real.display());
    let target = find_target(f.home.path(), &f.records(), &Tuning::default(), &id).unwrap();
    assert_eq!(target.file, real);
    let state = HandoffState::default();
    store(&state, &target, &rules(150));
    let applied = apply(
        &state,
        f.home.path(),
        &f.records(),
        &Tuning::default(),
        &f.log_file(),
        &id,
        &target.before_hash,
        NOW,
    )
    .unwrap();
    assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
    assert_eq!(std::fs::read_to_string(&link).unwrap(), rules(150));
    assert_eq!(Path::new(&applied.backup).parent(), real.parent());
    assert_eq!(applied.file, real.to_string_lossy());
}

/// 실데이터에 있던 모양: `CLAUDE.md -> AGENTS.md`. 실제 파일이 AGENTS.md라 대상이 아니다.
#[cfg(unix)]
#[test]
fn claude_md_linked_to_agents_md_is_not_a_target() {
    let f = fixture("agents-link", "");
    let agents = f.work.write("AGENTS.md", &rules(300));
    let link = f.work.path().join("CLAUDE.md");
    std::fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink(&agents, &link).unwrap();
    assert!(long_cards(f.home.path(), &f.records(), f.work.path()).is_empty());
}

// ---------- 가짜 CLI ----------

#[cfg(unix)]
mod fake_cli {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// `body`를 끝에 붙인 실행 스크립트. 인자·작업 폴더·stdin·실행 횟수를 스크립트 옆에 남긴다.
    pub fn cli(dir: &TempDir, body: &str) -> PathBuf {
        let script = format!(
            "#!/bin/sh\nd=\"$(dirname \"$0\")\"\nprintf '%s\\n' \"$@\" > \"$d/args\"\npwd > \"$d/cwd\"\n\
             echo run >> \"$d/runs\"\n{body}\n"
        );
        let path = dir.write("bin/claude", &script);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    pub fn respond(dir: &TempDir, value: &Value) -> String {
        dir.write("bin/response.json", &value.to_string());
        "cat > \"$d/stdin\"\ncat \"$d/response.json\"".into()
    }

    pub fn success(text: &str) -> Value {
        json!({"type": "result", "is_error": false, "result": "{}", "total_cost_usd": 0.01,
            "structured_output": {"text": text, "removed": ["  겹치는 규칙 두 줄을 합쳤어요 ", ""]}})
    }

    pub fn ask_once(
        f: &Fixture,
        state: &HandoffState,
        cli: PathBuf,
        timeout: Duration,
    ) -> Result<HandoffPreview, HandoffError> {
        tauri::async_runtime::block_on(ask(
            state,
            f.target(),
            Some(64.0),
            20,
            f.spec(Some(cli), timeout),
        ))
    }

    pub fn read(dir: &TempDir, name: &str) -> String {
        std::fs::read_to_string(dir.path().join("bin").join(name)).unwrap_or_default()
    }
}

#[cfg(unix)]
use fake_cli::{ask_once, cli, read, respond, success};

#[cfg(unix)]
const SECOND: Duration = Duration::from_secs(1);
#[cfg(unix)]
const MINUTE: Duration = Duration::from_secs(60);

#[cfg(unix)]
#[test]
fn success_uses_fixed_args_stdin_prompt_and_empty_work_dir() {
    let original = format!("@AGENTS.md\n@docs/style.md\n{}", rules(220));
    let f = fixture("success", &original);
    let body = respond(&f.data, &success(&format!("@AGENTS.md\n{}", rules(120))));
    let preview = ask_once(&f, &HandoffState::default(), cli(&f.data, &body), MINUTE).unwrap();
    let args = read(&f.data, "args");
    for arg in [
        "-p",
        "--safe-mode",
        MODEL,
        "medium",
        "--no-session-persistence",
        "--json-schema",
    ] {
        assert!(args.lines().any(|l| l == arg), "{arg} missing: {args}");
    }
    assert!(args.contains("--tools\n\n"), "empty --tools: {args}");
    let stdin = read(&f.data, "stdin");
    assert!(
        stdin.contains("번역하지 마라") && stdin.contains("@AGENTS.md"),
        "{stdin}"
    );
    assert!(!args.contains("rule 1"), "file content must not be in args");
    let cwd = std::fs::canonicalize(read(&f.data, "cwd").trim()).unwrap();
    assert_eq!(
        cwd,
        std::fs::canonicalize(f.data.path().join(DIR_NAME).join("work")).unwrap()
    );
    assert_eq!((preview.before_lines, preview.after_lines), (222, 121));
    assert_eq!(preview.removed, ["겹치는 규칙 두 줄을 합쳤어요"]);
    assert_eq!(preview.missing_imports, ["docs/style.md"]);
    assert!(preview.can_apply);
    // 프롬프트 파일은 지운다.
    let left: Vec<_> = std::fs::read_dir(f.data.path().join(DIR_NAME))
        .unwrap()
        .flatten()
        .map(|e| e.file_name())
        .collect();
    assert_eq!(left, ["work"]);
}

#[cfg(unix)]
#[test]
fn opus_404_is_reported_as_opus_unavailable() {
    let f = fixture("opus-404", &rules(210));
    let response = json!({"type": "result", "is_error": true, "terminal_reason": "api_error",
        "api_error_status": 404, "result": "There's an issue with the selected model"});
    let body = format!("{}\nexit 1", respond(&f.data, &response));
    let error = ask_once(&f, &HandoffState::default(), cli(&f.data, &body), MINUTE).unwrap_err();
    assert_eq!(error, HandoffError::OpusUnavailable);
}

#[cfg(unix)]
#[test]
fn unrecognized_model_in_stderr_is_reported_as_opus_unavailable() {
    let f = fixture("opus-stderr", &rules(210));
    let body = "echo '[claude-code:unrecognized_model] no access' >&2\nexit 1";
    let error = ask_once(&f, &HandoffState::default(), cli(&f.data, body), MINUTE).unwrap_err();
    assert_eq!(error, HandoffError::OpusUnavailable);
}

#[cfg(unix)]
#[test]
fn timeout_kills_cli_and_reports_timeout() {
    let f = fixture("timeout", &rules(210));
    let body = "echo $$ > \"$d/pid\"\nexec sleep 30";
    let error = ask_once(&f, &HandoffState::default(), cli(&f.data, body), SECOND).unwrap_err();
    assert_eq!(error, HandoffError::Timeout { secs: 1 });
    let pid = read(&f.data, "pid").trim().to_owned();
    let alive = || {
        std::process::Command::new("kill")
            .args(["-0", &pid])
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap()
            .success()
    };
    for _ in 0..40 {
        if !alive() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!alive(), "fake claude {pid} still running after timeout");
}

#[cfg(unix)]
#[test]
fn invalid_json_is_a_short_failure_without_stderr() {
    let f = fixture("bad-json", &rules(210));
    let body = "echo 'tok-SECRET-1' >&2\necho 'not json'";
    let error = ask_once(&f, &HandoffState::default(), cli(&f.data, body), MINUTE).unwrap_err();
    assert_eq!(error, failed("Claude 응답을 읽지 못했어요"));
    let response = json!({"type": "result", "is_error": false, "structured_output": {"text": 3}});
    let body = respond(&f.data, &response);
    let error = ask_once(&f, &HandoffState::default(), cli(&f.data, &body), MINUTE).unwrap_err();
    assert_eq!(error, failed("Claude 응답이 약속한 모양이 아니에요"));
}

#[cfg(unix)]
#[test]
fn other_cli_error_reports_exit_code_and_terminal_reason_only() {
    let f = fixture("exit-2", &rules(210));
    let response = json!({"is_error": true, "terminal_reason": "max_turns", "result": "/Users/me/secret path"});
    let body = format!("{}\nexit 2", respond(&f.data, &response));
    let error = ask_once(&f, &HandoffState::default(), cli(&f.data, &body), MINUTE).unwrap_err();
    assert_eq!(error, failed("exit 2, max_turns"));
}

#[cfg(unix)]
#[test]
fn second_ask_while_running_is_refused_as_busy() {
    let f = Arc::new(fixture("busy", &rules(210)));
    let body = format!("sleep 1\n{}", respond(&f.data, &success(&rules(120))));
    let path = cli(&f.data, &body);
    let state = Arc::new(HandoffState::default());
    let first = {
        let (f, state, path) = (f.clone(), state.clone(), path.clone());
        tauri::async_runtime::spawn(async move {
            ask(
                &state,
                f.target(),
                Some(64.0),
                20,
                f.spec(Some(path), MINUTE),
            )
            .await
        })
    };
    for _ in 0..100 {
        if state.is_running.load(Ordering::SeqCst) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(ask_once(&f, &state, path, MINUTE), Err(HandoffError::Busy));
    let done = tauri::async_runtime::block_on(first).unwrap();
    assert!(done.is_ok(), "{done:?}");
    assert_eq!(read(&f.data, "runs").lines().count(), 1);
}

#[cfg(unix)]
#[test]
fn same_card_and_hash_reuses_stored_result_without_running_again() {
    let f = fixture("reuse", &rules(210));
    let body = respond(&f.data, &success(&rules(120)));
    let path = cli(&f.data, &body);
    let state = HandoffState::default();
    let first = ask_once(&f, &state, path.clone(), MINUTE).unwrap();
    // 한도를 몰라도 보관본은 돌려준다(다시 실행하지 않으니 한도를 쓰지 않는다).
    let again = tauri::async_runtime::block_on(ask(
        &state,
        f.target(),
        None,
        20,
        f.spec(Some(path), MINUTE),
    ));
    assert_eq!(again, Ok(first));
    assert_eq!(read(&f.data, "runs").lines().count(), 1);
}

#[cfg(unix)]
#[test]
fn low_or_unknown_limit_and_missing_cli_are_refused_before_running() {
    let f = fixture("gate", &rules(210));
    let body = respond(&f.data, &success(&rules(120)));
    let path = cli(&f.data, &body);
    let state = HandoffState::default();
    let run_with = |remaining, cli| {
        tauri::async_runtime::block_on(ask(&state, f.target(), remaining, 20, f.spec(cli, MINUTE)))
    };
    assert_eq!(
        run_with(Some(12.0), Some(path.clone())),
        Err(HandoffError::LimitTooLow {
            remaining: 12.0,
            min_remaining: 20
        })
    );
    assert_eq!(run_with(None, Some(path)), Err(HandoffError::LimitUnknown));
    assert_eq!(run_with(Some(64.0), None), Err(HandoffError::CliMissing));
    assert_eq!(read(&f.data, "runs"), "");
}

// ---------- 실제 실행(돌리지 않는다) ----------

/// 겹치는 규칙이 많은 230줄쯤 되는 CLAUDE.md(영어, `@` 가져오기 두 줄).
fn overlapping_claude_md() -> String {
    let topics = [
        ("commit", "Write commit messages in the imperative mood"),
        ("test", "Run the test suite before you push"),
        (
            "style",
            "Follow the existing code style of the file you edit",
        ),
        ("deps", "Do not add a new dependency without asking"),
        (
            "errors",
            "Handle every error explicitly and never swallow it",
        ),
        ("secrets", "Never put secrets in code or logs"),
        (
            "naming",
            "Use descriptive names for variables and functions",
        ),
        ("docs", "Update the README when a command changes"),
        ("review", "Keep pull requests small and focused"),
        ("perf", "Do not poll; use events instead of timers"),
    ];
    let mut lines = vec![
        "# Project rules".to_owned(),
        "@AGENTS.md".to_owned(),
        "@~/.claude/extra.md".to_owned(),
        String::new(),
    ];
    for (name, rule) in topics {
        lines.push(format!("## {name}"));
        for i in 0..21 {
            lines.push(match i % 3 {
                0 => format!("- {rule}."),
                1 => format!("- Always: {}.", rule.to_lowercase()),
                _ => format!("- Remember ({name} #{i}): {}.", rule.to_lowercase()),
            });
        }
        lines.push(String::new());
    }
    lines.join("\n") + "\n"
}

/// 실행: `cargo test --lib real_claude_rewrites_overlapping_claude_md -- --ignored --nocapture`(src-tauri에서).
/// 실제 `claude`를 한 번 실행해 한도를 쓴다. 한도 문은 거치지 않는다(남은 한도를 먼저 직접 확인한다).
#[test]
#[ignore = "실제 claude CLI를 실행해 한도를 쓴다"]
fn real_claude_rewrites_overlapping_claude_md() {
    let f = fixture("real", &overlapping_claude_md());
    let real_home = std::env::home_dir().unwrap();
    let path_env = std::env::var_os("PATH");
    let cli = find_cli(&real_home, path_env.as_deref()).expect("claude CLI 없음");
    let target = f.target();
    println!(
        "cli={} before_lines={}",
        cli.display(),
        line_count(&target.original)
    );
    let result = tauri::async_runtime::block_on(ask(
        &HandoffState::default(),
        target,
        Some(100.0),
        0,
        f.spec(Some(cli), Duration::from_secs(300)),
    ));
    match result {
        Ok(p) => {
            let changed = p
                .diff
                .iter()
                .filter(|l| l.kind != diff::DiffKind::Same)
                .count();
            println!(
                "after_lines={} can_apply={} changed_lines={changed}",
                p.after_lines, p.can_apply
            );
            println!("missing_imports={:?}", p.missing_imports);
            for r in &p.removed {
                println!("removed: {r}");
            }
        }
        Err(e) => println!("error={e:?}"),
    }
}
