//! On-demand Claude sessions (0030). Tests inject all process operations.
use std::collections::HashSet;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use tauri::State;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{context::fsutil::is_inside, error::AppError, settings::SettingsState, tuning::Tuning};

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeSession {
    pub pid: Option<u32>,
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    pub status: Option<String>,
    /// Unix epoch milliseconds, as `claude agents --json` prints it (2.1.331).
    pub started_at: Option<u64>,
    pub kind: Option<String>,
    pub idle_seconds: Option<u64>,
    pub stop_eligible: bool,
    pub exclusion: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeSessions {
    pub sessions: Vec<ClaudeSession>,
    pub can_stop: bool,
    pub idle_hours: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionTarget {
    pub pid: u32,
    pub session_id: String,
}

#[derive(Debug, Default, Serialize)]
pub struct StopSessionsResult {
    pub stopped: usize,
    pub skipped: Vec<String>,
}

fn failure() -> AppError {
    AppError::InvalidInput(crate::tr!(
        "Claude 세션을 읽지 못했어요.",
        "Couldn't read Claude sessions."
    ))
}

/// Each field is read on its own: a missing or retyped field becomes `None`
/// instead of failing the whole list (the JSON shape is undocumented, 0030).
fn parse_agents(text: &[u8]) -> Result<Vec<ClaudeSession>, AppError> {
    let rows: Vec<serde_json::Value> = serde_json::from_slice(text).map_err(|e| {
        eprintln!("kcal: Claude agents JSON: {e}");
        failure()
    })?;
    let text_field = |row: &serde_json::Value, key: &str| {
        row.get(key).and_then(|v| v.as_str()).map(str::to_owned)
    };
    Ok(rows
        .iter()
        .filter(|row| row.is_object())
        .map(|row| ClaudeSession {
            pid: row
                .get("pid")
                .and_then(|v| v.as_u64())
                .and_then(|v| u32::try_from(v).ok()),
            session_id: text_field(row, "sessionId"),
            cwd: text_field(row, "cwd"),
            status: text_field(row, "status"),
            started_at: row.get("startedAt").and_then(|v| v.as_u64()),
            kind: text_field(row, "kind"),
            ..ClaudeSession::default()
        })
        .collect())
}

/// No mtime fallback. Only a plain session filename is accepted from the CLI.
fn idle_seconds(home: &Path, id: &str, now: i64) -> Option<u64> {
    if id.is_empty()
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return None;
    }
    let mut last: Option<OffsetDateTime> = None;
    let projects = std::fs::read_dir(home.join(".claude/projects")).ok()?;
    for project in projects.flatten() {
        let file = project.path().join(format!("{id}.jsonl"));
        let Ok(file) = std::fs::File::open(file) else {
            continue;
        };
        // The same id can have transcripts in several project folders (resume/fork):
        // keep the newest, never whichever `read_dir` lists last.
        let mut file_last = None;
        for line in BufReader::new(file).lines() {
            let line = line.ok()?;
            if line.trim().is_empty() {
                continue;
            }
            let value = serde_json::from_str::<serde_json::Value>(&line).ok()?;
            if let Some(timestamp) = value.get("timestamp") {
                // A malformed latest timestamp makes the age unknown, never stale/eligible.
                file_last = Some(
                    timestamp
                        .as_str()
                        .and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok()),
                );
            }
        }
        match file_last {
            None => {}
            Some(None) => return None,
            Some(Some(at)) => last = Some(last.map_or(at, |prev| prev.max(at))),
        }
    }
    let elapsed = now.checked_sub(last?.unix_timestamp())?;
    u64::try_from(elapsed).ok()
}

#[derive(Debug)]
struct Process {
    pid: u32,
    ppid: u32,
    cpu: f64,
}

fn parse_processes(text: &str) -> Option<Vec<Process>> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let mut cols = line.split_whitespace();
            let p = Process {
                pid: cols.next()?.parse().ok()?,
                ppid: cols.next()?.parse().ok()?,
                cpu: cols.next()?.parse().ok()?,
            };
            (p.cpu.is_finite() && p.cpu >= 0.0 && cols.next().is_none()).then_some(p)
        })
        .collect()
}

fn child_cpu(pid: u32, processes: &[Process]) -> f64 {
    let mut descendants = HashSet::from([pid]);
    loop {
        let before = descendants.len();
        for p in processes {
            if descendants.contains(&p.ppid) {
                descendants.insert(p.pid);
            }
        }
        if descendants.len() == before {
            break;
        }
    }
    processes
        .iter()
        .filter(|p| p.pid != pid && descendants.contains(&p.pid))
        .map(|p| p.cpu)
        .sum()
}

fn exclusion(s: &ClaudeSession, processes: Option<&[Process]>, t: &Tuning) -> Option<&'static str> {
    if s.status.as_deref() != Some("idle") {
        return Some("not_idle");
    }
    let Some(pid) = s.pid.filter(|p| *p > 1) else {
        return Some("unknown_pid");
    };
    if s.session_id.as_deref().is_none_or(str::is_empty) {
        return Some("unknown_session");
    }
    if s.idle_seconds
        .is_none_or(|age| age < t.session_idle_hours * 3600)
    {
        return Some("unknown_or_recent_activity");
    }
    let Some(cwd) = s.cwd.as_deref().map(Path::new).filter(|p| p.is_absolute()) else {
        return Some("unknown_folder");
    };
    let Ok(cwd) = cwd.canonicalize() else {
        return Some("unknown_folder");
    };
    for root in &t.session_protected_folders {
        let root = Path::new(root);
        if matches!(root.try_exists(), Ok(false)) {
            continue; // a missing folder cannot contain the (existing) session folder
        }
        let Ok(root) = root.canonicalize() else {
            return Some("unknown_protected_folder");
        };
        if is_inside(&cwd, &root) {
            return Some("protected_folder");
        }
    }
    let Some(processes) = processes else {
        return Some("unknown_cpu");
    };
    if !processes.iter().any(|p| p.pid == pid) {
        return Some("unknown_cpu");
    }
    (child_cpu(pid, processes) >= t.session_child_cpu_percent).then_some("child_cpu")
}

trait Processes {
    fn agents(&self) -> Result<Vec<ClaudeSession>, AppError>;
    fn processes(&self) -> Option<Vec<Process>>;
    fn terminate(&self, pid: u32) -> Result<(), AppError>;
}

struct SystemProcesses {
    home: PathBuf,
}
impl Processes for SystemProcesses {
    fn agents(&self) -> Result<Vec<ClaudeSession>, AppError> {
        let path_env = std::env::var_os("PATH");
        let cli = crate::usage::paths::claude_cli_candidates(&self.home, path_env.as_deref())
            .into_iter()
            .find(|p| p.is_file())
            .ok_or_else(failure)?;
        let output = Command::new(cli)
            .args(["agents", "--json"])
            .output()
            .map_err(|e| {
                eprintln!("kcal: Claude agents: {e}");
                failure()
            })?;
        if !output.status.success() {
            eprintln!("kcal: Claude agents exit: {}", output.status);
            return Err(failure());
        }
        parse_agents(&output.stdout)
    }
    fn processes(&self) -> Option<Vec<Process>> {
        // Windows only displays status; no process execution or signalling there.
        if !cfg!(target_os = "macos") {
            return None;
        }
        let output = Command::new("/bin/ps")
            .args(["-A", "-o", "pid=,ppid=,%cpu="])
            .env("LC_ALL", "C")
            .output()
            .map_err(|e| eprintln!("kcal: session process list: {e}"))
            .ok()?;
        if !output.status.success() {
            eprintln!("kcal: session process list exit: {}", output.status);
            return None;
        }
        parse_processes(std::str::from_utf8(&output.stdout).ok()?)
    }
    fn terminate(&self, pid: u32) -> Result<(), AppError> {
        if !cfg!(target_os = "macos") || pid <= 1 {
            return Err(AppError::InvalidInput("Unsupported session stop".into()));
        }
        let result = Command::new("/bin/kill")
            .args(["-TERM", &pid.to_string()])
            .status();
        match result {
            Ok(s) if s.success() => Ok(()),
            error => {
                eprintln!("kcal: session TERM failed: {error:?}");
                Err(AppError::Internal)
            }
        }
    }
}

fn list(system: &impl Processes, home: &Path, t: &Tuning) -> Result<ClaudeSessions, AppError> {
    let mut sessions = system.agents()?;
    let processes = system.processes();
    let now = OffsetDateTime::now_utc().unix_timestamp();
    for s in &mut sessions {
        s.idle_seconds = s
            .session_id
            .as_deref()
            .and_then(|id| idle_seconds(home, id, now));
        s.exclusion = exclusion(s, processes.as_deref(), t).map(str::to_owned);
        s.stop_eligible = cfg!(target_os = "macos") && s.exclusion.is_none();
    }
    sessions.sort_by_key(|s| match s.status.as_deref() {
        Some("waiting") => 0,
        Some("busy") => 1,
        Some("idle") => 2,
        _ => 3,
    });
    Ok(ClaudeSessions {
        sessions,
        can_stop: cfg!(target_os = "macos"),
        idle_hours: t.session_idle_hours,
    })
}

fn stop_target(
    system: &impl Processes,
    home: &Path,
    t: &Tuning,
    target: &SessionTarget,
) -> Result<Option<&'static str>, AppError> {
    let current = system.agents()?;
    let Some(mut session) = current
        .into_iter()
        .find(|s| s.pid == Some(target.pid) && s.session_id.as_ref() == Some(&target.session_id))
    else {
        return Ok(Some("session_changed"));
    };
    session.idle_seconds = idle_seconds(
        home,
        &target.session_id,
        OffsetDateTime::now_utc().unix_timestamp(),
    );
    if let Some(reason) = exclusion(&session, system.processes().as_deref(), t) {
        return Ok(Some(reason));
    }
    // Re-read identity/status immediately before TERM; also reject a changed working folder.
    let fresh = system.agents()?;
    let unchanged = fresh.iter().any(|s| {
        s.pid == Some(target.pid)
            && s.session_id.as_ref() == Some(&target.session_id)
            && s.status.as_deref() == Some("idle")
            && s.cwd == session.cwd
    });
    if !unchanged {
        return Ok(Some("session_changed"));
    }
    match system.terminate(target.pid) {
        Ok(()) => Ok(None),
        Err(_) => Ok(Some("signal_failed")),
    }
}

/// Shown to the user after a stop (0030 "건너뛴 이유").
fn reason_text(code: &str) -> String {
    match code {
        "not_idle" => crate::tr!("대기 상태가 아님", "not idle"),
        "unknown_or_recent_activity" => {
            crate::tr!("최근 활동이 있거나 모름", "recent or unknown activity")
        }
        "protected_folder" => crate::tr!("보호 폴더", "protected folder"),
        "unknown_protected_folder" => crate::tr!(
            "보호 폴더를 확인 못 함",
            "couldn't check a protected folder"
        ),
        "child_cpu" => crate::tr!("자식 프로세스가 CPU를 씀", "child processes are using CPU"),
        "unknown_cpu" => crate::tr!("CPU를 확인 못 함", "couldn't check CPU"),
        "session_changed" => crate::tr!("그사이 세션이 바뀜", "session changed meanwhile"),
        "signal_failed" => crate::tr!("종료 신호 실패", "stop signal failed"),
        "read_failed" => crate::tr!("세션을 다시 읽지 못함", "couldn't re-read sessions"),
        _ => crate::tr!("세션 정보를 확인 못 함", "couldn't verify the session"),
    }
}

fn stop(
    system: &impl Processes,
    home: &Path,
    t: &Tuning,
    targets: &[SessionTarget],
) -> Result<StopSessionsResult, AppError> {
    if targets
        .iter()
        .any(|s| s.pid <= 1 || s.session_id.is_empty())
    {
        return Err(AppError::InvalidInput(crate::tr!(
            "잘못된 세션 대상이에요.",
            "Invalid session target."
        )));
    }
    let mut result = StopSessionsResult::default();
    let mut seen = HashSet::new();
    for target in targets {
        if !seen.insert(target.pid) {
            continue;
        }
        match stop_target(system, home, t, target) {
            Ok(None) => result.stopped += 1,
            Ok(Some(reason)) => {
                result
                    .skipped
                    .push(format!("{}: {}", target.pid, reason_text(reason)))
            }
            Err(error) if result.stopped == 0 => return Err(error),
            // Preserve the count if an inventory read fails after an earlier signal.
            Err(_) => {
                result
                    .skipped
                    .push(format!("{}: {}", target.pid, reason_text("read_failed")));
            }
        }
    }
    Ok(result)
}

#[tauri::command]
pub async fn list_claude_sessions(
    state: State<'_, SettingsState>,
) -> Result<ClaudeSessions, AppError> {
    let tuning = state.tuning();
    tauri::async_runtime::spawn_blocking(move || {
        let home = std::env::home_dir().ok_or(AppError::Internal)?;
        list(&SystemProcesses { home: home.clone() }, &home, &tuning)
    })
    .await
    .map_err(|_| AppError::Internal)?
}

#[tauri::command]
pub async fn stop_idle_claude_sessions(
    state: State<'_, SettingsState>,
    targets: Vec<SessionTarget>,
) -> Result<StopSessionsResult, AppError> {
    if !cfg!(target_os = "macos") {
        return Err(AppError::InvalidInput(crate::tr!(
            "macOS에서만 세션을 끌 수 있어요.",
            "Stopping sessions is only available on macOS."
        )));
    }
    let tuning = state.tuning();
    tauri::async_runtime::spawn_blocking(move || {
        let home = std::env::home_dir().ok_or(AppError::Internal)?;
        stop(
            &SystemProcesses { home: home.clone() },
            &home,
            &tuning,
            &targets,
        )
    })
    .await
    .map_err(|_| AppError::Internal)?
}

#[cfg(test)]
mod tests;
