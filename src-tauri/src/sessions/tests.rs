use super::*;
use crate::context::fsutil::testing::TempDir;
use std::cell::RefCell;

#[test]
fn parses_optional_fields_and_rejects_garbage() {
    let good =
        parse_agents(br#"[{"pid":42,"sessionId":"abc","status":"idle","extra":true}]"#).unwrap();
    assert_eq!(good[0].pid, Some(42));
    assert_eq!(good[0].cwd, None);
    assert_eq!(parse_agents(b"[{}]").unwrap()[0].status, None);
    assert!(parse_agents(b"garbage").is_err());
    // Real 2.1.331 shape: startedAt is epoch ms. A retyped field must not drop the row.
    let real = parse_agents(
        br#"[{"pid":7,"cwd":"/w","kind":"interactive","startedAt":1795564623264,"sessionId":"s","status":"idle"},{"pid":"x","status":3}]"#,
    )
    .unwrap();
    assert_eq!(real[0].started_at, Some(1_795_564_623_264));
    assert_eq!(real[0].status.as_deref(), Some("idle"));
    assert_eq!((real[1].pid, real[1].status.as_deref()), (None, None));
    assert!(parse_agents(b"{}").is_err());
    assert_eq!(parse_agents(br#"[{"pid":"oops"}]"#).unwrap()[0].pid, None);
}

#[test]
fn age_uses_last_timestamp_and_never_mtime() {
    let home = TempDir::new("session-age");
    home.write(
        ".claude/projects/p/abc.jsonl",
        "{\"timestamp\":\"2026-11-24T00:00:00Z\"}\n{\"timestamp\":\"2026-11-25T00:00:00Z\"}\n{}\n",
    );
    let now = OffsetDateTime::parse("2026-11-26T00:00:00Z", &Rfc3339)
        .unwrap()
        .unix_timestamp();
    assert_eq!(idle_seconds(&home.0, "abc", now), Some(86400));
    assert_eq!(idle_seconds(&home.0, "abc", now - 86401), None);
    assert_eq!(idle_seconds(&home.0, "missing", now), None);
    assert_eq!(idle_seconds(&home.0, "../abc", now), None);
    home.write(".claude/projects/p/abc.jsonl", "{\"timestamp\":\"bad\"}\n");
    assert_eq!(idle_seconds(&home.0, "abc", now), None);
}

#[test]
fn age_takes_newest_of_duplicate_transcripts() {
    let home = TempDir::new("session-age-dup");
    home.write(
        ".claude/projects/a/dup.jsonl",
        "{\"timestamp\":\"2026-11-20T00:00:00Z\"}\n",
    );
    home.write(
        ".claude/projects/b/dup.jsonl",
        "{\"timestamp\":\"2026-11-25T12:00:00Z\"}\n",
    );
    home.write(
        ".claude/projects/c/dup.jsonl",
        "{\"timestamp\":\"2026-11-19T00:00:00Z\"}\n",
    );
    let now = OffsetDateTime::parse("2026-11-26T00:00:00Z", &Rfc3339)
        .unwrap()
        .unix_timestamp();
    assert_eq!(idle_seconds(&home.0, "dup", now), Some(12 * 3600));
}

#[test]
fn missing_protected_folder_does_not_block_others() {
    let home = TempDir::new("session-protect-missing");
    let s = session(&home.0);
    let mut t = Tuning {
        session_protected_folders: vec![home.0.join("gone").display().to_string()],
        ..Tuning::default()
    };
    let ps = parse_processes("42 1 0.0").unwrap();
    assert_eq!(exclusion(&s, Some(&ps), &t), None);
    t.session_protected_folders
        .push(home.0.display().to_string());
    assert_eq!(exclusion(&s, Some(&ps), &t), Some("protected_folder"));
}

fn session(home: &Path) -> ClaudeSession {
    ClaudeSession {
        pid: Some(42),
        session_id: Some("abc".into()),
        status: Some("idle".into()),
        cwd: Some(home.display().to_string()),
        idle_seconds: Some(86400),
        ..Default::default()
    }
}
fn processes() -> Vec<Process> {
    parse_processes("42 1 0.0\n43 42 0.8\n44 43 1.2\n45 1 99.0").unwrap()
}

#[test]
fn excludes_busy_waiting_unknown_age_protected_paths_and_descendant_cpu() {
    let home = TempDir::new("session-exclusions");
    let mut s = session(&home.0);
    let mut t = Tuning::default();
    let mut ps = processes();
    assert_eq!(child_cpu(42, &ps), 2.0);
    assert_eq!(exclusion(&s, Some(&ps), &t), Some("child_cpu"));
    ps[2].cpu = 1.1;
    assert_eq!(exclusion(&s, Some(&ps), &t), None);
    for status in ["busy", "waiting", "unknown"] {
        s.status = Some(status.into());
        assert_eq!(exclusion(&s, Some(&ps), &t), Some("not_idle"));
    }
    s.status = Some("idle".into());
    s.idle_seconds = None;
    assert_eq!(
        exclusion(&s, Some(&ps), &t),
        Some("unknown_or_recent_activity")
    );
    s.idle_seconds = Some(86400);
    assert_eq!(exclusion(&s, None, &t), Some("unknown_cpu"));
    t.session_protected_folders = vec![home.0.parent().unwrap().display().to_string()];
    assert_eq!(exclusion(&s, Some(&ps), &t), Some("protected_folder"));
    assert!(parse_processes("42 1 NaN").is_none());
    assert!(parse_processes("bad").is_none());
}

struct Fake {
    snapshots: RefCell<Vec<Vec<ClaudeSession>>>,
    killed: RefCell<Vec<u32>>,
}
impl Processes for Fake {
    fn agents(&self) -> Result<Vec<ClaudeSession>, AppError> {
        Ok(self.snapshots.borrow_mut().remove(0))
    }
    fn processes(&self) -> Option<Vec<Process>> {
        Some(parse_processes("42 1 0").unwrap())
    }
    fn terminate(&self, pid: u32) -> Result<(), AppError> {
        self.killed.borrow_mut().push(pid);
        Ok(())
    }
}

#[test]
fn rechecks_identity_and_idle_immediately_before_fake_signal() {
    let home = TempDir::new("session-recheck");
    home.write(
        ".claude/projects/p/abc.jsonl",
        "{\"timestamp\":\"2020-01-01T00:00:00Z\"}\n",
    );
    let s = session(&home.0);
    for change in ["identity", "busy", "gone", "unchanged"] {
        let mut fresh = s.clone();
        if change == "identity" {
            fresh.session_id = Some("reused".into());
        }
        if change == "busy" {
            fresh.status = Some("busy".into());
        }
        let fake = Fake {
            snapshots: RefCell::new(vec![
                vec![s.clone()],
                if change == "gone" {
                    vec![]
                } else {
                    vec![fresh]
                },
            ]),
            killed: RefCell::new(vec![]),
        };
        let result = stop(
            &fake,
            &home.0,
            &Tuning::default(),
            &[SessionTarget {
                pid: 42,
                session_id: "abc".into(),
            }],
        )
        .unwrap();
        assert_eq!(result.stopped, usize::from(change == "unchanged"));
        assert_eq!(fake.killed.borrow().len(), result.stopped);
        assert_eq!(result.skipped.len(), usize::from(change != "unchanged"));
    }
}

#[test]
#[ignore = "Read-only real Mac session inventory; never signals"]
fn real_claude_sessions_read_only() {
    let home = std::env::home_dir().unwrap();
    let inventory = list(
        &SystemProcesses { home: home.clone() },
        &home,
        &Tuning::default(),
    )
    .unwrap();
    for status in ["busy", "waiting", "idle"] {
        println!(
            "{status}={}",
            inventory
                .sessions
                .iter()
                .filter(|s| s.status.as_deref() == Some(status))
                .count()
        );
    }
    for s in &inventory.sessions {
        println!(
            "session status={:?} idle_h={:?} excl={:?}",
            s.status,
            s.idle_seconds.map(|v| v / 3600),
            s.exclusion
        );
    }
    println!(
        "total={} eligible={}",
        inventory.sessions.len(),
        inventory
            .sessions
            .iter()
            .filter(|s| s.stop_eligible)
            .count()
    );
}

#[test]
fn unknown_identity_folder_or_recent_activity_is_never_eligible() {
    let home = TempDir::new("session-unknown");
    let t = Tuning::default();
    let ps = parse_processes("42 1 0").unwrap();
    let s = session(&home.0);
    for candidate in [
        ClaudeSession {
            pid: None,
            ..s.clone()
        },
        ClaudeSession {
            pid: Some(0),
            ..s.clone()
        },
        ClaudeSession {
            session_id: None,
            ..s.clone()
        },
        ClaudeSession {
            cwd: None,
            ..s.clone()
        },
        ClaudeSession {
            cwd: Some("relative".into()),
            ..s.clone()
        },
        ClaudeSession {
            idle_seconds: Some(1),
            ..s.clone()
        },
    ] {
        assert!(exclusion(&candidate, Some(&ps), &t).is_some());
    }
    home.write(
        ".claude/projects/p/abc.jsonl",
        "{\"timestamp\":\"2020-01-01T00:00:00Z\"}\n{garbage\n",
    );
    assert_eq!(
        idle_seconds(&home.0, "abc", OffsetDateTime::now_utc().unix_timestamp()),
        None
    );
}
