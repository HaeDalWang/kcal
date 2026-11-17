//! Claude 5시간 한도 환율(docs/decisions/0006 "2026-11-11 Claude"). Codex와 같은 창 단위 NNLS(`fit`)와
//! 판정(`codex::judged`)을 쓴다. 패리티 기준: `~/.local/share/agent-usage-audit/claude_rate.py`.
//!
//! - 요청: `~/.claude/projects/**/*.jsonl` assistant 줄의 `message.usage` 숫자만 읽는다(`metrics::claude`와
//!   같은 표식·walker). 같은 `message.id`는 출력 토큰이 가장 큰 줄 하나(같으면 먼저 읽은 줄)만 센다.
//! - 창: 한도 기록(`history`)의 `fiveHour.resetsAt`을 `resetTolSecs`로 묶는다. y = 창 안 최대 %,
//!   토큰 = [max(리셋 − 5시간, 직전 창 리셋), 그 최대 %를 처음 기록한 시각] 안 요청의 합.
//!   기록된 요청이 없는 창은 회귀에서 뺀다(`excludedWindows`).
//! - 변수(Codex와 같은 3개): 비캐시 입력 = `input_tokens` + `cache_creation_input_tokens`(캐시에서 읽지 않은 입력),
//!   캐시 입력 = `cache_read_input_tokens`, 출력 = `output_tokens`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::codex::{self, DAY_SECS, VARIABLES, WindowFit};
use super::fit;
use super::history::HistoryEntry;
use super::{CodexRate, RateStatus, WindowPattern};
use crate::metrics::claude::{Entry, entries, log_files};
use crate::metrics::unix_secs;
use crate::tuning::AdvancedTuning;

const WINDOW_SECS: f64 = 5.0 * 3600.0;

/// 요청 하나의 토큰(유닉스 초 시각).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Request {
    pub ts: f64,
    pub input: f64,
    pub cache_read: f64,
    pub cache_write: f64,
    pub output: f64,
}

impl Request {
    fn total_input(&self) -> f64 {
        self.input + self.cache_read + self.cache_write
    }
}

/// 요청 하나와 그 `message.id`(usage가 있는 assistant 줄만). 파일 파싱은 `metrics::claude::entries`가 나눠 쓴다.
fn request(e: &Entry) -> Option<(Option<String>, Request)> {
    let usage = e.usage?;
    Some((
        e.id.clone(),
        Request {
            ts: e.ts,
            input: usage.input_tokens,
            cache_read: usage.cache_read_input_tokens,
            cache_write: usage.cache_creation_input_tokens,
            output: usage.output_tokens,
        },
    ))
}

/// `~/.claude/projects/**/*.jsonl` 전부. 전체 경로 문자열 순(파이썬 `sorted(glob)`).
pub fn files(home: &Path) -> Vec<PathBuf> {
    let mut files = log_files(home, f64::NEG_INFINITY, usize::MAX);
    files.sort_by_cached_key(|p| p.to_string_lossy().into_owned());
    files
}

/// 같은 `message.id`는 출력이 가장 큰 줄 하나(스트리밍 중간 줄은 출력이 덜 찼다). id가 없으면 줄마다 요청. 시간순.
pub fn requests(files: &[PathBuf]) -> Vec<Request> {
    let mut by_id: HashMap<String, usize> = HashMap::new();
    let mut out: Vec<Request> = Vec::new();
    for (id, r) in files
        .iter()
        .flat_map(|f| entries(f).iter().filter_map(request).collect::<Vec<_>>())
    {
        let Some(id) = id else {
            out.push(r);
            continue;
        };
        match by_id.get(&id) {
            Some(&i) if r.output > out[i].output => out[i] = r,
            Some(_) => {}
            None => {
                by_id.insert(id, out.len());
                out.push(r);
            }
        }
    }
    out.sort_by(|a, b| a.ts.total_cmp(&b.ts));
    out
}

/// 한도 기록의 5시간 창 하나.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LimitWindow {
    /// max(리셋 − 5시간, 직전 창 리셋).
    pub start: f64,
    /// 묶음에서 가장 이른 리셋 시각(초 단위로 자름).
    pub reset: f64,
    /// 창 안 최대 %를 처음 기록한 시각. 그 뒤 요청은 기록된 최대 %에 들어갈 수 없다.
    pub until: f64,
    pub used_percent: f64,
}

/// 기록 파일의 5시간 창(시간순). 파일이 없거나 깨진 줄은 건너뛴다.
pub fn windows(history_file: &Path, reset_tol_secs: f64) -> Vec<LimitWindow> {
    // (기록 시각, 리셋, %). 파일 순서를 지킨다: 같은 최대 %는 먼저 기록된 시각이 `until`이다.
    let records: Vec<(f64, f64, f64)> = std::fs::read_to_string(history_file)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<HistoryEntry>(line).ok())
        .filter_map(|e| {
            let five = e.five_hour?;
            Some((
                unix_secs(e.at),
                five.resets_at?.unix_timestamp() as f64,
                five.used_percent,
            ))
        })
        .collect();
    let mut resets: Vec<f64> = records.iter().map(|r| r.1).collect();
    resets.sort_by(f64::total_cmp);
    resets.dedup();
    let ids = codex::cluster_ids(&resets, reset_tol_secs);
    let count = ids.last().map_or(0, |last| last + 1);
    let mut found: Vec<Option<LimitWindow>> = vec![None; count];
    for &(at, reset, pct) in &records {
        let Ok(i) = resets.binary_search_by(|r| r.total_cmp(&reset)) else {
            continue;
        };
        let w = found[ids[i]].get_or_insert(LimitWindow {
            start: 0.0,
            reset,
            until: at,
            used_percent: f64::NEG_INFINITY,
        });
        w.reset = w.reset.min(reset);
        if pct > w.used_percent {
            w.used_percent = pct;
            w.until = at;
        }
    }
    let mut out: Vec<LimitWindow> = found.into_iter().flatten().collect();
    for i in 0..out.len() {
        let earliest = out[i].reset - WINDOW_SECS;
        out[i].start = if i == 0 {
            earliest
        } else {
            earliest.max(out[i - 1].reset)
        };
    }
    out
}

/// 창마다 [start, until] 안 요청의 (비캐시 입력, 캐시 입력, 출력) 합. 요청이 없으면 `None`.
fn window_row(requests: &[Request], w: &LimitWindow) -> Option<Vec<f64>> {
    let inside = requests
        .iter()
        .filter(|r| w.start <= r.ts && r.ts <= w.until);
    let mut sums = [0.0; VARIABLES];
    let mut has_request = false;
    for r in inside {
        has_request = true;
        sums[0] += r.input + r.cache_write;
        sums[1] += r.cache_read;
        sums[2] += r.output;
    }
    has_request.then(|| sums.to_vec())
}

/// Codex `rate`와 같은 판정. 기록된 요청이 있는 창이 하나도 없으면 `NoData`.
pub fn rate(requests: &[Request], windows: &[LimitWindow], tuning: &AdvancedTuning) -> CodexRate {
    let (rows, y): (Vec<Vec<f64>>, Vec<f64>) = windows
        .iter()
        .filter_map(|w| Some((window_row(requests, w)?, w.used_percent)))
        .unzip();
    let excluded_windows = windows.len() - rows.len();
    if rows.is_empty() {
        return CodexRate::status(RateStatus::NoData, 0, excluded_windows);
    }
    let fit = (rows.len() >= tuning.min_samples_per_var * VARIABLES).then(|| {
        let groups: Vec<usize> = (0..rows.len()).collect();
        fit::fit_eval(&rows, &y, &groups, tuning.holdout_train)
    });
    codex::judged(
        WindowFit {
            windows: rows.len(),
            excluded_windows,
            fit,
        },
        tuning,
    )
}

/// 최근 `days`일 창마다 요청 수 N과 캐시 미스 수 M(`cache_read / 전체 입력 < cacheMissRatio`).
/// 요청은 입력 > 0이고 [start, 리셋] 안, 기간 시작 뒤. 창이 없으면 `None`.
pub fn pattern(
    requests: &[Request],
    windows: &[LimitWindow],
    now: f64,
    days: u64,
    tuning: &AdvancedTuning,
) -> Option<WindowPattern> {
    let cut = now - days as f64 * DAY_SECS;
    let (n, m): (Vec<usize>, Vec<usize>) = windows
        .iter()
        .filter(|w| w.reset >= cut)
        .filter_map(|w| {
            let inside: Vec<&Request> = requests
                .iter()
                .filter(|r| {
                    w.start <= r.ts && r.ts <= w.reset && r.ts >= cut && r.total_input() > 0.0
                })
                .collect();
            let misses = inside
                .iter()
                .filter(|r| r.cache_read / r.total_input() < tuning.cache_miss_ratio)
                .count();
            (!inside.is_empty()).then_some((inside.len(), misses))
        })
        .unzip();
    (!n.is_empty()).then(|| WindowPattern {
        days,
        windows: n.len(),
        requests: codex::spread(&n),
        misses: codex::spread(&m),
    })
}

#[cfg(test)]
#[path = "claude_tests.rs"]
mod tests;
