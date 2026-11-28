//! Kiro messages.jsonl의 tool_call을 서버 이름 정확 일치로 집계한다(0022).
use super::{Key, MIB, UsageIndex};
use crate::context::{fsutil, kiro_session};
use crate::metrics::{DAY_SECS, unix_secs};
use crate::tuning::Tuning;
use serde_json::Value;
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
const MAX_LINE_BYTES: u64 = 16 << 20;

/// 작업 공간 없이 연 세션(`workspacePaths: []`, `_global`)은 홈에서 시작한 것으로 센다.
/// 홈 사용은 프로젝트 이동 제안을 막을 뿐 기록 전체를 부분으로 만들지 않는다.
fn project(file: &Path, home: &Path) -> Option<PathBuf> {
    let session = fsutil::read_json(&file.parent()?.join("session.json"))?;
    let paths = session.get("workspacePaths")?.as_array()?;
    let Some(first) = paths.first() else {
        return Some(home.to_path_buf());
    };
    let first = first.as_str()?;
    (!first.is_empty() && Path::new(first).is_absolute()).then(|| PathBuf::from(first))
}

fn call<'a>(row: &'a Value, payload: &'a Value) -> Option<(&'a str, Option<&'a str>)> {
    if payload.get("type")?.as_str()? != "tool_call" {
        return None;
    }
    let server = payload
        .pointer("/_meta/kiro/serverName")?
        .as_str()
        .filter(|s| !s.is_empty())?;
    let id = payload
        .get("toolCallId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            row.get("id")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
        });
    Some((server, id))
}

fn row(
    row: Value,
    since: f64,
    project: Option<&Path>,
    index: &mut UsageIndex,
    seen: &mut HashSet<String>,
) -> bool {
    let at = row
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(|t| OffsetDateTime::parse(t, &Rfc3339).ok())
        .map(unix_secs);
    let Some(at) = at else {
        index.is_partial = true;
        return false;
    };
    if at < since {
        return false;
    }
    let Some(payload) = row.get("payload") else {
        index.is_partial = true;
        return true;
    };
    let Some((server, id)) = call(&row, payload) else {
        return true;
    };
    let Some(project) = project else {
        return true;
    };
    if id.is_some_and(|id| !seen.insert(id.to_owned())) {
        return true;
    }
    let stat = index
        .counts
        .entry(Key::Server(server.into()))
        .or_default()
        .entry(project.into())
        .or_default();
    stat.count += 1;
    stat.last = stat.last.max(at);
    true
}

fn read_file(file: &Path, home: &Path, since: f64, index: &mut UsageIndex) {
    let project = project(file, home);
    if project.is_none() {
        index.is_partial = true;
    }
    let result = (|| -> std::io::Result<bool> {
        let mut reader = BufReader::new(std::fs::File::open(file)?);
        let mut line = Vec::new();
        let mut in_window = false;
        let mut seen = HashSet::new();
        loop {
            line.clear();
            let count = reader
                .by_ref()
                .take(MAX_LINE_BYTES + 1)
                .read_until(b'\n', &mut line)?;
            if count == 0 {
                return Ok(in_window);
            }
            if count as u64 > MAX_LINE_BYTES {
                return Err(std::io::Error::other("Kiro line too long"));
            }
            if line.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            match serde_json::from_slice(&line) {
                Ok(value) => in_window |= row(value, since, project.as_deref(), index, &mut seen),
                Err(_) => index.is_partial = true,
            }
        }
    })();
    match result {
        Ok(true) => {
            index.sessions += 1;
            if let Some(project) = project {
                index.projects.insert(project.clone());
                *index.project_sessions.entry(project).or_default() += 1;
            }
        }
        Ok(false) => {}
        Err(_) => index.is_partial = true,
    }
}

pub(super) fn read(home: &Path, tuning: &Tuning, now: f64) -> UsageIndex {
    let since = now - tuning.diagnose_usage_days as f64 * DAY_SECS;
    let mut index = UsageIndex {
        days: tuning.diagnose_usage_days,
        min_sessions: tuning.diagnose_min_sessions,
        scope_min_uses: tuning.scope_min_uses,
        ..UsageIndex::default()
    };
    let mut bytes = 0u64;
    for (files, file) in kiro_session::message_files(home)
        .into_iter()
        .filter(|file| {
            std::fs::metadata(file)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .is_none_or(|t| t.as_secs_f64() >= since)
        })
        .enumerate()
    {
        let Ok(meta) = std::fs::metadata(&file) else {
            index.is_partial = true;
            continue;
        };
        if files >= tuning.diagnose_usage_files
            || bytes.saturating_add(meta.len()) > tuning.diagnose_usage_max_mib.saturating_mul(MIB)
        {
            index.is_partial = true;
            break;
        }
        bytes += meta.len();
        read_file(&file, home, since, &mut index);
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::fsutil::testing::TempDir;
    use serde_json::json;
    fn call(id: &str, timestamp: &str) -> String {
        json!({"id":id,"timestamp":timestamp,"payload":{"type":"tool_call","toolCallId":id,"_meta":{"kiro":{"serverName":"docs-A"}}}}).to_string()
    }
    fn session(home: &TempDir, workspace: bool, messages: &str) {
        home.write(
            ".kiro/sessions/h/s/session.json",
            &if workspace {
                json!({"workspacePaths":[home.path().join("work/sub")]})
            } else {
                json!({})
            }
            .to_string(),
        );
        home.write(".kiro/sessions/h/s/messages.jsonl", messages);
    }
    #[test]
    fn counts_calls_deduplicates_ids_excludes_old_and_preserves_workspace() {
        let home = TempDir::new("usage-kiro");
        let current = call("a", "2026-11-28T00:00:00Z");
        session(
            &home,
            true,
            &[
                current.clone(),
                current,
                call("b", "2025-01-01T00:00:00Z"),
                call("c", "2026-11-28T00:00:00Z"),
            ]
            .join("\n"),
        );
        let now = unix_secs(OffsetDateTime::parse("2026-11-28T12:00:00Z", &Rfc3339).unwrap());
        let index = read(home.path(), &Tuning::default(), now);
        assert_eq!(index.for_server("docs-A").total, 2);
        assert_eq!(index.for_server("docs_A").total, 0);
        assert_eq!(index.sessions, 1);
        assert!(!index.is_partial);
        assert!(index.projects.contains(&home.path().join("work/sub")));
    }
    #[test]
    fn calls_without_ids_are_counted_and_incomplete_reads_are_partial() {
        let home = TempDir::new("usage-kiro-no-id");
        let anonymous = json!({"timestamp":"2026-11-28T00:00:00Z","payload":{"type":"tool_call","_meta":{"kiro":{"serverName":"docs-A"}}}}).to_string();
        let now = unix_secs(OffsetDateTime::parse("2026-11-28T12:00:00Z", &Rfc3339).unwrap());
        session(&home, true, &format!("{anonymous}\n{anonymous}"));
        let index = read(home.path(), &Tuning::default(), now);
        assert_eq!(index.for_server("docs-A").total, 2);
        assert!(!index.is_partial);
        let mut index = UsageIndex::default();
        read_file(
            &home.path().join("missing/messages.jsonl"),
            home.path(),
            now - DAY_SECS,
            &mut index,
        );
        assert!(index.is_partial);
        session(&home, true, &" ".repeat(MAX_LINE_BYTES as usize + 1));
        assert!(read(home.path(), &Tuning::default(), now).is_partial);
    }

    #[test]
    fn broken_lines_missing_workspace_and_limits_are_partial() {
        let home = TempDir::new("usage-kiro-partial");
        let current = call("a", "2026-11-28T00:00:00Z");
        let now = unix_secs(OffsetDateTime::parse("2026-11-28T12:00:00Z", &Rfc3339).unwrap());
        session(&home, true, &format!("{current}\n{{broken"));
        assert!(read(home.path(), &Tuning::default(), now).is_partial);
        session(&home, false, &current);
        assert!(read(home.path(), &Tuning::default(), now).is_partial);
        home.write(
            ".kiro/sessions/h/s/session.json",
            r#"{"workspacePaths":[]}"#,
        );
        let global = read(home.path(), &Tuning::default(), now);
        assert!(!global.is_partial);
        assert!(global.projects.contains(home.path()));
        session(&home, true, &current);
        for tuning in [
            Tuning {
                diagnose_usage_files: 0,
                ..Tuning::default()
            },
            Tuning {
                diagnose_usage_max_mib: 0,
                ..Tuning::default()
            },
        ] {
            assert!(read(home.path(), &tuning, now).is_partial);
        }
        session(&home, true, &call("old", "2025-01-01T00:00:00Z"));
        assert_eq!(read(home.path(), &Tuning::default(), now).sessions, 0);
    }
    #[test]
    #[ignore = "실제 HOME 읽기 전용 측정"]
    fn real_kiro_usage_and_proposals() {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        let tuning = Tuning::default();
        let index = read(&home, &tuning, unix_secs(OffsetDateTime::now_utc()));
        println!("sessions={} partial={}", index.sessions, index.is_partial);
        let config = fsutil::read_json(&home.join(".kiro/settings/mcp.json")).unwrap();
        for (name, enabled) in crate::context::scan::kiro_mcp_states(&config) {
            let used = index.for_server(&name);
            let scope = crate::coach::scope::proposal(&home, enabled, &used).is_some();
            println!(
                "server={name} uses={} projects={} scope={scope} unused_evidence={} project_names={:?}",
                used.total,
                used.project_count,
                enabled
                    && used.total == 0
                    && !used.is_partial
                    && used.sessions >= used.min_sessions,
                used.projects.iter().map(|p| &p.name).collect::<Vec<_>>()
            );
        }
    }
}
