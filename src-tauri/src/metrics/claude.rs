//! Claude 지표. 지금 5시간 구간과 막대는 한도 기록(`limit-history.jsonl`)에서, 요청 수·캐시 적중률은
//! `~/.claude/projects/**/*.jsonl`의 assistant 줄 `message.usage`에서 낸다.
//!
//! 줄은 `"type":"assistant"`와 `"usage"`가 둘 다 있을 때만 파싱하고, 숫자 필드·timestamp·`message.id`만
//! 가진 타입으로 읽는다(본문은 serde가 건너뛰어 메모리에 남지 않는다). 같은 `message.id`는 한 번만 센다.

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::SystemTime;

use serde::Deserialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::{
    AgentMetrics, BarUnit, DAY_SECS, Window, current, hourly, hourly_start, modified_secs,
    percent_bars, ratio, unix_secs, weekly_increments, windows,
};
use crate::context::{fsutil, paths};
use crate::rate::history;
use crate::tuning::Tuning;
use crate::usage::Agent;

const ASSISTANT_MARK: &[u8] = br#""type":"assistant""#;
const USAGE_MARK: &[u8] = br#""usage""#;
const MODEL_MARK: &[u8] = br#""model""#;

#[derive(Deserialize)]
struct Row {
    #[serde(rename = "type")]
    kind: Option<String>,
    timestamp: Option<String>,
    message: Option<Message>,
    /// 세션 작업 폴더. 최적화 점검(0018 작업 A)이 최근 작업 폴더를 모을 때만 쓴다.
    cwd: Option<String>,
    entrypoint: Option<String>,
    effort: Option<serde_json::Value>,
    #[serde(rename = "attributionAgent")]
    attribution_agent: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct Message {
    id: Option<String>,
    /// 최적화 점검(coach::checks)이 최근 모델을 셀 때만 쓴다.
    model: Option<String>,
    usage: Option<RawUsage>,
}

/// 값이 null이어도 줄을 버리지 않는다(`rate::claude`와 같다).
#[derive(Deserialize)]
struct RawUsage {
    input_tokens: Option<f64>,
    cache_creation_input_tokens: Option<f64>,
    cache_read_input_tokens: Option<f64>,
    output_tokens: Option<f64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Usage {
    pub(crate) input_tokens: f64,
    pub(crate) cache_creation_input_tokens: f64,
    pub(crate) cache_read_input_tokens: f64,
    pub(crate) output_tokens: f64,
}

impl Usage {
    pub(crate) fn total_input(&self) -> f64 {
        self.input_tokens + self.cache_creation_input_tokens + self.cache_read_input_tokens
    }
}

/// assistant 줄 하나에서 쓰는 곳 모두가 필요한 값(시각·id·usage·model·cwd·entrypoint). 본문은 serde가 건너뛴다.
#[derive(Clone, Debug)]
pub(crate) struct Entry {
    pub(crate) ts: f64,
    pub(crate) id: Option<Box<str>>,
    pub(crate) usage: Option<Usage>,
    /// model·cwd·entrypoint는 거의 모든 줄에서 같은 값이라 하나를 나눠 쓴다(`intern`).
    pub(crate) model: Option<Arc<str>>,
    pub(crate) cwd: Option<Arc<str>>,
    pub(crate) entrypoint: Option<Arc<str>>,
    /// 서브에이전트 기록 파일(`…/subagents/*.jsonl`)의 줄에만 있다. 메모가 모든 줄을 들고 있어 한 칸(8B)으로 둔다.
    pub(crate) subagent: Option<Box<Subagent>>,
}

#[derive(Clone, Debug)]
pub(crate) struct Subagent {
    pub(crate) effort: Option<Arc<str>>,
    pub(crate) attribution_agent: Option<Arc<str>>,
}

/// 같은 문자열을 하나로 모은다. 서로 다른 model·cwd·entrypoint 값만큼만 커지고 줄 수에는 안 는다.
fn intern(s: String) -> Arc<str> {
    static POOL: LazyLock<Mutex<HashSet<Arc<str>>>> = LazyLock::new(Default::default);
    let Ok(mut pool) = POOL.lock() else {
        return s.into();
    };
    if let Some(found) = pool.get(s.as_str()) {
        return Arc::clone(found);
    }
    let new: Arc<str> = s.into();
    pool.insert(Arc::clone(&new));
    new
}

/// 요청 하나: (시각(유닉스 초), `message.id`, usage).
type Request = (f64, Option<String>, Usage);

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn entry(line: &[u8], is_subagent_file: bool) -> Option<Entry> {
    if !contains(line, ASSISTANT_MARK)
        || !(contains(line, USAGE_MARK) || contains(line, MODEL_MARK))
    {
        return None;
    }
    let row: Row = serde_json::from_slice(line).ok()?;
    if row.kind.as_deref() != Some("assistant") {
        return None;
    }
    let ts = OffsetDateTime::parse(row.timestamp.as_deref()?, &Rfc3339).ok()?;
    let message = row.message?;
    let value = |v: Option<f64>| v.unwrap_or(0.0);
    Some(Entry {
        ts: unix_secs(ts),
        id: message.id.map(String::into_boxed_str),
        usage: message.usage.map(|u| Usage {
            input_tokens: value(u.input_tokens),
            cache_creation_input_tokens: value(u.cache_creation_input_tokens),
            cache_read_input_tokens: value(u.cache_read_input_tokens),
            output_tokens: value(u.output_tokens),
        }),
        model: message.model.map(intern),
        cwd: row.cwd.map(intern),
        entrypoint: row.entrypoint.map(intern),
        subagent: is_subagent_file.then(|| {
            let text = |v: Option<serde_json::Value>| v?.as_str().map(|s| intern(s.to_owned()));
            Box::new(Subagent {
                effort: text(row.effort),
                attribution_agent: text(row.attribution_agent),
            })
        }),
    })
}

/// 파일별 파싱 결과: (수정 시각, 크기)가 같으면 다시 읽지 않는다. 환율·지표·검진 전후·점검이 같은 파일을 나눠 쓴다.
/// 없어진 파일의 항목은 `log_files`가 목록을 훑을 때 지운다.
#[allow(clippy::type_complexity)]
static PARSED: LazyLock<Mutex<HashMap<PathBuf, (SystemTime, u64, Arc<Vec<Entry>>)>>> =
    LazyLock::new(Default::default);

/// 테스트용: 메모의 (파일 수, 항목 수, 추정 바이트 = 항목 크기 × 개수 + 문자열 capacity 합).
#[cfg(test)]
pub(crate) fn memo_stats() -> (usize, usize, usize) {
    let map = PARSED.lock().unwrap();
    let (mut n, mut bytes) = (0, 0);
    for (_, _, v) in map.values() {
        n += v.len();
        bytes += v.capacity() * std::mem::size_of::<Entry>();
        for e in v.iter() {
            // 인터닝된 model·cwd는 값마다 한 번만 세지 못하므로 id만 더한다(나머지는 공유 포인터).
            bytes += e.id.as_ref().map_or(0, |s| s.len());
        }
    }
    (map.len(), n, bytes)
}

/// 테스트용: 이 경로의 파싱 결과가 메모에 있는가.
#[cfg(test)]
pub(crate) fn memo_has(path: &Path) -> bool {
    PARSED.lock().unwrap().contains_key(path)
}

/// 테스트용: 경로별로 실제로 파일을 읽은 횟수.
#[cfg(test)]
pub(crate) static PARSE_COUNTS: LazyLock<Mutex<HashMap<PathBuf, usize>>> =
    LazyLock::new(Default::default);

/// 파일의 assistant 줄(usage나 model이 있는 것). 같은 파일은 바뀌기 전까지 한 번만 읽는다.
pub(crate) fn entries(file: &Path) -> Arc<Vec<Entry>> {
    let key = std::fs::metadata(file)
        .ok()
        .and_then(|m| Some((m.modified().ok()?, m.len())));
    if let Some((modified, len)) = key
        && let Ok(map) = PARSED.lock()
        && let Some((m, l, found)) = map.get(file)
        && (*m, *l) == (modified, len)
    {
        return Arc::clone(found);
    }
    #[cfg(test)]
    if let Ok(mut counts) = PARSE_COUNTS.lock() {
        *counts.entry(file.to_path_buf()).or_default() += 1;
    }
    // 바로 위 폴더 이름만 본다(HOME 경로 어딘가의 `subagents` 폴더는 서브에이전트 기록이 아니다).
    let is_subagent_file = file
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|n| n == "subagents");
    let parsed = Arc::new(read_lines(file, |line| entry(line, is_subagent_file)));
    if let Some((modified, len)) = key
        && let Ok(mut map) = PARSED.lock()
    {
        map.insert(file.to_path_buf(), (modified, len, Arc::clone(&parsed)));
    }
    parsed
}

/// 요청 하나의 모델: (시각(유닉스 초), `message.id`, `message.model`). 정규화는 쓰는 쪽이 한다.
pub(crate) type ModelRequest = (f64, Option<String>, String);

fn read_requests(file: &Path) -> Vec<Request> {
    entries(file)
        .iter()
        .filter_map(|e| Some((e.ts, e.id.as_deref().map(String::from), e.usage?)))
        .collect()
}

/// 파일을 줄마다 `parse`한다. 표식이 없는 줄은 `parse`가 JSON으로 읽기 전에 건너뛴다.
pub(crate) fn read_lines<T>(file: &Path, parse: impl Fn(&[u8]) -> Option<T>) -> Vec<T> {
    let Ok(handle) = std::fs::File::open(file) else {
        return Vec::new();
    };
    let mut reader = BufReader::new(handle);
    let mut line = Vec::new();
    let mut requests = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        requests.extend(parse(&line));
    }
    requests
}

/// 최적화 점검: `since` 뒤에 바뀐 기록 파일의 assistant 줄 중 `since` 뒤 요청의 모델(같은 walker를 쓴다)과
/// 그 줄들의 작업 폴더(`cwd`, 중복 없이 이름순). 폴더가 지금 있는지는 쓰는 쪽이 본다.
#[cfg(test)]
pub(crate) fn model_requests(home: &Path, since: f64) -> (Vec<ModelRequest>, Vec<PathBuf>) {
    let (requests, workspaces, _) = recent_requests(home, since);
    (requests, workspaces)
}

/// `model_requests`와 같은 기간의 서브에이전트 assistant 줄(세 번째 값)을 한 번의 폴더 탐색으로 낸다.
pub(crate) fn recent_requests(
    home: &Path,
    since: f64,
) -> (Vec<ModelRequest>, Vec<PathBuf>, Vec<Entry>) {
    let mut subagent = Vec::new();
    let mut lines: Vec<(ModelRequest, Option<String>)> = Vec::new();
    for file in log_files(home, since, usize::MAX) {
        for e in entries(&file).iter().filter(|e| e.ts >= since) {
            if e.subagent.is_some() {
                subagent.push(e.clone());
            }
            if let Some(model) = e.model.as_deref() {
                lines.push((
                    (e.ts, e.id.as_deref().map(String::from), model.to_owned()),
                    e.cwd.as_deref().map(String::from),
                ));
            }
        }
    }
    let workspaces: std::collections::BTreeSet<PathBuf> = lines
        .iter()
        .filter_map(|(_, cwd)| cwd.as_deref().map(PathBuf::from))
        .collect();
    let requests = lines.into_iter().map(|(request, _)| request).collect();
    (requests, workspaces.into_iter().collect(), subagent)
}

/// `root` 아래 메모 항목 중 지금 목록에 없는(지워진) 파일을 뺀다.
fn prune_memo(root: &Path, present: &[PathBuf]) {
    if let Ok(mut map) = PARSED.lock() {
        let present: HashSet<&PathBuf> = present.iter().collect();
        map.retain(|path, _| !path.starts_with(root) || present.contains(path));
    }
}

/// `since` 뒤에 바뀐 기록 파일, 최근에 바뀐 순으로 `limit`개.
// ponytail: 파일을 통째로 읽는다(이 Mac 5시간 안 9개 약 45MB). 무거우면 파일별 (mtime, 크기)·읽은 위치를 캐시한다.
pub(crate) fn log_files(home: &Path, since: f64, limit: usize) -> Vec<PathBuf> {
    let root = paths::claude_root(home).join("projects");
    let walked = fsutil::walk_files(&root, &|p| p.extension().is_some_and(|e| e == "jsonl"));
    prune_memo(&root, &walked);
    let mut files: Vec<(PathBuf, f64)> = walked
        .into_iter()
        .filter_map(|p| {
            let modified = modified_secs(&p)?;
            (modified >= since).then_some((p, modified))
        })
        .collect();
    files.sort_by(|a, b| b.1.total_cmp(&a.1));
    files.into_iter().take(limit).map(|(p, _)| p).collect()
}

/// 구간 `[start, now]`의 요청 수와 (캐시 읽기, 전체 입력) 토큰. 같은 `message.id`는 처음 것만, 입력 0은 빼고 센다.
fn tally(requests: impl IntoIterator<Item = Request>, start: f64, now: f64) -> (u64, f64, f64) {
    let mut seen = HashSet::new();
    let mut tally = (0, 0.0, 0.0);
    for (ts, id, usage) in requests {
        if ts < start || ts > now || usage.total_input() <= 0.0 {
            continue;
        }
        if let Some(id) = id
            && !seen.insert(id)
        {
            continue;
        }
        tally.0 += 1;
        tally.1 += usage.cache_read_input_tokens;
        tally.2 += usage.total_input();
    }
    tally
}

fn history_windows(history_file: &Path, cut: f64, tuning: &Tuning) -> Vec<Window> {
    let samples = history::five_hour_records(history_file)
        .into_iter()
        .filter_map(|r| Some((unix_secs(r.resets_at?), r.used_percent)))
        .filter(|(reset, _)| *reset >= cut)
        .collect();
    windows(samples, tuning.advanced.reset_tol_secs as f64)
}

/// 요청(같은 `message.id`는 처음 것만, 입력 0은 뺀다). `tally`와 같은 정의.
fn unique_requests(requests: &[Request]) -> Vec<&Request> {
    let mut seen = HashSet::new();
    requests
        .iter()
        .filter(|(_, id, usage)| {
            usage.total_input() > 0.0 && id.as_ref().is_none_or(|id| seen.insert(id))
        })
        .collect()
}

fn request_times(requests: &[Request]) -> Vec<f64> {
    unique_requests(requests).iter().map(|r| r.0).collect()
}

/// 전후 비교(health, 0016): `since` 뒤에 바뀐 기록 파일 전부의 (시각, 전체 입력 토큰)과,
/// 그보다 오래된 파일이 있었는가(있으면 기록이 `since` 전부터 있다).
pub(crate) fn request_inputs(home: &Path, since: f64) -> (Vec<(f64, f64)>, bool) {
    let all = log_files(home, f64::NEG_INFINITY, usize::MAX);
    let recent: Vec<&PathBuf> = all
        .iter()
        .take_while(|f| modified_secs(f).is_some_and(|m| m >= since))
        .collect();
    let has_older = recent.len() < all.len();
    let requests: Vec<Request> = recent.iter().flat_map(|f| read_requests(f)).collect();
    let inputs = unique_requests(&requests)
        .iter()
        .map(|(ts, _, usage)| (*ts, usage.total_input()))
        .collect();
    (inputs, has_older)
}

/// 한도 기록의 주간(`sevenDay`) 표본 → (기록 시각, 증가분 %p).
pub(crate) fn weekly_used(history_file: &Path, tuning: &Tuning) -> Vec<(f64, f64)> {
    let samples = std::fs::read_to_string(history_file)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<history::HistoryEntry>(line).ok())
        .filter_map(|e| {
            let week = e.seven_day?;
            Some((
                unix_secs(e.at),
                unix_secs(week.resets_at?),
                week.used_percent,
            ))
        })
        .collect();
    weekly_increments(samples, tuning.advanced.reset_tol_secs as f64)
}

pub fn metrics(home: &Path, history_file: &Path, now: f64, tuning: &Tuning) -> AgentMetrics {
    let cut = now - tuning.bar_days as f64 * DAY_SECS;
    let windows = history_windows(history_file, cut, tuning);
    // 시간 버킷 기간의 기록을 한 번 읽어 지금 5시간 구간 집계에도 쓴다. 최근에 바뀐 순이라 5시간 구간의 파일이 먼저 들어간다.
    let requests: Vec<Request> = log_files(
        home,
        hourly_start(now, tuning.bar_days),
        tuning.claude_log_files,
    )
    .iter()
    .flat_map(|f| read_requests(f))
    .collect();
    let tallied = current(&windows, now).map(|w| tally(requests.iter().cloned(), w.start(), now));
    AgentMetrics {
        agent: Agent::Claude,
        recent_requests: tallied.map(|t| t.0),
        cache_hit_ratio: tallied.and_then(|t| ratio(t.1, t.2)),
        bar_unit: BarUnit::Percent,
        bars: percent_bars(&windows, now, tuning.recent_bars),
        hourly: hourly(
            now,
            tuning.bar_days,
            &request_times(&requests),
            &weekly_used(history_file, tuning),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::fsutil::testing::TempDir;
    use serde_json::json;

    fn assistant(ts: &str, id: &str, input: u64, read: u64, create: u64) -> String {
        json!({"type": "assistant", "timestamp": ts, "message": {"id": id,
            "content": [{"type": "text", "text": "secret body"}],
            "usage": {"input_tokens": input, "cache_read_input_tokens": read,
                      "cache_creation_input_tokens": create, "output_tokens": 9}}})
        .to_string()
    }

    #[test]
    fn reads_only_usage_numbers_from_assistant_lines() {
        let line = assistant("2026-11-05T01:00:00Z", "msg_1", 2, 90, 8);
        let e = entry(line.as_bytes(), false).unwrap();
        assert_eq!(
            e.ts,
            unix_secs(time::macros::datetime!(2026-11-05 01:00 UTC))
        );
        assert_eq!(e.id.as_deref(), Some("msg_1"));
        let usage = e.usage.unwrap();
        assert_eq!((usage.total_input(), usage.output_tokens), (100.0, 9.0));
        // user 줄, usage 없는 줄, 깨진 줄은 요청이 아니다.
        let user = json!({"type": "user", "timestamp": "2026-11-05T01:00:00Z",
            "message": {"content": "\"type\":\"assistant\" \"usage\""}})
        .to_string();
        assert!(entry(user.as_bytes(), false).is_none());
        assert!(entry(br#"{"type":"assistant","usage" broken"#, false).is_none());
    }

    #[test]
    fn reads_model_and_message_id_from_assistant_lines_only() {
        let line = json!({"type": "assistant", "timestamp": "2026-11-05T01:00:00Z",
            "message": {"id": "msg_1", "model": "claude-opus-5-5", "content": []}})
        .to_string();
        let e = entry(line.as_bytes(), false).unwrap();
        assert_eq!(
            (e.id.as_deref(), e.model.as_deref()),
            (Some("msg_1"), Some("claude-opus-5-5"))
        );
        let user = json!({"type": "user", "timestamp": "2026-11-05T01:00:00Z",
            "message": {"model": "x", "content": "\"type\":\"assistant\""}})
        .to_string();
        assert!(entry(user.as_bytes(), false).is_none());
    }

    #[test]
    fn tally_dedups_message_ids_and_skips_out_of_window_and_empty_requests() {
        let usage = |input, read, create| Usage {
            input_tokens: input,
            cache_read_input_tokens: read,
            cache_creation_input_tokens: create,
            output_tokens: 0.0,
        };
        let requests = vec![
            (10.0, Some("a".into()), usage(2.0, 90.0, 8.0)),
            (11.0, Some("a".into()), usage(2.0, 90.0, 8.0)), // 같은 message.id
            (12.0, None, usage(10.0, 0.0, 90.0)),
            (13.0, Some("b".into()), usage(0.0, 0.0, 0.0)), // 입력 0
            (5.0, Some("c".into()), usage(1.0, 1.0, 1.0)),  // 구간 전
            (30.0, Some("d".into()), usage(1.0, 1.0, 1.0)), // 지금 뒤
        ];
        assert_eq!(tally(requests, 10.0, 20.0), (2, 90.0, 200.0));
    }

    #[test]
    fn every_reader_shares_one_parse_per_unchanged_file() {
        let home = TempDir::new("metrics-claude-once");
        let now = OffsetDateTime::now_utc();
        let ts = now.format(&Rfc3339).unwrap();
        let line =
            json!({"type": "assistant", "timestamp": ts, "cwd": "/w", "message": {"id": "m1",
            "model": "claude-opus-5-5", "usage": {"input_tokens": 3, "output_tokens": 4}}})
            .to_string();
        let file = home.write(".claude/projects/-w/s.jsonl", &line);
        let history = home.write("data/limit-history.jsonl", "");
        let secs = unix_secs(now);
        let _ = metrics(home.path(), &history, secs, &Tuning::default());
        let (inputs, _) = request_inputs(home.path(), secs - DAY_SECS);
        let (models, _) = model_requests(home.path(), secs - DAY_SECS);
        assert_eq!((inputs.len(), models.len()), (1, 1));
        let parses = |f: &Path| PARSE_COUNTS.lock().unwrap().get(f).copied();
        let path = file.parent().unwrap().join("s.jsonl");
        assert_eq!(parses(&path).or(parses(&file)), Some(1));
        // 파일이 바뀌면(크기) 다시 읽는다.
        std::fs::write(&path, format!("{line}\n{line}")).unwrap();
        assert_eq!(model_requests(home.path(), secs - DAY_SECS).0.len(), 2);
        assert_eq!(parses(&path), Some(2));
    }

    #[test]
    fn subagent_effort_metadata_shares_the_memo_and_uses_directory_components() {
        let home = TempDir::new("metrics-subagent-effort");
        let ts = OffsetDateTime::now_utc().format(&Rfc3339).unwrap();
        let line = json!({"type":"assistant", "timestamp":ts, "effort":"xhigh",
            "attributionAgent":"Explore", "message":{"id":"one", "model":"claude-opus-5-5"}})
        .to_string();
        let file = home.write(".claude/projects/-w/session/subagents/a.jsonl", &line);
        let first = entries(&file);
        let second = entries(&file);
        assert!(Arc::ptr_eq(&first, &second));
        let meta = first[0].subagent.as_deref().unwrap();
        assert_eq!(meta.effort.as_deref(), Some("xhigh"));
        assert_eq!(meta.attribution_agent.as_deref(), Some("Explore"));
        let malformed = json!({"type":"assistant", "timestamp":ts, "effort":5,
            "attributionAgent":{}, "message":{"id":"old-reader", "model":"claude-opus-5-5"}})
        .to_string();
        let parsed = entry(malformed.as_bytes(), true).unwrap();
        let meta = parsed.subagent.as_deref().unwrap();
        assert!(meta.effort.is_none() && meta.attribution_agent.is_none());
        assert_eq!(parsed.model.as_deref(), Some("claude-opus-5-5"));
        assert!(first[0].subagent.is_some());
        let main = home.write(".claude/projects/-w/subagents-like/a.jsonl", &line);
        assert!(entries(&main)[0].subagent.is_none());
        // 조상 폴더 이름이 subagents여도 기록 파일 바로 위가 아니면 메인 기록이다.
        let nested = home.write(".claude/projects/-w/subagents/x/main.jsonl", &line);
        assert!(entries(&nested)[0].subagent.is_none());
        let since = unix_secs(OffsetDateTime::now_utc()) - DAY_SECS;
        assert_eq!(model_requests(home.path(), since).0.len(), 3);
        assert_eq!(recent_requests(home.path(), since).2.len(), 1);
        assert_eq!(PARSE_COUNTS.lock().unwrap().get(&file), Some(&1));
        std::fs::write(&file, format!("{line}\n{line}")).unwrap();
        assert_eq!(recent_requests(home.path(), since).2.len(), 2);
        assert_eq!(PARSE_COUNTS.lock().unwrap().get(&file), Some(&2));
    }

    #[test]
    fn metrics_use_the_running_history_window_and_recent_logs() {
        // 기록 파일은 방금 쓰였으므로(mtime = 지금) 시각을 지금 기준으로 잡는다.
        let home = TempDir::new("metrics-claude");
        let now = OffsetDateTime::now_utc();
        let rfc = |minutes: i64| {
            (now + time::Duration::minutes(minutes))
                .format(&Rfc3339)
                .unwrap()
        };
        // 지금 구간: 리셋 3시간 뒤 → 시작 2시간 전.
        let lines = [
            assistant(&rfc(-150), "old", 50, 50, 0), // 구간 시작 전
            assistant(&rfc(-60), "m1", 10, 80, 10),
            assistant(&rfc(-50), "m2", 20, 0, 80),
        ];
        home.write(".claude/projects/-w/s.jsonl", &lines.join("\n"));
        let record = |at: i64, pct: f64, reset: i64| {
            format!(
                r#"{{"at":"{}","fiveHour":{{"usedPercent":{pct},"resetsAt":"{}"}},"sevenDay":null}}"#,
                rfc(at),
                rfc(reset)
            )
        };
        let history = home.write(
            "data/limit-history.jsonl",
            &[
                record(-400, 30.0, -300),
                record(-60, 4.0, 180),
                record(-30, 7.0, 180),
            ]
            .join("\n"),
        );
        let got = metrics(home.path(), &history, unix_secs(now), &Tuning::default());
        assert_eq!(got.recent_requests, Some(2));
        assert_eq!(got.cache_hit_ratio, Some(80.0 / 200.0));
        let bars: Vec<(f64, bool)> = got.bars.iter().map(|b| (b.value, b.is_current)).collect();
        assert_eq!(bars, [(30.0, false), (7.0, true)]);
        // 시간 버킷은 5시간 구간 밖 요청도 센다. 주간 기록이 없으면 쓴 양 0.
        let requests: u64 = got.hourly.iter().map(|b| b.requests).sum();
        assert_eq!(requests, 3);
        assert!(got.hourly.iter().all(|b| b.used == 0.0));

        // 리셋이 지난 기록뿐이면 지금 구간을 모른다 → 숨김.
        let later = unix_secs(now + time::Duration::hours(4));
        let got = metrics(home.path(), &history, later, &Tuning::default());
        assert_eq!((got.recent_requests, got.cache_hit_ratio), (None, None));
    }
}
