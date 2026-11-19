//! Codex 사용 기록(0022 Codex 갱신, 2026-11-19): 최근 `diagnoseUsageDays`일 rollout에서 MCP 서버를 부른 횟수를
//! 세션을 시작한 프로젝트 폴더별로 센다. 읽는 것은 시작 `cwd`와 도구 호출의 서버 이름뿐이고 내용은 남기지 않는다.
//!
//! 세는 것(이 Mac 실측, 2026-11-19): `response_item`의 `custom_tool_call`(`name: "exec"`, 코드 모드) `input` 코드 안
//! `tools.mcp__<서버>__<도구>(` 호출 하나당 1회, 그리고 `function_call`의 `name`이 `mcp__<서버>__<도구>`인 것(코드 모드가
//! 아닐 때, 추정). 서버 이름은 설정의 `code-review-graph`와 도구 이름의 `code_review_graph`가 달라 `-` 같은 기호를 `_`로
//! 맞춰 비교한다(`server_key`, 추정). 프로젝트 = 시작 `cwd`에서 올라가며 처음 만나는 `.git` 폴더(없으면 `cwd` 그대로).
//! Codex가 `.codex/config.toml`을 읽는 기준이 프로젝트 루트이기 때문이다.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::bytes::Regex;
use serde::Deserialize;
use serde_json::Value;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::{Key, MIB, UsageIndex};
use crate::metrics::{DAY_SECS, unix_secs};
use crate::rate::codex as rollout;
use crate::tuning::Tuning;

/// 코드 모드 입력 안의 `tools.mcp__<서버>__<도구>(` 호출. 서버 이름은 `__` 앞까지(밑줄 하나는 이름의 일부).
static CODE_CALL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"tools\.mcp__((?:[A-Za-z0-9-]|_[A-Za-z0-9-])+)__[A-Za-z0-9_]+\s*\(").unwrap()
});
/// 줄을 JSON으로 읽을지 가르는 표식.
const MARK: &[u8] = b"mcp__";

/// 설정의 서버 이름과 도구 이름의 서버 부분을 같은 꼴로 맞춘다: 영숫자·`_` 밖은 `_`.
pub(in crate::coach) fn server_key(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

#[derive(Deserialize)]
struct Row {
    timestamp: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    payload: Option<Payload>,
}

#[derive(Deserialize)]
struct Payload {
    #[serde(rename = "type")]
    kind: Option<String>,
    name: Option<String>,
    /// 코드 모드 입력 코드(문자열이 아니면 줄을 버리지 않게 `Value`).
    input: Option<Value>,
    call_id: Option<String>,
    cwd: Option<String>,
}

/// 서버 이름의 `mcp__<서버>__<도구>` 꼴 도구 이름.
fn direct_server(name: &str) -> Option<&str> {
    let rest = name.strip_prefix("mcp__")?;
    let (server, tool) = rest.split_once("__")?;
    (!server.is_empty() && !tool.is_empty()).then_some(server)
}

/// 한 줄의 호출: (서버 키, 중복 확인용 ID, 줄 시각). 호출 줄이 아니거나 `since`보다 오래되면 빈 목록.
fn line_uses(line: &[u8], since: f64) -> Vec<(String, Option<String>, f64)> {
    if !rollout::contains(line, MARK) {
        return Vec::new();
    }
    let Ok(row) = serde_json::from_slice::<Row>(line) else {
        return Vec::new();
    };
    let at = row
        .timestamp
        .as_deref()
        .and_then(|t| OffsetDateTime::parse(t, &Rfc3339).ok())
        .map(unix_secs)
        .filter(|&at| at >= since);
    let (Some("response_item"), Some(payload), Some(at)) = (row.kind.as_deref(), row.payload, at)
    else {
        return Vec::new();
    };
    let id = |i: usize| payload.call_id.as_ref().map(|c| format!("{c}#{i}"));
    match payload.kind.as_deref() {
        Some("custom_tool_call") => {
            let code = payload.input.as_ref().and_then(Value::as_str).unwrap_or("");
            CODE_CALL
                .captures_iter(code.as_bytes())
                .enumerate()
                .map(|(i, c)| {
                    let server = String::from_utf8_lossy(&c[1]).into_owned();
                    (server_key(&server), id(i), at)
                })
                .collect()
        }
        Some("function_call") => payload
            .name
            .as_deref()
            .and_then(direct_server)
            .map(|server| vec![(server_key(server), id(0), at)])
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// 시작 폴더에서 올라가며 처음 `.git`이 있는 폴더. 없으면 그 폴더 그대로.
fn project_root(cwd: &Path, roots: &mut HashMap<PathBuf, PathBuf>) -> PathBuf {
    if let Some(known) = roots.get(cwd) {
        return known.clone();
    }
    let root = cwd
        .ancestors()
        .find(|dir| dir.join(".git").exists())
        .unwrap_or(cwd)
        .to_path_buf();
    roots.insert(cwd.to_path_buf(), root.clone());
    root
}

/// 파일 하나: 시작 `cwd`(session_meta·turn_context의 첫 값)와 기간 안 호출.
fn read_file(
    file: &Path,
    since: f64,
    seen: &mut HashSet<String>,
    roots: &mut HashMap<PathBuf, PathBuf>,
    index: &mut UsageIndex,
) -> Option<PathBuf> {
    let mut first_cwd: Option<String> = None;
    let mut uses = Vec::new();
    rollout::for_each_line(file, |line| {
        if first_cwd.is_none() && rollout::contains(line, b"\"cwd\"") {
            first_cwd = serde_json::from_slice::<Row>(line)
                .ok()
                .and_then(|r| r.payload?.cwd)
                .filter(|c| !c.is_empty());
        }
        uses.extend(line_uses(line, since));
    });
    let project = project_root(Path::new(&first_cwd?), roots);
    for (server, id, at) in uses {
        if id.is_some_and(|id| !seen.insert(id)) {
            continue;
        }
        let stat = index
            .counts
            .entry(Key::Server(server))
            .or_default()
            .entry(project.clone())
            .or_default();
        stat.count += 1;
        stat.last = stat.last.max(at);
    }
    index.projects.insert(project.clone());
    Some(project)
}

/// 최근 `diagnoseUsageDays`일에 바뀐 rollout을 최근 순으로 `diagnoseUsageFiles`개, 합계 `diagnoseUsageMaxMib`까지 읽는다
/// (Claude 쪽과 같은 tuning).
pub(super) fn read(home: &Path, tuning: &Tuning, now: f64) -> UsageIndex {
    let since = now - tuning.diagnose_usage_days as f64 * DAY_SECS;
    let mut files: Vec<(PathBuf, f64, u64)> = rollout::rollout_files(home)
        .into_iter()
        .filter_map(|path| {
            let meta = std::fs::metadata(&path).ok()?;
            let modified = meta.modified().ok()?;
            let modified = modified
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_secs_f64();
            (modified >= since).then_some((path, modified, meta.len()))
        })
        .collect();
    files.sort_by(|a, b| b.1.total_cmp(&a.1));
    let limit = tuning.diagnose_usage_files;
    let budget = tuning.diagnose_usage_max_mib.saturating_mul(MIB);
    let mut index = UsageIndex {
        days: tuning.diagnose_usage_days,
        min_sessions: tuning.diagnose_min_sessions,
        scope_min_uses: tuning.scope_min_uses,
        is_partial: files.len() > limit,
        ..UsageIndex::default()
    };
    let mut seen = HashSet::new();
    let mut roots = HashMap::new();
    let mut read_bytes = 0u64;
    for (file, _, size) in files.iter().take(limit) {
        if read_bytes.saturating_add(*size) > budget {
            index.is_partial = true;
            break;
        }
        read_bytes += size;
        let project = read_file(file, since, &mut seen, &mut roots, &mut index);
        index.sessions += 1;
        if let Some(project) = project {
            *index.project_sessions.entry(project).or_default() += 1;
        }
    }
    index
}
