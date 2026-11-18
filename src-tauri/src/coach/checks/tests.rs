//! 점검 판정·적용·되돌리기 테스트. 모두 임시 폴더를 HOME으로 쓴다(실제 설정 파일을 쓰지 않는다).

use super::models::{ModelCount, RecentRecords, normalize, parse};
use super::*;
use crate::coach::{log, undo};
use crate::context::fsutil::testing::TempDir;
use time::macros::datetime;

const NOW: OffsetDateTime = datetime!(2026-11-07 12:00:00.000 UTC);

pub(super) fn records(models: &[(&str, u64)]) -> RecentRecords {
    RecentRecords {
        days: 30,
        known: models
            .iter()
            .map(|(id, n)| {
                let model = parse(&normalize(id)).unwrap();
                (
                    ModelCount {
                        model: (*id).into(),
                        requests: *n,
                    },
                    model,
                )
            })
            .collect(),
        unknown: vec![],
        workspace: None,
        workspaces: vec![],
    }
}

/// 이 Mac(2026-11-07)과 같은 모델 구성.
pub(super) const THIS_MAC: [(&str, u64); 5] = [
    ("claude-opus-5-5", 22759),
    ("claude-sonnet-5-5", 540),
    ("claude-opus-5", 170),
    ("claude-haiku-4-5", 19),
    ("claude-haiku-5-5", 16),
];

pub(super) const MAC_SETTINGS: &str = "{\n  \"model\": \"opus\",\n  \"effortLevel\": \"high\",\n  \"modelSettings\": {\n    \"claude-opus-5\": {\n      \"effortLevel\": \"high\"\n    },\n    \"claude-opus-5-5\": {\n      \"effortLevel\": \"xhigh\",\n      \"autoCompactWindow\": 400000\n    },\n    \"claude-sonnet-5-5\": {\n      \"effortLevel\": \"medium\"\n    }\n  }\n}\n";

pub(super) fn home_with(name: &str, settings: &str) -> TempDir {
    let home = TempDir::new(&format!("checks-{name}"));
    home.write(".claude/settings.json", settings);
    home
}

pub(super) fn checks(home: &TempDir, records: &RecentRecords) -> Vec<CoachCheck> {
    list(home.path(), records, &Tuning::default())
}

pub(super) fn find_check<'a>(all: &'a [CoachCheck], id: &str) -> Option<&'a CoachCheck> {
    all.iter().find(|c| c.id == id)
}

fn option_ids(check: &CoachCheck) -> Vec<(&str, bool, bool)> {
    check
        .options
        .iter()
        .map(|o| (o.id.as_str(), o.is_recommended, o.is_writable))
        .collect()
}

pub(super) fn message(error: AppError) -> String {
    match error {
        AppError::InvalidInput(m) => m,
        AppError::Internal => "internal".into(),
    }
}

fn settings_file(home: &TempDir) -> PathBuf {
    home.path().join(".claude/settings.json")
}

/// 미리보기 → 적용 → 되돌리기. 적용 뒤 파일을 돌려주고, 되돌린 뒤 원본과 바이트가 같은지 확인한다.
fn round_trip(home: &TempDir, records: &RecentRecords, id: &str, option: &str) -> Value {
    // 같은 홈에서 여러 번 적용해도 백업 이름(밀리초 시각)이 겹치지 않게 호출마다 시각을 민다.
    static CALLS: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
    let now =
        NOW + time::Duration::minutes(CALLS.fetch_add(1, std::sync::atomic::Ordering::SeqCst));
    let file = settings_file(home);
    let original = std::fs::read(&file).unwrap();
    let log_file = home.path().join("app-data").join(log::FILE_NAME);
    let tuning = Tuning::default();
    let p = preview(home.path(), records, &tuning, id, option).unwrap();
    assert!(!p.changes.is_empty(), "{id}:{option}");
    let applied = apply(
        home.path(),
        records,
        &tuning,
        &log_file,
        id,
        option,
        &p.before_hash,
        now,
    )
    .unwrap();
    assert_eq!(applied.target, format!("claude/check:{id}:{option}"));
    let after: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    undo(
        home.path(),
        &log_file,
        &applied.id,
        now + time::Duration::SECOND,
    )
    .unwrap();
    assert_eq!(
        std::fs::read(&file).unwrap(),
        original,
        "{id}:{option} undo"
    );
    after
}

// ---------- D ----------

#[test]
fn effort_level_on_this_mac_is_ignored_by_5_5_models_and_moves_nothing() {
    let home = home_with("effort-mac", MAC_SETTINGS);
    let all = checks(&home, &records(&THIS_MAC));
    let check = find_check(&all, "D-claude-effort-level").unwrap();
    let CheckFacts::EffortLevel {
        value,
        ignored_by,
        moved_to,
        current,
        models,
        ..
    } = &check.facts
    else {
        panic!("{:?}", check.facts)
    };
    assert_eq!(*value, Effort::High);
    assert_eq!(
        ignored_by,
        &["claude-opus-5-5", "claude-sonnet-5-5", "claude-haiku-5-5"]
    );
    assert!(moved_to.is_empty());
    assert_eq!(current, "claude-opus-5-5");
    assert_eq!(models[0].own_effort, Some(Effort::Xhigh));
    // 지금 모델에 자기 값이 있어 "지금 모델에 저장" 옵션은 없다.
    assert_eq!(option_ids(check), [("remove", true, true)]);
    let after = round_trip(
        &home,
        &records(&THIS_MAC),
        "D-claude-effort-level",
        "remove",
    );
    let mut expected: Value = serde_json::from_str(MAC_SETTINGS).unwrap();
    expected
        .as_object_mut()
        .unwrap()
        .shift_remove("effortLevel");
    assert_eq!(after, expected);
}

#[test]
fn effort_level_moves_value_to_models_where_it_still_applies_and_offers_saving_current() {
    let home = home_with("effort-move", "{\n  \"effortLevel\": \"low\"\n}\n");
    let recent = records(&[
        ("claude-haiku-5-5", 50),
        ("claude-opus-5", 10),
        ("claude-opus-4-7", 1),
    ]);
    let all = checks(&home, &recent);
    let check = find_check(&all, "D-claude-effort-level").unwrap();
    let CheckFacts::EffortLevel {
        ignored_by,
        moved_to,
        ..
    } = &check.facts
    else {
        panic!()
    };
    assert_eq!(ignored_by, &["claude-haiku-5-5"]);
    assert_eq!(moved_to, &["claude-opus-5", "claude-opus-4-7"]);
    assert_eq!(
        option_ids(check),
        [
            ("remove", true, true),
            ("remove-and-save-current", false, true)
        ]
    );
    let after = round_trip(&home, &recent, "D-claude-effort-level", "remove");
    assert_eq!(
        after,
        serde_json::json!({"modelSettings": {"claude-opus-5": {"effortLevel": "low"}, "claude-opus-4-7": {"effortLevel": "low"}}})
    );
    let after = round_trip(
        &home,
        &recent,
        "D-claude-effort-level",
        "remove-and-save-current",
    );
    assert_eq!(
        after["modelSettings"]["claude-haiku-5-5"]["effortLevel"],
        "low"
    );
    assert!(after.get("effortLevel").is_none());
}

#[test]
fn effort_level_is_not_flagged_without_ignoring_models_env_override_or_records() {
    let home = home_with("effort-none", "{\n  \"effortLevel\": \"high\"\n}\n");
    assert!(
        find_check(
            &checks(&home, &records(&[("claude-opus-5", 9)])),
            "D-claude-effort-level"
        )
        .is_none()
    );
    assert!(find_check(&checks(&home, &records(&[])), "D-claude-effort-level").is_none());
    let env = home_with(
        "effort-env",
        "{\"effortLevel\": \"high\", \"env\": {\"CLAUDE_CODE_EFFORT_LEVEL\": \"low\"}}",
    );
    assert!(find_check(&checks(&env, &records(&THIS_MAC)), "D-claude-effort-level").is_none());
}

#[test]
fn always_thinking_off_recommends_keep_when_some_recent_model_still_obeys_it() {
    let home = home_with("think", "{\n  \"alwaysThinkingEnabled\": false\n}\n");
    let both = records(&[("claude-opus-5-5", 9), ("claude-opus-5", 1)]);
    let all = checks(&home, &both);
    let check = find_check(&all, "D-claude-always-thinking").unwrap();
    let CheckFacts::AlwaysThinking {
        ignored_by,
        affected,
        ..
    } = &check.facts
    else {
        panic!()
    };
    assert_eq!(
        (ignored_by.as_slice(), affected.as_slice()),
        (
            &["claude-opus-5-5".to_owned()][..],
            &["claude-opus-5".to_owned()][..]
        )
    );
    assert_eq!(
        option_ids(check),
        [("remove", false, true), ("keep", true, false)]
    );
    let only_new = records(&[("claude-opus-5-5", 9)]);
    let all = checks(&home, &only_new);
    assert_eq!(
        option_ids(find_check(&all, "D-claude-always-thinking").unwrap())[0],
        ("remove", true, true)
    );
    assert_eq!(
        round_trip(&home, &only_new, "D-claude-always-thinking", "remove"),
        serde_json::json!({})
    );
    assert!(
        find_check(
            &checks(&home, &records(&[("claude-opus-5", 1)])),
            "D-claude-always-thinking"
        )
        .is_none()
    );
    let on = home_with("think-on", "{\"alwaysThinkingEnabled\": true}");
    assert!(find_check(&checks(&on, &both), "D-claude-always-thinking").is_none());
}

#[test]
fn max_thinking_tokens_zero_and_positive_follow_their_own_rules() {
    let zero = home_with(
        "mtt-zero",
        "{\n  \"env\": {\n    \"MAX_THINKING_TOKENS\": \"0\",\n    \"A\": \"1\"\n  }\n}\n",
    );
    let recent = records(&[("claude-haiku-5-5", 3)]);
    assert!(find_check(&checks(&zero, &recent), "D-claude-max-thinking-tokens").is_some());
    let after = round_trip(&zero, &recent, "D-claude-max-thinking-tokens", "remove");
    assert_eq!(after, serde_json::json!({"env": {"A": "1"}}));

    let positive = home_with("mtt-pos", "{\"env\": {\"MAX_THINKING_TOKENS\": \"8000\"}}");
    let old = records(&[("claude-opus-4-6", 3), ("claude-haiku-4-5", 1)]);
    let all = checks(&positive, &old);
    let check = find_check(&all, "D-claude-max-thinking-tokens").unwrap();
    let CheckFacts::MaxThinkingTokens {
        value,
        ignored_by,
        affected,
        ..
    } = &check.facts
    else {
        panic!()
    };
    assert_eq!(
        (*value, ignored_by.len(), affected.as_slice()),
        (8000, 1, &["claude-haiku-4-5".to_owned()][..])
    );
    let adaptive_off = home_with(
        "mtt-adaptive",
        "{\"env\": {\"MAX_THINKING_TOKENS\": \"8000\", \"CLAUDE_CODE_DISABLE_ADAPTIVE_THINKING\": \"1\"}}",
    );
    assert!(
        find_check(
            &checks(&adaptive_off, &records(&[("claude-opus-4-6", 3)])),
            "D-claude-max-thinking-tokens"
        )
        .is_none()
    );
    let text = home_with("mtt-text", "{\"env\": {\"MAX_THINKING_TOKENS\": \"lots\"}}");
    assert!(find_check(&checks(&text, &recent), "D-claude-max-thinking-tokens").is_none());
}

// ---------- C ----------

pub(super) const GIT_RULE: &str =
    "# Git Workflow\n\n## Commit Message Format\n\nTypes: feat, fix\n";

#[path = "cards_tests.rs"]
mod cards;

// ---------- 적용 경계 ----------

#[test]
fn apply_refuses_keep_unknown_ids_and_options_not_offered() {
    let home = home_with("refuse", MAC_SETTINGS);
    home.write(".claude/rules/git.md", GIT_RULE);
    let recent = records(&THIS_MAC);
    let tuning = Tuning::default();
    let log_file = home.path().join("log.jsonl");
    let run = |id: &str, option: &str, hash: &str| {
        apply(
            home.path(),
            &recent,
            &tuning,
            &log_file,
            id,
            option,
            hash,
            NOW,
        )
        .map_err(message)
    };
    let hash = edit::content_hash(MAC_SETTINGS.as_bytes());
    assert!(
        run("C-claude-git-instructions", "keep", &hash)
            .unwrap_err()
            .contains("그대로")
    );
    assert!(
        preview(
            home.path(),
            &recent,
            &tuning,
            "C-claude-git-instructions",
            "keep"
        )
        .is_err()
    );
    assert!(
        run("D-claude-missing", "remove", &hash)
            .unwrap_err()
            .contains("목록")
    );
    assert!(
        run("D-claude-effort-level", "remove-and-save-current", &hash)
            .unwrap_err()
            .contains("없는 선택")
    );
}

#[test]
fn apply_refuses_and_writes_nothing_when_settings_changed_after_preview() {
    let home = home_with("stale", MAC_SETTINGS);
    let recent = records(&THIS_MAC);
    let tuning = Tuning::default();
    let log_file = home.path().join("log.jsonl");
    let run = |id: &str, option: &str, hash: &str| {
        apply(
            home.path(),
            &recent,
            &tuning,
            &log_file,
            id,
            option,
            hash,
            NOW,
        )
        .map_err(message)
    };
    let p = preview(
        home.path(),
        &recent,
        &tuning,
        "D-claude-effort-level",
        "remove",
    )
    .unwrap();
    let changed = MAC_SETTINGS.replace("\"opus\"", "\"sonnet\"");
    std::fs::write(settings_file(&home), &changed).unwrap();
    assert!(
        run("D-claude-effort-level", "remove", &p.before_hash)
            .unwrap_err()
            .contains("바뀌었어요")
    );
    assert_eq!(
        std::fs::read_to_string(settings_file(&home)).unwrap(),
        changed
    );
    assert!(log::read(&log_file).is_empty());
}

#[test]
fn applied_check_shows_up_in_prescription_effects_by_name() {
    let home = home_with("health", MAC_SETTINGS);
    let recent = records(&THIS_MAC);
    let tuning = Tuning::default();
    let log_file = home.path().join("log.jsonl");
    let p = preview(
        home.path(),
        &recent,
        &tuning,
        "D-claude-effort-level",
        "remove",
    )
    .unwrap();
    apply(
        home.path(),
        &recent,
        &tuning,
        &log_file,
        "D-claude-effort-level",
        "remove",
        &p.before_hash,
        NOW,
    )
    .unwrap();
    let samples = [crate::health::AgentSamples {
        agent: Agent::Claude,
        requests: vec![],
        requests_from: None,
        used: vec![],
        used_from: None,
        compactions: None,
    }];
    let report = crate::health::report(
        &samples,
        &log::read(&log_file),
        NOW + time::Duration::DAY,
        7,
    );
    let names: Vec<(&str, &str)> = report
        .prescriptions
        .iter()
        .map(|p| (p.name.as_str(), p.target.as_str()))
        .collect();
    assert_eq!(
        names,
        [(
            "생각 깊이 옛 설정 정리",
            "claude/check:D-claude-effort-level:remove"
        )]
    );
    assert!(names[0].1.starts_with(CHECK_TARGET_PREFIX));
}

#[test]
fn reads_recent_model_counts_from_transcripts() {
    let home = TempDir::new("checks-records");
    let line = |id: &str, model: &str| {
        serde_json::json!({"type": "assistant", "timestamp": "2026-11-07T11:00:00Z",
            "message": {"id": id, "model": model, "usage": {"input_tokens": 1}}})
        .to_string()
    };
    home.write(
        ".claude/projects/-w/s.jsonl",
        &[
            line("a", "claude-opus-5-5"),
            line("a", "claude-opus-5-5"),
            line("d", "claude-opus-5-5[1m]"),
            line("b", "<synthetic>"),
            line("c", "claude-haiku-4-5-20251001"),
        ]
        .join("\n"),
    );
    let got = read_records(
        home.path(),
        &Tuning::default(),
        crate::metrics::unix_secs(NOW),
    );
    let counts: Vec<(&str, u64)> = got
        .known
        .iter()
        .map(|(c, _)| (c.model.as_str(), c.requests))
        .collect();
    assert_eq!(counts, [("claude-opus-5-5", 2), ("claude-haiku-4-5", 1)]);
    assert_eq!(got.days, 30);
}

/// 이 Mac의 실제 Claude 기록(최근 `checkModelDays`일)을 읽는 시간을 단계별로 잰다(숫자만, 기록 내용은 쓰지 않는다).
/// 결과를 `/tmp/kcal-checks-timing-<debug|release>.txt`에 한 줄 덧붙인다.
/// `cargo test [--release] --lib real_checks_records_timing -- --ignored --nocapture`.
#[test]
#[ignore = "실제 ~/.claude/projects를 읽는다(읽기만)"]
fn real_checks_records_timing_on_this_mac() {
    use crate::metrics::claude::{log_files, model_requests, read_lines};
    use std::io::Write;
    use std::time::Instant;

    let home = std::env::home_dir().unwrap();
    let tuning = Tuning::default();
    let since = now_secs() - tuning.check_model_days as f64 * crate::metrics::DAY_SECS;
    let load = std::process::Command::new("uptime")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default();
    let secs = |t: Instant| (t.elapsed().as_secs_f64() * 1000.0).round() / 1000.0;

    let t = Instant::now();
    let first = read_records(&home, &tuning, now_secs());
    let a_cold = secs(t);
    let t = Instant::now();
    let files = log_files(&home, since, usize::MAX);
    let b_walk = secs(t);
    let bytes: u64 = files
        .iter()
        .filter_map(|f| std::fs::metadata(f).ok())
        .map(|m| m.len())
        .sum();
    let lines = std::cell::Cell::new(0u64);
    let t = Instant::now();
    for f in &files {
        read_lines(f, |_| {
            lines.set(lines.get() + 1);
            None::<()>
        });
    }
    let c_read = secs(t);
    let t = Instant::now();
    let (requests, _) = model_requests(&home, since);
    let d_model_requests = secs(t);
    let t = Instant::now();
    let shape = shape::latest(&home, None, tuning.shape_sessions_to_search);
    let e_shape = secs(t);
    let t = Instant::now();
    let _ = read_records(&home, &tuning, now_secs());
    let a_warm = secs(t);

    let variant = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let out = format!(
        "{variant} load=\"{load}\" days={} files={} bytes={bytes} lines={} requests={} models={} \
         a_cold={a_cold}s a_warm={a_warm}s b_walk={b_walk}s c_read={c_read}s d_model_requests={d_model_requests}s \
         e_shape={e_shape}s shape_found={}\n",
        tuning.check_model_days,
        files.len(),
        lines.get(),
        requests.len(),
        first.known.len() + first.unknown.len(),
        shape.is_some(),
    );
    print!("{out}");
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(format!("/tmp/kcal-checks-timing-{variant}.txt"))
        .unwrap()
        .write_all(out.as_bytes())
        .unwrap();
}
