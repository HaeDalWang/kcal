//! Codex 5시간(primary) 한도 환율. `~/.local/share/agent-usage-audit/codex_rate.py`의 회귀 B
//! (창 단위, `out_total`)만 옮겼다: 수집 → 중복 제거 → 창 배정 → 창 단위 NNLS(절편 없음) → 검증 R².
//!
//! 본문 줄은 JSON 파싱조차 하지 않는다: 앞 400자 안에 `"token_count"`가 있는 줄만 파싱하고,
//! 그 줄에서도 숫자 필드(`timestamp`, `info.*_token_usage`, `rate_limits.primary`)만 꺼낸다.
//! Codex가 열흘쯤 지나 압축한 `.jsonl.zst`도 풀어 읽는다(0026).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::Value;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::fit::{self, Fit};
use super::{Bounds, CodexRate, RateStatus, Spread, TokenRange, TokenRate, WindowPattern};
use crate::context::{fsutil, paths};
use crate::tuning::AdvancedTuning;

const TOKEN_COUNT_MARK: &[u8] = br#""token_count""#;
/// 접두 검사 길이(문자). 파이썬 `line[:400]`.
const PREFIX_CHARS: usize = 400;
/// 변수 순서: 비캐시 입력, 캐시 입력, 출력.
pub(super) const VARIABLES: usize = 3;

#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub ts: f64,
    /// 원문 timestamp(중복 판정 키).
    pub tsr: String,
    /// rollout 파일 이름(경로 없이).
    pub file: String,
    pub input: f64,
    pub cached: f64,
    pub output: f64,
    /// `total_token_usage`를 키 순으로 정렬해 이은 문자열(중복·반복 판정 키).
    pub total_key: String,
    pub used_percent: Option<f64>,
    pub resets_at: Option<f64>,
    /// 주간(`rate_limits.secondary`) 사용 %·리셋 시각. 대시보드 하루 사용량(metrics, 0015)에만 쓴다.
    pub weekly_used_percent: Option<f64>,
    pub weekly_resets_at: Option<f64>,
}

/// Codex가 오래된 rollout을 압축한 이름 끝(이 Mac 실측, 2026-11-18).
const ZST: &str = ".jsonl.zst";

fn is_rollout(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with("rollout-") && (n.ends_with(".jsonl") || n.ends_with(ZST)))
}

/// `sessions/`·`archived_sessions/` 아래 `rollout-*.jsonl`. 전체 경로 문자열 순(파이썬 `sorted`).
pub fn rollout_files(home: &Path) -> Vec<PathBuf> {
    let root = paths::codex_root(home);
    let mut files: Vec<PathBuf> = ["sessions", "archived_sessions"]
        .iter()
        .flat_map(|dir| fsutil::walk_files(&root.join(dir), &is_rollout))
        .collect();
    files.sort_by_cached_key(|p| p.to_string_lossy().into_owned());
    files.dedup();
    files
}

/// 앞 `n`자(UTF-8 문자 경계)까지의 바이트.
pub(crate) fn char_prefix(line: &[u8], n: usize) -> &[u8] {
    let mut chars = 0;
    for (i, &b) in line.iter().enumerate() {
        if b & 0xC0 != 0x80 {
            if chars == n {
                return &line[..i];
            }
            chars += 1;
        }
    }
    line
}

pub(crate) fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// 파이썬 truthiness: null·false·0·빈 문자열·빈 배열·빈 객체는 거짓.
fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64() != Some(0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

fn number(value: Option<&Value>, key: &str) -> f64 {
    value
        .and_then(|v| v.get(key))
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
}

fn total_key(total: Option<&Value>) -> String {
    let mut pairs: Vec<(String, String)> = total
        .and_then(Value::as_object)
        .map(|o| o.iter().map(|(k, v)| (k.clone(), v.to_string())).collect())
        .unwrap_or_default();
    pairs.sort();
    pairs
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// `token_count` 이벤트 하나. info·rate_limits가 없거나 `limit_id`가 `codex`가 아니면 `None`.
fn event(row: &Value, file: &str) -> Option<Event> {
    if row.get("type")?.as_str()? != "event_msg" {
        return None;
    }
    let payload = row.get("payload")?;
    if payload.get("type")?.as_str()? != "token_count" {
        return None;
    }
    let (info, limits) = (payload.get("info"), payload.get("rate_limits"));
    if !truthy(info) || !truthy(limits) {
        return None;
    }
    let limits = limits?;
    let primary = limits.get("primary");
    if limits.get("limit_id").and_then(Value::as_str) != Some("codex") || !truthy(primary) {
        return None;
    }
    let tsr = row.get("timestamp")?.as_str()?;
    let ts = OffsetDateTime::parse(tsr, &Rfc3339).ok()?;
    let last = info?.get("last_token_usage");
    let secondary = limits.get("secondary");
    Some(Event {
        ts: ts.unix_timestamp_nanos() as f64 / 1e9,
        tsr: tsr.to_owned(),
        file: file.to_owned(),
        input: number(last, "input_tokens"),
        cached: number(last, "cached_input_tokens"),
        output: number(last, "output_tokens"),
        total_key: total_key(info?.get("total_token_usage")),
        used_percent: primary?.get("used_percent").and_then(Value::as_f64),
        resets_at: primary?.get("resets_at").and_then(Value::as_f64),
        weekly_used_percent: secondary
            .and_then(|s| s.get("used_percent"))
            .and_then(Value::as_f64),
        weekly_resets_at: secondary
            .and_then(|s| s.get("resets_at"))
            .and_then(Value::as_f64),
    })
}

/// 파일 하나의 이벤트. 접두 검사를 통과한 줄만 JSON으로 읽는다. 잘린 줄은 건너뛴다.
pub fn read_events(file: &Path) -> Vec<Event> {
    read_rollout(file).0
}

/// 이벤트와 작업 폴더 경로(첫 줄 `session_meta`의 `cwd`). 같은 읽기 루프에서 첫 줄만 더 본다.
pub(super) fn read_rollout(file: &Path) -> (Vec<Event>, Option<String>) {
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let Ok(handle) = std::fs::File::open(file) else {
        return (Vec::new(), None);
    };
    let source: Box<dyn std::io::Read> = if name.ends_with(ZST) {
        // 압축 머리가 깨졌으면 읽을 것이 없다. 중간에 깨지면 그 앞 줄까지만 쓴다(아래 Err에서 멈춤).
        match ruzstd::decoding::StreamingDecoder::new(handle) {
            Ok(decoder) => Box::new(decoder),
            Err(_) => return (Vec::new(), None),
        }
    } else {
        Box::new(handle)
    };
    let mut reader = BufReader::new(source);
    let mut line = Vec::new();
    let mut events = Vec::new();
    let mut session = None;
    let mut is_first = true;
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        if std::mem::take(&mut is_first) {
            session = super::behavior::session_cwd(&line);
        }
        if !contains(char_prefix(&line, PREFIX_CHARS), TOKEN_COUNT_MARK) {
            continue;
        }
        if let Ok(row) = serde_json::from_slice::<Value>(&line)
            && let Some(e) = event(&row, &name)
        {
            events.push(e);
        }
    }
    (events, session)
}

/// 시간순(같으면 파일 이름순)으로 정렬하고 `(timestamp, total_token_usage)`가 같은 이벤트를 지운다.
/// 같은 rollout에서 `total_token_usage`가 직전과 같으면 새 토큰이 없는 반복 보고라 토큰을 0으로 둔다.
pub fn dedup(events: &[Event]) -> Vec<Event> {
    let mut sorted = events.to_vec();
    sorted.sort_by(|a, b| a.ts.total_cmp(&b.ts).then_with(|| a.file.cmp(&b.file)));
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut last_total: HashMap<String, String> = HashMap::new();
    sorted
        .into_iter()
        .filter(|e| seen.insert((e.tsr.clone(), e.total_key.clone())))
        .map(|e| {
            let is_repeat = last_total.get(&e.file) == Some(&e.total_key);
            last_total.insert(e.file.clone(), e.total_key.clone());
            if is_repeat {
                Event {
                    input: 0.0,
                    cached: 0.0,
                    output: 0.0,
                    ..e
                }
            } else {
                e
            }
        })
        .collect()
}

/// 정렬된 값마다 묶음 번호. 직전 값과 `reset_tol_secs`(관측된 흔들림은 최대 ~18초) 이내면 같은 묶음.
pub fn cluster_ids(sorted: &[f64], reset_tol_secs: f64) -> Vec<usize> {
    let mut ids = Vec::with_capacity(sorted.len());
    let mut current = 0;
    for (i, value) in sorted.iter().enumerate() {
        if i > 0 && value - sorted[i - 1] > reset_tol_secs {
            current += 1;
        }
        ids.push(current);
    }
    ids
}

/// `resets_at`을 묶어 창을 만든다. 반환: (창 목록(시간순, 창 안은 ts순), 묶음 수).
/// 묶음 수 − 창 수 = `used_percent`가 있는 이벤트가 하나도 없어 제외된 창.
pub fn assign_windows(events: &[Event], reset_tol_secs: f64) -> (Vec<Vec<Event>>, usize) {
    let mut resets: Vec<f64> = events.iter().filter_map(|e| e.resets_at).collect();
    resets.sort_by(f64::total_cmp);
    resets.dedup();
    let ids = cluster_ids(&resets, reset_tol_secs);
    let clusters = ids.last().map_or(0, |last| last + 1);
    let mut windows: BTreeMap<usize, Vec<Event>> = BTreeMap::new();
    for e in events {
        let (Some(_), Some(at)) = (e.used_percent, e.resets_at) else {
            continue;
        };
        if let Ok(i) = resets.binary_search_by(|r| r.total_cmp(&at)) {
            windows.entry(ids[i]).or_default().push(e.clone());
        }
    }
    let windows = windows
        .into_values()
        .map(|mut w| {
            w.sort_by(|a, b| a.ts.total_cmp(&b.ts));
            w
        })
        .collect();
    (windows, clusters)
}

/// 창 하나의 변수 합과 목표(창 안 최대 `used_percent`).
pub(super) fn window_row(window: &[Event]) -> (Vec<f64>, f64) {
    let mut sums = [0.0; VARIABLES];
    for e in window {
        sums[0] += (e.input - e.cached).max(0.0);
        sums[1] += e.cached;
        sums[2] += e.output;
    }
    let y = window
        .iter()
        .filter_map(|e| e.used_percent)
        .fold(f64::NEG_INFINITY, f64::max);
    (sums.to_vec(), y)
}

pub struct WindowFit {
    pub windows: usize,
    pub excluded_windows: usize,
    /// 표본 부족이면 `None`. 특이 행렬이면 `Some(None)`.
    pub fit: Option<Option<Fit>>,
}

pub fn window_fit(events: &[Event], tuning: &AdvancedTuning) -> WindowFit {
    let (windows, clusters) = assign_windows(events, tuning.reset_tol_secs as f64);
    let excluded_windows = clusters - windows.len();
    if windows.len() < tuning.min_samples_per_var * VARIABLES {
        return WindowFit {
            windows: windows.len(),
            excluded_windows,
            fit: None,
        };
    }
    let (rows, y): (Vec<Vec<f64>>, Vec<f64>) = windows.iter().map(|w| window_row(w)).unzip();
    let groups: Vec<usize> = (0..rows.len()).collect();
    WindowFit {
        windows: windows.len(),
        excluded_windows,
        fit: Some(fit::fit_eval(&rows, &y, &groups, tuning.holdout_train)),
    }
}

/// 계수(%p/token) → 1%p당 토큰. 0이면 `None`.
fn per_percent(coef: f64) -> Option<f64> {
    (coef > 0.0).then(|| 1.0 / coef)
}

fn token_values<T>(fit: &Fit, pick: impl Fn(usize) -> T, missing: T) -> [T; VARIABLES]
where
    T: Copy,
{
    let mut out = [missing; VARIABLES];
    for (pos, &var) in fit.used.iter().enumerate() {
        out[var] = pick(pos);
    }
    out
}

fn rate_of(fit: &Fit) -> TokenRate {
    let [uncached_input, cached_input, output] =
        token_values(fit, |pos| per_percent(fit.coef[pos]), None);
    TokenRate {
        cached_input,
        uncached_input,
        output,
    }
}

fn range_of(fit: &Fit) -> TokenRange {
    let none = Bounds {
        min: None,
        max: None,
    };
    let [uncached_input, cached_input, output] = token_values(
        fit,
        |pos| {
            fit.fold_range.get(pos).map_or(none, |&(lo, hi)| Bounds {
                min: per_percent(hi),
                max: per_percent(lo),
            })
        },
        none,
    );
    TokenRange {
        cached_input,
        uncached_input,
        output,
    }
}

/// 판정 기준은 `tuning`: 검증 R²가 `good_r2` 이상이면 환율, `range_r2` 이상이면 범위만.
pub fn rate(events: &[Event], tuning: &AdvancedTuning) -> CodexRate {
    if events.is_empty() {
        return CodexRate::status(RateStatus::NoData, 0, 0);
    }
    judged(window_fit(events, tuning), tuning)
}

/// 창 단위 회귀 결과의 판정. Claude(`claude::rate`)도 같은 기준을 쓴다.
pub(super) fn judged(result: WindowFit, tuning: &AdvancedTuning) -> CodexRate {
    let base = CodexRate::status(
        RateStatus::Insufficient,
        result.windows,
        result.excluded_windows,
    );
    let Some(fit) = result.fit else {
        return base;
    };
    let Some(fit) = fit else {
        return CodexRate {
            status: RateStatus::Unreliable,
            ..base
        };
    };
    let validation = fit.validation();
    let r2 = (!validation.is_nan()).then_some(validation);
    if validation >= tuning.good_r2 {
        CodexRate {
            status: RateStatus::Estimated,
            r2,
            per_percent: Some(rate_of(&fit)),
            fold_range: Some(range_of(&fit)),
            ..base
        }
    } else if validation >= tuning.range_r2 {
        CodexRate {
            status: RateStatus::RangeOnly,
            r2,
            fold_range: Some(range_of(&fit)),
            ..base
        }
    } else {
        CodexRate {
            status: RateStatus::Unreliable,
            r2,
            ..base
        }
    }
}

/// 정렬된 값의 중앙값(파이썬 `statistics.median`). 빈 목록은 받지 않는다.
fn median(sorted: &[f64]) -> f64 {
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted[mid]
    } else {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    }
}

/// 정렬된 값의 p90: 파이썬 `statistics.quantiles(xs, n=100, method="inclusive")[89]`를
/// 같은 정수 연산 순서로 옮겼다(선형 보간). 원소 1개면 그 값(프로토타입 `pct`).
fn p90(sorted: &[f64]) -> f64 {
    const N: usize = 100;
    const I: usize = 90;
    if sorted.len() == 1 {
        return sorted[0];
    }
    let (j, delta) = ((I * (sorted.len() - 1)) / N, (I * (sorted.len() - 1)) % N);
    (sorted[j] * (N - delta) as f64 + sorted[j + 1] * delta as f64) / N as f64
}

pub(super) fn spread(counts: &[usize]) -> Spread {
    let mut sorted: Vec<f64> = counts.iter().map(|&c| c as f64).collect();
    sorted.sort_by(f64::total_cmp);
    Spread {
        median: median(&sorted),
        p90: p90(&sorted),
    }
}

pub(super) const DAY_SECS: f64 = 86_400.0;

/// 최근 `days`일 요청. `dedup`이 반복 보고의 토큰을 0으로 두므로 `input > 0`이 파이썬의
/// `not repeat and inp > 0`과 같다.
pub(super) fn recent_requests(events: &[Event], now: f64, days: u64) -> Vec<Event> {
    let cut = now - days as f64 * DAY_SECS;
    events
        .iter()
        .filter(|e| e.ts >= cut && e.input > 0.0)
        .cloned()
        .collect()
}

/// 최근 `days`일 창마다 요청 수 N과 캐시 미스 수 M. 프로토타입 `decompose.py`의 `codex_usage`와
/// 연산 순서가 같다: 기간 → 요청(반복 보고 아님, 입력 > 0) → 창 배정 → 창별 N·M. 창이 없으면 `None`.
pub fn pattern(
    events: &[Event],
    now: f64,
    days: u64,
    tuning: &AdvancedTuning,
) -> Option<WindowPattern> {
    let requests = recent_requests(events, now, days);
    let (windows, _) = assign_windows(&requests, tuning.reset_tol_secs as f64);
    if windows.is_empty() {
        return None;
    }
    let n: Vec<usize> = windows.iter().map(Vec::len).collect();
    let m: Vec<usize> = windows
        .iter()
        .map(|w| {
            w.iter()
                .filter(|e| e.cached / e.input < tuning.cache_miss_ratio)
                .count()
        })
        .collect();
    Some(WindowPattern {
        days,
        windows: windows.len(),
        requests: spread(&n),
        misses: spread(&m),
    })
}

/// 모든 rollout을 읽어 중복을 지운 이벤트.
pub fn events(files: &[PathBuf]) -> Vec<Event> {
    let raw: Vec<Event> = files.iter().flat_map(|f| read_events(f)).collect();
    dedup(&raw)
}

#[cfg(test)]
#[path = "codex_tests.rs"]
mod tests;
