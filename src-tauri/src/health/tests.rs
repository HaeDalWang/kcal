use time::macros::datetime;

use super::read::compaction_times;
use super::*;
use crate::context::fsutil::testing::TempDir;

const D: f64 = DAY_SECS;
const H: f64 = 3600.0;

fn samples(agent: Agent, requests: Vec<(f64, f64)>, used: Vec<(f64, f64)>) -> AgentSamples {
    AgentSamples {
        agent,
        requests,
        requests_from: Some(f64::NEG_INFINITY),
        used,
        used_from: Some(f64::NEG_INFINITY),
        compactions: (agent == Agent::Codex).then(Vec::new),
    }
}

fn applied(id: &str, at: OffsetDateTime, agent: Agent) -> AppliedSwitch {
    AppliedSwitch {
        id: id.into(),
        at,
        action: CoachAction::Apply,
        target: "codex/plugin:p@m".into(),
        agent,
        name: "p@m".into(),
        file: "/f".into(),
        backup: "/f.kcal-x.bak".into(),
        enabled: false,
        tokens: Some(900),
        after_hash: "0".into(),
        undo_command: "cp".into(),
        undo_of: None,
        created: None,
        files: Vec::new(),
    }
}

#[test]
fn period_includes_its_start_and_excludes_its_end() {
    let t = 100.0 * D;
    let s = samples(
        Agent::Claude,
        vec![
            (t - 7.0 * D, 10.0),
            (t - 1.0, 20.0),
            (t, 40.0),
            (t + 7.0 * D, 80.0),
        ],
        vec![(t - 7.0 * D, 7.0), (t, 14.0), (t + 7.0 * D, 99.0)],
    );
    let (before, after) = compare(&s, t, 7.0 * D, t + 7.0 * D);
    // t − 7일과 t − 1초는 전, t는 후, t + 7일은 어디에도 없다.
    assert_eq!(
        (before.requests, before.avg_input_tokens),
        (Some(2), Some(15.0))
    );
    assert_eq!(
        (after.requests, after.avg_input_tokens),
        (Some(1), Some(40.0))
    );
    assert_eq!(
        (before.daily_used, after.daily_used),
        (Some(1.0), Some(2.0))
    );
    assert_eq!(
        (before.from, before.to, after.from),
        (at(t - 7.0 * D), at(t), at(t))
    );
    assert_eq!((before.coverage, after.coverage), (Some(1.0), Some(1.0)));
    assert_eq!((before.avg_credits, before.compactions), (None, None));
}

#[test]
fn unfinished_after_period_divides_by_the_days_elapsed_so_far() {
    let t = 100.0 * D;
    let now = t + 2.5 * D;
    let s = samples(
        Agent::Kiro,
        vec![(t + H, 1.0), (t + D, 4.0)],
        vec![(t + H, 1.0), (t + D, 4.0)],
    );
    let (_, after) = compare(&s, t, 7.0 * D, now);
    assert_eq!(after.elapsed_days, 2.5);
    assert_eq!(after.daily_used, Some(5.0 / 2.5));
    assert_eq!(
        (after.avg_credits, after.avg_input_tokens),
        (Some(2.5), None)
    );
    assert_eq!(after.used_unit, BarUnit::Credits);
}

#[test]
fn coverage_is_partial_when_records_start_late_and_null_without_records() {
    let t = 100.0 * D;
    let late = AgentSamples {
        requests_from: Some(t - 2.0 * D),
        used_from: Some(t - 1.0 * D),
        ..samples(Agent::Claude, vec![(t - H, 100.0)], vec![(t - H, 3.0)])
    };
    let before = period(&late, t - 4.0 * D, t);
    // 요청은 4일 중 2일, %p는 1일을 덮는다 → 작은 쪽. 하루 평균은 %p 기록이 덮은 1일로 나눈다.
    assert_eq!(before.coverage, Some(0.25));
    assert_eq!((before.requests, before.daily_used), (Some(1), Some(3.0)));
    // 기록이 구간 뒤에야 시작하면 값은 모두 null(0이 아니다).
    let after_start = period(&late, t - 10.0 * D, t - 5.0 * D);
    assert_eq!(after_start.coverage, Some(0.0));
    assert_eq!((after_start.requests, after_start.daily_used), (None, None));
    let none = AgentSamples {
        requests_from: None,
        used_from: None,
        ..samples(Agent::Codex, vec![], vec![])
    };
    let got = period(&none, t - D, t);
    assert_eq!(
        (got.coverage, got.requests, got.daily_used, got.compactions),
        (None, None, None, None)
    );
    // %p 기록만 없는 Claude: 요청 값은 있고 하루 %p만 null.
    let no_history = AgentSamples {
        used_from: None,
        ..samples(Agent::Claude, vec![(t - H, 50.0)], vec![])
    };
    let got = period(&no_history, t - D, t);
    assert_eq!(
        (got.coverage, got.avg_input_tokens, got.daily_used),
        (Some(1.0), Some(50.0), None)
    );
}

#[test]
fn undone_prescription_cuts_its_after_period_at_the_undo_time() {
    let now = datetime!(2026-11-06 12:00 UTC);
    let t = datetime!(2026-11-02 12:00 UTC);
    let undo_at = datetime!(2026-11-03 12:00 UTC);
    let apply = applied("1", t, Agent::Codex);
    let undo = AppliedSwitch {
        id: "2".into(),
        at: undo_at,
        action: CoachAction::Undo,
        undo_of: Some("1".into()),
        ..apply.clone()
    };
    let other = applied("3", datetime!(2026-11-05 12:00 UTC), Agent::Kiro);
    let ts = unix_secs(t);
    let s = AgentSamples {
        compactions: Some(vec![ts + H, ts + 2.0 * D]),
        ..samples(
            Agent::Codex,
            vec![(ts + H, 10.0), (ts + 2.0 * D, 99.0)],
            vec![],
        )
    };
    let got = report(&[s], &[other, undo, apply], now, 7);
    // Kiro 기록은 읽은 에이전트가 아니라 빠진다. undo 기록은 처방 카드가 아니다.
    assert_eq!(got.prescriptions.len(), 1);
    let p = &got.prescriptions[0];
    assert_eq!((p.id.as_str(), p.undone_at), ("1", Some(undo_at)));
    assert_eq!((p.after.to, p.after.elapsed_days), (undo_at, 1.0));
    assert_eq!((p.after.requests, p.after.compactions), (Some(1), Some(1)));
    // 주간 요약: t = 지금 − 7일.
    assert_eq!(got.weekly[0].after.from, datetime!(2026-10-30 12:00 UTC));
    assert_eq!(got.weekly[0].after.to, now);
    // 가장 오래된 적용 − 7일이 주간 요약 전 구간(지금 − 14일)보다 늦으면 지금 − 14일부터 읽는다.
    let log = [applied("1", t, Agent::Codex)];
    assert_eq!(
        read_start(&log, unix_secs(now), 7),
        unix_secs(now) - 14.0 * D
    );
    let old = [applied("1", datetime!(2026-10-01 00:00 UTC), Agent::Codex)];
    assert_eq!(
        read_start(&old, unix_secs(now), 7),
        unix_secs(datetime!(2026-09-24 00:00 UTC))
    );
}

#[test]
fn counts_codex_compacted_lines_once_and_ignores_lookalikes() {
    let home = TempDir::new("health-compacted");
    let compacted = |ts: &str| {
        format!(
            r#"{{"timestamp":"{ts}","type":"compacted","payload":{{"message":"secret","replacement_history":[]}}}}"#
        )
    };
    let lines = [
        r#"{"timestamp":"2026-11-03T00:00:00Z","type":"session_meta","payload":{}}"#.to_owned(),
        compacted("2026-11-03T05:51:31.889Z"),
        // 본문 안의 같은 문자열(이스케이프됨)은 압축이 아니다.
        r#"{"timestamp":"2026-11-03T06:00:00Z","type":"response_item","payload":{"text":"\"type\":\"compacted\""}}"#.to_owned(),
        r#"{"timestamp":"2026-11-03T06:30:00Z","type":"event_msg","payload":{"type":"compacted"}}"#.to_owned(),
        compacted("2026-11-03T07:36:35.794Z"),
        "{broken".to_owned(),
    ];
    let a = home.write(
        ".codex/sessions/2026/11/03/rollout-a.jsonl",
        &lines.join("\n"),
    );
    // fork한 rollout은 앞 기록을 그대로 담는다 → 같은 시각은 한 번만.
    home.write(
        ".codex/sessions/2026/11/03/rollout-b.jsonl",
        &lines[..2].join("\n"),
    );
    let got: Vec<f64> = compaction_times(&a).into_iter().map(|c| c.1).collect();
    assert_eq!(
        got,
        [
            unix_secs(datetime!(2026-11-03 05:51:31.889 UTC)),
            unix_secs(datetime!(2026-11-03 07:36:35.794 UTC))
        ]
    );
    let s = read::samples(
        home.path(),
        &home.path().join("none"),
        Agent::Codex,
        0.0,
        &Default::default(),
    );
    assert_eq!(s.compactions.as_ref().map(Vec::len), Some(2));
    // token_count가 없으면 요청 기록이 없다 → 압축 횟수도 null(0이 아니다).
    let t = unix_secs(datetime!(2026-11-04 00:00 UTC));
    assert_eq!(s.requests_from, None);
    assert_eq!(period(&s, t - 7.0 * D, t).compactions, None);
    // 요청 기록이 있으면 압축 시각을 구간으로 센다.
    let with_requests = AgentSamples {
        requests_from: Some(f64::NEG_INFINITY),
        ..s
    };
    assert_eq!(period(&with_requests, t - 7.0 * D, t).compactions, Some(2));
}

#[test]
fn compaction_times_reads_zst_rollouts() {
    let home = TempDir::new("compact-zst");
    let line = r#"{"timestamp":"2026-11-03T05:51:31.889Z","type":"compacted","payload":{}}"#;
    let path = home.path().join("rollout-z.jsonl.zst");
    std::fs::write(
        &path,
        ruzstd::encoding::compress_to_vec(
            line.as_bytes(),
            ruzstd::encoding::CompressionLevel::Fastest,
        ),
    )
    .unwrap();
    assert_eq!(compaction_times(&path).len(), 1);
}

/// 이 Mac의 실제 기록으로 주간 요약을 낸다(숫자만). 결과를 `/tmp/why-limit-burned.real-data.log`에 쓴다.
/// `cargo test -- --ignored real_health_on_this_mac --nocapture`.
#[test]
#[ignore = "실제 ~/.codex·~/.claude·~/.kiro와 한도·적용 기록을 읽는다"]
fn real_health_on_this_mac() {
    let home = std::env::home_dir().unwrap();
    let data = home.join("Library/Application Support/io.github.haedalwang.kcal");
    let history = crate::rate::history::file(&data);
    let log = log::read(&log::file(&data));
    let settings = Settings::default();
    let started = std::time::Instant::now();
    let report = compute(&home, &history, &log, &settings);
    let mut out = format!(
        "HEALTH computed_at={} compare_days={} elapsed={:.1}s prescriptions={}\n",
        report.computed_at,
        report.compare_days,
        started.elapsed().as_secs_f64(),
        report.prescriptions.len()
    );
    let line = |label: &str, p: &PeriodStats| {
        format!(
            "  {label}: {}..{} days={:.2} coverage={:?} requests={:?} avg_input_tokens={:?} avg_credits={:?} daily_used={:?}({:?}) compactions={:?}\n",
            p.from,
            p.to,
            p.elapsed_days,
            p.coverage.map(|c| (c * 1000.0).round() / 1000.0),
            p.requests,
            p.avg_input_tokens.map(f64::round),
            p.avg_credits.map(|c| (c * 1000.0).round() / 1000.0),
            p.daily_used.map(|u| (u * 100.0).round() / 100.0),
            p.used_unit,
            p.compactions
        )
    };
    out += &format!(
        "WEEKLY_DETAILS {}\n",
        serde_json::to_string(&report.weekly).unwrap()
    );
    for c in &report.weekly {
        out += &format!("WEEKLY {:?}\n", c.agent);
        out += &line("before", &c.before);
        out += &line("after", &c.after);
    }
    for p in &report.prescriptions {
        out += &format!(
            "PRESCRIPTION {:?} at={} undone_at={:?}\n",
            p.agent, p.at, p.undone_at
        );
        out += &line("before", &p.before);
        out += &line("after", &p.after);
    }
    print!("{out}");
    std::fs::write("/tmp/why-limit-burned.real-data.log", out).unwrap();
}
