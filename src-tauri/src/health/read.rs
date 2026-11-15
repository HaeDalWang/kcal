//! 전후 비교에 쓸 기록 읽기. 기존 파서를 그대로 쓴다: Claude `metrics::claude`(assistant `usage`),
//! Codex `rate::codex`(`token_count`, 중복 제거), Kiro `rate::kiro`(턴 크레딧), Claude 주간 %p `limit-history.jsonl`.
//! 새로 읽는 것은 Codex rollout의 `type == "compacted"` 줄 시각뿐이다(ctxmeter `foldRollout`과 같은 줄).
//!
//! 전체 기간(`since` ~ 지금)을 한 번 읽어 구간마다 나눠 쓴다. `since` 전에 바뀐 파일은 읽지 않고,
//! 그런 파일이 있으면 기록이 그 전부터 있었다고 본다(시작 = −∞, 덮은 비율 1).
// ponytail: 매번 기간 안 파일을 통째로 읽는다(이 Mac 14일 Claude 약 300MB). 무거우면 파일별 (mtime, 크기)·결과를 캐시한다.

use std::collections::HashSet;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::context::kiro_session;
use crate::metrics::{claude, modified_secs, unix_secs, weekly_increments};
use crate::rate::codex::{self as rollout, char_prefix, contains};
use crate::rate::kiro::timed_credits;
use crate::tuning::Tuning;
use crate::usage::Agent;

const COMPACTED_MARK: &[u8] = br#""type":"compacted""#;
/// 접두 검사 길이(문자). `rate::codex`의 `token_count` 검사와 같다.
const PREFIX_CHARS: usize = 400;

/// 에이전트 하나의 기록(숫자만). 시각은 유닉스 초.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentSamples {
    pub agent: Agent,
    /// 요청: (시각, 값). Claude·Codex = 입력 토큰, Kiro = 크레딧.
    pub requests: Vec<(f64, f64)>,
    /// 요청 기록의 가장 이른 시각. 기록이 없으면 `None`, 읽지 않은 더 오래된 파일이 있으면 −∞.
    pub requests_from: Option<f64>,
    /// 쓴 양: (시각, 주간 한도 %p 증가분 또는 크레딧).
    pub used: Vec<(f64, f64)>,
    pub used_from: Option<f64>,
    /// 압축 시각(Codex만).
    pub compactions: Option<Vec<f64>>,
}

/// 가장 이른 시각. `has_older`면 −∞.
fn earliest(times: impl IntoIterator<Item = f64>, has_older: bool) -> Option<f64> {
    if has_older {
        return Some(f64::NEG_INFINITY);
    }
    times.into_iter().reduce(f64::min)
}

pub fn samples(
    home: &Path,
    history: &Path,
    agent: Agent,
    since: f64,
    tuning: &Tuning,
) -> AgentSamples {
    let tol = tuning.advanced.reset_tol_secs as f64;
    match agent {
        Agent::Claude => claude_samples(home, history, since, tuning),
        Agent::Codex => codex_samples(home, since, tol),
        Agent::Kiro => kiro_samples(home, since),
    }
}

fn claude_samples(home: &Path, history: &Path, since: f64, tuning: &Tuning) -> AgentSamples {
    let (requests, has_older) = claude::request_inputs(home, since);
    let used = claude::weekly_used(history, tuning);
    AgentSamples {
        agent: Agent::Claude,
        requests_from: earliest(requests.iter().map(|r| r.0), has_older),
        requests,
        used_from: earliest(used.iter().map(|u| u.0), false),
        used,
        compactions: None,
    }
}

fn codex_samples(home: &Path, since: f64, reset_tol_secs: f64) -> AgentSamples {
    let (recent, older): (Vec<PathBuf>, Vec<PathBuf>) = rollout::rollout_files(home)
        .into_iter()
        .partition(|f| modified_secs(f).is_some_and(|m| m >= since));
    let has_older = !older.is_empty();
    let events = rollout::events(&recent);
    let weekly: Vec<(f64, f64, f64)> = events
        .iter()
        .filter_map(|e| Some((e.ts, e.weekly_resets_at?, e.weekly_used_percent?)))
        .collect();
    let mut seen = HashSet::new();
    let compactions = recent
        .iter()
        .flat_map(|f| compaction_times(f))
        .filter(|(tsr, _)| seen.insert(tsr.clone()))
        .map(|(_, ts)| ts)
        .collect();
    AgentSamples {
        agent: Agent::Codex,
        requests: events
            .iter()
            .filter(|e| e.input > 0.0)
            .map(|e| (e.ts, e.input))
            .collect(),
        requests_from: earliest(events.iter().map(|e| e.ts), has_older),
        used_from: earliest(weekly.iter().map(|w| w.0), has_older),
        used: weekly_increments(weekly, reset_tol_secs),
        compactions: Some(compactions),
    }
}

fn kiro_samples(home: &Path, since: f64) -> AgentSamples {
    let files = kiro_session::message_files(home);
    // 최근에 바뀐 순이라 기간 전에 바뀐 파일이 나오면 그 뒤는 볼 필요가 없다.
    let recent: Vec<&PathBuf> = files
        .iter()
        .take_while(|f| modified_secs(f).is_some_and(|m| m >= since))
        .collect();
    let has_older = recent.len() < files.len();
    let turns: Vec<(f64, f64)> = recent
        .iter()
        .flat_map(|f| timed_credits(f))
        .filter_map(|(ts, credit)| Some((ts?, credit)))
        .collect();
    let from = earliest(turns.iter().map(|t| t.0), has_older);
    AgentSamples {
        agent: Agent::Kiro,
        requests: turns.clone(),
        requests_from: from,
        used: turns,
        used_from: from,
        compactions: None,
    }
}

/// `compacted` 줄에서 꺼내는 것: 종류와 시각뿐(`payload`의 대화 내용은 serde가 건너뛴다).
#[derive(Deserialize)]
struct Row {
    #[serde(rename = "type")]
    kind: Option<String>,
    timestamp: Option<String>,
}

/// rollout 하나의 압축 (원문 timestamp, 시각). 접두 검사를 통과한 줄만 JSON으로 읽는다.
pub(super) fn compaction_times(file: &Path) -> Vec<(String, f64)> {
    let Ok(handle) = std::fs::File::open(file) else {
        return Vec::new();
    };
    let mut reader = BufReader::new(handle);
    let mut line = Vec::new();
    let mut times = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        if !contains(char_prefix(&line, PREFIX_CHARS), COMPACTED_MARK) {
            continue;
        }
        let Ok(row) = serde_json::from_slice::<Row>(&line) else {
            continue;
        };
        if row.kind.as_deref() != Some("compacted") {
            continue;
        }
        if let Some(tsr) = row.timestamp
            && let Ok(ts) = OffsetDateTime::parse(&tsr, &Rfc3339)
        {
            times.push((tsr, unix_secs(ts)));
        }
    }
    times
}
