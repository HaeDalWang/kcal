//! 적용·되돌리기 테스트. 모두 임시 폴더를 HOME으로 쓴다(실제 설정 파일을 쓰지 않는다).

use super::*;
use crate::context::fsutil::testing::TempDir;
use crate::context::{AgentContext, ContextItem, ContextKind, ContextSource, Loading};
use crate::usage::Agent;
use time::macros::datetime;

const CONFIG: &str =
    "# keep me\n[plugins.\"big@m\"]\nenabled = true\n\n[mcp_servers.docs]\ncommand = \"x\"\n";
const NOW: OffsetDateTime = datetime!(2026-11-05 12:00:00.123 UTC);
const TARGET: &str = "codex/plugin:big@m";

fn snapshot() -> ContextSnapshot {
    ContextSnapshot {
        computed_at: NOW,
        agents: vec![AgentContext {
            agent: Agent::Codex,
            items: vec![ContextItem {
                kind: ContextKind::Plugins,
                name: "big@m (스킬 3개)".into(),
                path: None,
                bytes: Some(3600),
                tokens: Some(900),
                source: ContextSource::Estimated,
                loading: Loading::Startup,
            }],
            total_startup_tokens: 900,
            first_request_tokens: None,
            first_request_chars: None,
            model: None,
            context_window: None,
            warnings: vec![],
        }],
    }
}

struct Fixture {
    home: TempDir,
    log: PathBuf,
    config: PathBuf,
}

fn fixture(name: &str) -> Fixture {
    let home = TempDir::new(&format!("coach-{name}"));
    let config = home.write(".codex/config.toml", CONFIG);
    let log = home.path().join("app-data").join(log::FILE_NAME);
    Fixture { home, log, config }
}

fn preview_of(f: &Fixture, target: &str, enabled: bool) -> Result<SwitchPreview, AppError> {
    preview(
        f.home.path(),
        Some(&snapshot()),
        None,
        &Tuning::default(),
        target,
        enabled,
    )
}

fn apply_with(f: &Fixture, hash: &str) -> Result<AppliedSwitch, AppError> {
    apply(
        f.home.path(),
        Some(&snapshot()),
        None,
        &Tuning::default(),
        &f.log,
        TARGET,
        false,
        hash,
        NOW,
    )
}

fn message(error: AppError) -> String {
    match error {
        AppError::InvalidInput(m) => m,
        AppError::Internal => "internal".into(),
    }
}

#[test]
fn preview_shows_the_line_to_change_without_writing() {
    let f = fixture("preview");
    let p = preview_of(&f, TARGET, false).unwrap();
    assert_eq!(
        (p.before.as_deref(), p.after.as_str()),
        (Some("enabled = true"), "enabled = false")
    );
    assert!(p.changed);
    assert_eq!(p.before_hash, edit::content_hash(CONFIG.as_bytes()));
    assert_eq!(std::fs::read_to_string(&f.config).unwrap(), CONFIG);
}

#[test]
fn apply_refuses_and_leaves_file_unchanged_when_it_changed_after_preview() {
    let f = fixture("stale");
    let p = preview_of(&f, TARGET, false).unwrap();
    let changed = format!("{CONFIG}# agent wrote this\n");
    std::fs::write(&f.config, &changed).unwrap();
    assert!(message(apply_with(&f, &p.before_hash).unwrap_err()).contains("바뀌었어요"));
    assert_eq!(std::fs::read_to_string(&f.config).unwrap(), changed);
    assert!(log::read(&f.log).is_empty());
}

#[test]
fn apply_writes_backup_next_to_original_with_same_content() {
    let f = fixture("backup");
    let p = preview_of(&f, TARGET, false).unwrap();
    let applied = apply_with(&f, &p.before_hash).unwrap();
    let backup = PathBuf::from(&applied.backup);
    assert_eq!(backup.parent(), f.config.parent());
    assert_eq!(
        backup.file_name().unwrap().to_string_lossy(),
        "config.toml.kcal-20261105T120000123Z.bak"
    );
    assert_eq!(std::fs::read_to_string(&backup).unwrap(), CONFIG);
    assert_eq!(
        std::fs::read_to_string(&f.config).unwrap(),
        CONFIG.replace("enabled = true", "enabled = false")
    );
    assert_eq!(applied.tokens, Some(900));
    assert!(applied.undo_command.contains(&applied.backup));
    assert_eq!(log::read(&f.log), [applied]);
}

#[cfg(unix)]
#[test]
fn apply_refuses_symlink_planted_at_backup_name() {
    let f = fixture("backup-link");
    let victim = f.config.with_file_name("victim");
    let planted = f
        .config
        .with_file_name("config.toml.kcal-20261105T120000123Z.bak");
    std::os::unix::fs::symlink(&victim, &planted).unwrap();
    let p = preview_of(&f, TARGET, false).unwrap();
    assert!(message(apply_with(&f, &p.before_hash).unwrap_err()).contains("백업"));
    assert!(!victim.exists());
    assert_eq!(std::fs::read_to_string(&f.config).unwrap(), CONFIG);
}

#[cfg(unix)]
#[test]
fn apply_keeps_file_permission_mode() {
    use std::os::unix::fs::PermissionsExt;
    let f = fixture("mode");
    std::fs::set_permissions(&f.config, std::fs::Permissions::from_mode(0o600)).unwrap();
    let p = preview_of(&f, TARGET, false).unwrap();
    let applied = apply_with(&f, &p.before_hash).unwrap();
    let mode_of = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode_of(&f.config), 0o600);
    assert_eq!(mode_of(Path::new(&applied.backup)), 0o600);
}

#[cfg(unix)]
#[test]
fn apply_writes_to_symlink_target() {
    let f = fixture("link");
    let real = f.home.write("dotfiles/config.toml", CONFIG);
    std::fs::remove_file(&f.config).unwrap();
    std::os::unix::fs::symlink(&real, &f.config).unwrap();
    let p = preview_of(&f, TARGET, false).unwrap();
    apply_with(&f, &p.before_hash).unwrap();
    assert!(
        std::fs::symlink_metadata(&f.config)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(
        std::fs::read_to_string(&real)
            .unwrap()
            .contains("enabled = false")
    );
}

#[test]
fn apply_refuses_manual_and_unknown_targets_and_bad_hash() {
    let f = fixture("refuse");
    let hash = edit::content_hash(CONFIG.as_bytes());
    let run = |target: &str, hash: &str| {
        apply(
            f.home.path(),
            None,
            None,
            &Tuning::default(),
            &f.log,
            target,
            false,
            hash,
            NOW,
        )
    };
    assert!(message(run("codex/plugin:missing@m", &hash).unwrap_err()).contains("목록에 없어요"));
    assert!(message(run(TARGET, "not-a-hash").unwrap_err()).contains("미리보기"));
    f.home
        .write(".claude.json", r#"{"mcpServers": {"docs": {}}}"#);
    assert!(message(run("claude/mcp:docs", &hash).unwrap_err()).contains("/mcp"));
    assert_eq!(std::fs::read_to_string(&f.config).unwrap(), CONFIG);
}

#[test]
fn undo_right_after_apply_restores_identical_bytes() {
    let f = fixture("undo");
    let p = preview_of(&f, TARGET, false).unwrap();
    let applied = apply_with(&f, &p.before_hash).unwrap();
    let undone = undo(
        f.home.path(),
        &f.log,
        &applied.id,
        NOW + time::Duration::SECOND,
    )
    .unwrap();
    assert_eq!(std::fs::read(&f.config).unwrap(), CONFIG.as_bytes());
    assert_eq!((undone.action, undone.enabled), (CoachAction::Undo, true));
    assert_eq!(undone.undo_of.as_deref(), Some(applied.id.as_str()));
    // 두 번 되돌리지 않는다.
    assert!(message(undo(f.home.path(), &f.log, &applied.id, NOW).unwrap_err()).contains("이미"));
}

#[test]
fn undo_refuses_when_file_changed_after_apply() {
    let f = fixture("undo-stale");
    let p = preview_of(&f, TARGET, false).unwrap();
    let applied = apply_with(&f, &p.before_hash).unwrap();
    let edited = format!("{}# later\n", std::fs::read_to_string(&f.config).unwrap());
    std::fs::write(&f.config, &edited).unwrap();
    let error = message(undo(f.home.path(), &f.log, &applied.id, NOW).unwrap_err());
    assert!(error.contains("바뀌어") && error.contains(&applied.undo_command));
    assert_eq!(std::fs::read_to_string(&f.config).unwrap(), edited);
}

#[test]
fn undo_refuses_record_with_backup_in_another_folder() {
    let f = fixture("undo-path");
    let p = preview_of(&f, TARGET, false).unwrap();
    let applied = apply_with(&f, &p.before_hash).unwrap();
    let elsewhere = f.home.write("elsewhere/config.toml.kcal-1.bak", CONFIG);
    let tampered = AppliedSwitch {
        id: "1".into(),
        backup: elsewhere.to_string_lossy().into_owned(),
        ..applied.clone()
    };
    log::append(&f.log, &tampered).unwrap();
    let after = std::fs::read(&f.config).unwrap();
    assert!(message(undo(f.home.path(), &f.log, "1", NOW).unwrap_err()).contains("경로"));
    // 원본 밖 파일을 가리키는 기록도 거부한다.
    let outside = AppliedSwitch {
        id: "2".into(),
        file: elsewhere
            .with_file_name("config.toml")
            .to_string_lossy()
            .into_owned(),
        ..applied
    };
    log::append(&f.log, &outside).unwrap();
    assert!(undo(f.home.path(), &f.log, "2", NOW).is_err());
    assert_eq!(std::fs::read(&f.config).unwrap(), after);
}

#[test]
fn kiro_and_claude_json_switches_apply_through_the_same_path() {
    let f = fixture("json");
    let kiro = f.home.write(
        ".kiro/settings/mcp.json",
        "{\n  \"mcpServers\": {\n    \"k\": {}\n  }\n}\n",
    );
    f.home.write(
        ".claude/settings.json",
        "{\n  \"enabledPlugins\": {\n    \"cp@m\": true\n  }\n}\n",
    );
    for (target, enabled) in [("kiro/mcp:k", false), ("claude/plugin:cp@m", false)] {
        let p = preview_of(&f, target, enabled).unwrap();
        apply(
            f.home.path(),
            None,
            None,
            &Tuning::default(),
            &f.log,
            target,
            enabled,
            &p.before_hash,
            NOW + time::Duration::milliseconds(target.len() as i64),
        )
        .unwrap();
    }
    assert!(
        std::fs::read_to_string(&kiro)
            .unwrap()
            .contains("\"disabled\": true")
    );
    assert_eq!(log::read(&f.log).len(), 2);
}

/// 이 Mac의 실제 설정을 읽기만 해서 스위치 목록·제안·첫 제안 미리보기를 출력한다(쓰기 없음).
/// `cargo test real_switches -- --ignored --nocapture`
#[test]
#[ignore]
fn real_switches_read_only() {
    let home = std::env::home_dir().unwrap();
    let tuning = Tuning::default();
    let snapshot = context::measure(&home, None, &std::env::vars().collect(), &tuning);
    let now = crate::metrics::unix_secs(OffsetDateTime::now_utc());
    let index = usage::read(&home, &tuning, now);
    let all = switches::list(&home, Some(&snapshot), Some(&index), &tuning);
    println!(
        "스위치 {}개 (제안 {}개)",
        all.len(),
        all.iter().filter(|s| s.is_proposed).count()
    );
    for s in &all {
        println!(
            "{} {:<50} {:>4} tokens={:<8} loading={:<12} format={:?}",
            if s.is_proposed { "제안" } else { "    " },
            s.target,
            if s.enabled { "켬" } else { "끔" },
            s.tokens.map_or("모름".into(), |t| t.to_string()),
            s.loading.map_or("-".into(), |l| format!("{l:?}")),
            s.format,
        );
    }
    if let Some(first) = all
        .iter()
        .find(|s| s.is_proposed && s.format != SwitchFormat::Manual)
    {
        match preview(
            &home,
            Some(&snapshot),
            Some(&index),
            &tuning,
            &first.target,
            false,
        ) {
            Ok(p) => println!(
                "\n미리보기 {}: {} | {:?} → {} | changed={}",
                p.target, p.location, p.before, p.after, p.changed
            ),
            Err(e) => println!("\n미리보기 {} 거부: {e}", first.target),
        }
    }
}

/// 이 Mac의 실제 Claude 기록으로 진단하기 사용 집계를 표로 출력한다(읽기만, 0022).
/// 플러그인·MCP 이름, 횟수, 프로젝트는 폴더 이름만. 홈 경로·기록 내용은 찍지 않는다.
/// `cargo test real_usage -- --ignored --nocapture`
#[test]
#[ignore]
fn real_usage_read_only() {
    let home = std::env::home_dir().unwrap();
    let tuning = Tuning::default();
    let started = std::time::Instant::now();
    let now = crate::metrics::unix_secs(OffsetDateTime::now_utc());
    let index = usage::read(&home, &tuning, now);
    let read_secs = started.elapsed().as_secs_f64();
    let snapshot = context::measure(&home, None, &std::env::vars().collect(), &tuning);
    let all = switches::list(&home, Some(&snapshot), Some(&index), &tuning);
    println!(
        "기간 {}일, 기록 다 봄: {}, 시작 폴더 {}곳, 읽기 {read_secs:.1}초",
        index.days,
        !index.is_partial,
        index.projects.len()
    );
    println!("| 스위치 | 사용자 범위 | 토큰 | 최근 사용 | 프로젝트별(위 3곳) | 판정 |");
    println!("| --- | --- | --- | --- | --- | --- |");
    for s in all
        .iter()
        .filter(|s| s.agent == crate::usage::Agent::Claude)
    {
        let Some(used) = &s.usage else { continue };
        let top = used
            .projects
            .iter()
            .map(|p| {
                let gone = if p.exists { "" } else { "(없음)" };
                format!("{}{gone} {}", p.name, p.count)
            })
            .collect::<Vec<_>>()
            .join(", ");
        let more = used.project_count.saturating_sub(used.projects.len());
        let verdict = match (&s.scope_move, s.is_proposed) {
            (_, true) => "끄기 제안".to_owned(),
            (Some(m), _) => format!(
                "프로젝트에서만 켜기 → {}{}",
                m.projects
                    .iter()
                    .map(|p| p.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                if m.skipped.is_empty() {
                    String::new()
                } else {
                    format!(" (건너뜀: {})", m.skipped.join(", "))
                }
            ),
            _ if used.total > 0 => "그대로(씀)".into(),
            _ => "-".into(),
        };
        let projects_on = s.scope.as_ref().map_or(String::new(), |scope| {
            scope
                .projects
                .iter()
                .map(|p| format!("{}:{}", p.name, if p.enabled { "켬" } else { "끔" }))
                .collect::<Vec<_>>()
                .join(", ")
        });
        println!(
            "| {} {} | {}{} | {} | {}회 | {}{} | {} |",
            match s.kind {
                switches::SwitchKind::Plugin => "플러그인",
                switches::SwitchKind::Mcp => "MCP",
            },
            s.name,
            if s.kind == switches::SwitchKind::Plugin {
                if s.enabled { "켬" } else { "끔" }
            } else {
                "-"
            },
            if projects_on.is_empty() {
                String::new()
            } else {
                format!(" (프로젝트 {projects_on})")
            },
            s.tokens.map_or("모름".into(), |t| t.to_string()),
            used.total,
            top,
            if more > 0 {
                format!(" 외 {more}곳")
            } else {
                String::new()
            },
            verdict,
        );
    }
}

// ---------- 쓰기 실패·기록 손상 ----------

#[test]
fn non_utf8_settings_file_is_refused_and_left_byte_for_byte() {
    let f = fixture("non-utf8");
    let bytes = b"[plugins.\"big@m\"]\nenabled = true\n# \xff\xfe\n".to_vec();
    std::fs::write(&f.config, &bytes).unwrap();
    let hash = edit::content_hash(&bytes);
    assert!(message(preview_of(&f, TARGET, false).unwrap_err()).contains("UTF-8"));
    assert!(message(apply_with(&f, &hash).unwrap_err()).contains("UTF-8"));
    assert_eq!(std::fs::read(&f.config).unwrap(), bytes);
    assert_eq!(
        std::fs::read_dir(f.config.parent().unwrap())
            .unwrap()
            .count(),
        1
    );
    assert!(log::read(&f.log).is_empty());
}

#[test]
fn apply_error_carries_the_manual_undo_command_when_the_log_cannot_be_written() {
    let f = fixture("log-fail");
    // 기록 경로가 폴더면 줄을 덧붙일 수 없다.
    std::fs::create_dir_all(&f.log).unwrap();
    let p = preview_of(&f, TARGET, false).unwrap();
    let error = message(apply_with(&f, &p.before_hash).unwrap_err());
    let backup = f
        .config
        .with_file_name("config.toml.kcal-20261105T120000123Z.bak");
    assert!(error.contains(&copy_command(&backup, &f.config)), "{error}");
    // 설정은 이미 바뀌었고 백업은 원본이다(안내한 명령이 실제로 되돌린다).
    assert!(
        std::fs::read_to_string(&f.config)
            .unwrap()
            .contains("enabled = false")
    );
    assert_eq!(std::fs::read_to_string(&backup).unwrap(), CONFIG);
}

#[test]
fn log_read_skips_a_corrupt_line_and_undo_still_works() {
    let f = fixture("log-corrupt");
    let p = preview_of(&f, TARGET, false).unwrap();
    let applied = apply_with(&f, &p.before_hash).unwrap();
    // 쓰다 끊긴 줄이 기록 가운데에 끼었다.
    std::fs::write(
        &f.log,
        format!(
            "{}{{\"id\":\"9\",\"at\":\n",
            std::fs::read_to_string(&f.log).unwrap()
        ),
    )
    .unwrap();
    let undone = undo(
        f.home.path(),
        &f.log,
        &applied.id,
        NOW + time::Duration::SECOND,
    )
    .unwrap();
    assert_eq!(std::fs::read(&f.config).unwrap(), CONFIG.as_bytes());
    assert_eq!(log::read(&f.log), [undone, applied]);
}
