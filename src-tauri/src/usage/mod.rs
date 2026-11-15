//! 에이전트별 남은 한도(docs/decisions/0003 "한도 출처", 0004).
//!
//! 토큰은 `sources`에서만 읽고 이 모듈 밖으로 나가지 않는다. 어떤 값·reason·오류에도 토큰,
//! Keychain 원문, 계정 id를 넣지 않는다. TS 쪽 짝은 `src/lib/ipc.ts`의 같은 이름 타입.

mod parse;
pub(crate) mod paths;
pub mod schedule;
pub(crate) mod sources;

use std::sync::Mutex;

use serde::Serialize;
use time::OffsetDateTime;

use crate::error::AppError;

pub const USAGE_UPDATED_EVENT: &str = "usage-updated";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Agent {
    Claude,
    Codex,
    Kiro,
}

impl Agent {
    pub const ALL: [Agent; 3] = [Agent::Claude, Agent::Codex, Agent::Kiro];

    pub fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Codex => "Codex",
            Self::Kiro => "Kiro",
        }
    }

    /// 직렬화 이름과 같은 id(`claude` 등). 설정 파일·트레이 id에 쓴다.
    pub fn id(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Kiro => "kiro",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.id() == id)
    }
}

/// 값을 어디서 읽었는가. 숫자 표기 규칙(0003): 모두 실측이다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UsageSource {
    Api,
    Log,
    Cli,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum WindowId {
    #[serde(rename = "5h")]
    FiveHour,
    #[serde(rename = "7d")]
    SevenDay,
    #[serde(rename = "month")]
    Month,
}

impl WindowId {
    pub fn label(self) -> &'static str {
        match self {
            Self::FiveHour => "5h",
            Self::SevenDay => "7d",
            Self::Month => crate::i18n::pick("월", "month"),
        }
    }
}

/// 한도 창 하나. 리셋 시각이 지난 창은 0%로 둔다(옛 값은 더 이상 맞지 않는다).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitWindow {
    pub id: WindowId,
    pub used_percent: f64,
    #[serde(with = "time::serde::rfc3339::option")]
    pub resets_at: Option<OffsetDateTime>,
    pub reset_passed: bool,
    /// Kiro 크레딧 사용량·한도.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<f64>,
}

impl LimitWindow {
    /// 보관 중인 값을 다시 보여 줄 때 그 사이 리셋이 지났는지 반영한다.
    fn at(mut self, now: OffsetDateTime) -> Self {
        if self.resets_at.is_some_and(|at| at <= now) {
            self.used_percent = 0.0;
            self.reset_passed = true;
        }
        self
    }

    /// 표시용 남은 % (0~100). 리셋이 지난 창은 100%.
    pub fn remaining_percent(&self, now: OffsetDateTime) -> f64 {
        if self.reset_passed || self.resets_at.is_some_and(|at| at <= now) {
            return 100.0;
        }
        (100.0 - self.used_percent).clamp(0.0, 100.0)
    }
}

/// `reason`의 종류. `Setup`은 사용자가 할 일(설치·로그인·허용)이라 화면이 차분하게 보이고,
/// `Failure`만 오류로 보인다. TS 짝은 `ipc.ts`의 `ReasonKind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasonKind {
    Setup,
    Failure,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentUsage {
    pub agent: Agent,
    /// 값이 없으면 `None`.
    pub source: Option<UsageSource>,
    /// 값이 기록된 시각(API·CLI는 조회 시각, log는 기록 시각).
    #[serde(with = "time::serde::rfc3339::option")]
    pub observed_at: Option<OffsetDateTime>,
    pub windows: Vec<LimitWindow>,
    /// 주 경로를 못 쓴 이유, 또는 값이 없는 이유.
    pub reason: Option<String>,
    /// `reason`이 있을 때만 `Some`.
    pub reason_kind: Option<ReasonKind>,
    /// 마지막으로 조회를 마친 시각. `None`이면 아직 첫 조회 중.
    #[serde(with = "time::serde::rfc3339::option")]
    pub checked_at: Option<OffsetDateTime>,
}

impl AgentUsage {
    pub(crate) fn pending(agent: Agent) -> Self {
        Self {
            agent,
            source: None,
            observed_at: None,
            windows: Vec::new(),
            reason: None,
            reason_kind: None,
            checked_at: None,
        }
    }

    /// 새 조회가 값을 못 얻으면 마지막 값을 유지하고 이유만 바꾼다(0003: Kiro "마지막 성공 값 유지").
    fn merged(previous: &Self, next: Self, now: OffsetDateTime) -> Self {
        if !next.windows.is_empty() || previous.windows.is_empty() {
            return next;
        }
        Self {
            source: previous.source,
            observed_at: previous.observed_at,
            windows: previous
                .windows
                .iter()
                .cloned()
                .map(|w| w.at(now))
                .collect(),
            ..next
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshot {
    pub agents: Vec<AgentUsage>,
}

impl AgentUsage {
    /// 가장 급한 창(남은 %가 가장 적은 창)의 남은 %. 값이 없으면 `None`.
    pub fn min_remaining(&self, now: OffsetDateTime) -> Option<f64> {
        self.windows
            .iter()
            .map(|w| w.remaining_percent(now))
            .reduce(f64::min)
    }

    /// 트레이 tooltip용 한 줄 요약(남은 % 기준).
    pub fn summary(&self, now: OffsetDateTime) -> String {
        let windows: Vec<String> = self
            .windows
            .iter()
            .map(|w| {
                let (label, left) = (w.id.label(), w.remaining_percent(now));
                crate::tr!("{label} {left:.0}% 남음", "{label} {left:.0}% left")
            })
            .collect();
        let values = if windows.is_empty() {
            "—".to_owned()
        } else {
            windows.join(" · ")
        };
        format!("{} {values}", self.agent.label())
    }
}

/// 최신 스냅샷. 타이머가 쓰고 `get_usage`가 읽는다. 잠금 안에서 I/O를 하지 않는다.
pub struct UsageState(Mutex<UsageSnapshot>);

impl Default for UsageState {
    fn default() -> Self {
        Self(Mutex::new(UsageSnapshot {
            agents: Agent::ALL.into_iter().map(AgentUsage::pending).collect(),
        }))
    }
}

impl UsageState {
    pub fn snapshot(&self) -> Result<UsageSnapshot, AppError> {
        self.0
            .lock()
            .map(|s| s.clone())
            .map_err(|_| AppError::Internal)
    }

    /// 한 에이전트 결과를 반영하고 새 스냅샷을 돌려준다.
    pub fn update(&self, next: AgentUsage, now: OffsetDateTime) -> Result<UsageSnapshot, AppError> {
        let mut guard = self.0.lock().map_err(|_| AppError::Internal)?;
        let agents = guard
            .agents
            .iter()
            .map(|a| {
                if a.agent == next.agent {
                    AgentUsage::merged(a, next.clone(), now)
                } else {
                    a.clone()
                }
            })
            .collect();
        *guard = UsageSnapshot { agents };
        Ok(guard.clone())
    }
}

/// 최신 스냅샷. 이후 값은 `usage-updated` event로 온다.
#[tauri::command]
pub fn get_usage(state: tauri::State<'_, UsageState>) -> Result<UsageSnapshot, AppError> {
    state.snapshot()
}

/// 이 컴퓨터에서 찾은 에이전트(`sources::is_installed`와 같은 규칙). 점검 탭이 "점검 못 함"과 "고칠 것 없음"을 가른다.
#[tauri::command]
pub fn get_installed_agents() -> Result<Vec<Agent>, AppError> {
    let home = std::env::home_dir().ok_or(AppError::Internal)?;
    let path_env = std::env::var_os("PATH");
    Ok(Agent::ALL
        .into_iter()
        .filter(|&agent| sources::is_installed(agent, &home, path_env.as_deref()))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn window(percent: f64, resets_at: OffsetDateTime) -> LimitWindow {
        LimitWindow {
            id: WindowId::Month,
            used_percent: percent,
            resets_at: Some(resets_at),
            reset_passed: false,
            used: None,
            limit: None,
        }
    }

    #[test]
    fn keeps_last_value_with_new_reason_when_refresh_fails() {
        let now = datetime!(2026-11-04 12:00 UTC);
        let state = UsageState::default();
        let ok = AgentUsage {
            source: Some(UsageSource::Cli),
            observed_at: Some(now),
            windows: vec![window(20.0, datetime!(2026-12-01 0:00 UTC))],
            checked_at: Some(now),
            ..AgentUsage::pending(Agent::Kiro)
        };
        state.update(ok, now).unwrap();
        let failed = AgentUsage {
            reason: Some("kiro-cli 시간 초과(30초)".into()),
            reason_kind: Some(ReasonKind::Failure),
            checked_at: Some(now),
            ..AgentUsage::pending(Agent::Kiro)
        };
        let kiro = state.update(failed, now).unwrap().agents[2].clone();
        assert_eq!(kiro.source, Some(UsageSource::Cli));
        assert_eq!(kiro.windows[0].used_percent, 20.0);
        assert_eq!(kiro.reason.as_deref(), Some("kiro-cli 시간 초과(30초)"));
    }

    #[test]
    fn kept_value_drops_to_zero_after_its_reset_passes() {
        let previous = AgentUsage {
            windows: vec![window(80.0, datetime!(2026-11-04 12:00 UTC))],
            ..AgentUsage::pending(Agent::Kiro)
        };
        let merged = AgentUsage::merged(
            &previous,
            AgentUsage::pending(Agent::Kiro),
            datetime!(2026-11-04 12:01 UTC),
        );
        assert_eq!(merged.windows[0].used_percent, 0.0);
        assert!(merged.windows[0].reset_passed);
    }

    #[test]
    fn remaining_percent_is_hundred_minus_used_and_full_after_reset() {
        let now = datetime!(2026-11-04 12:00 UTC);
        assert_eq!(
            window(2.0, datetime!(2026-11-04 13:00 UTC)).remaining_percent(now),
            98.0
        );
        assert_eq!(
            window(130.0, datetime!(2026-11-04 13:00 UTC)).remaining_percent(now),
            0.0
        );
        // 리셋 시각이 지났거나 reset_passed면 100%.
        assert_eq!(window(80.0, now).remaining_percent(now), 100.0);
        let passed = LimitWindow {
            reset_passed: true,
            ..window(80.0, datetime!(2026-12-01 0:00 UTC))
        };
        assert_eq!(passed.remaining_percent(now), 100.0);
    }

    #[test]
    fn summarizes_most_urgent_window_and_tooltip_as_remaining() {
        let now = datetime!(2026-11-04 11:00 UTC);
        let mut claude = AgentUsage::pending(Agent::Claude);
        assert_eq!(claude.min_remaining(now), None);
        assert_eq!(claude.summary(now), "Claude —");
        claude.windows = vec![
            LimitWindow {
                id: WindowId::FiveHour,
                ..window(12.4, datetime!(2026-11-04 12:00 UTC))
            },
            LimitWindow {
                id: WindowId::SevenDay,
                ..window(40.6, datetime!(2026-11-04 12:00 UTC))
            },
        ];
        assert_eq!(claude.min_remaining(now), Some(100.0 - 40.6));
        assert_eq!(claude.summary(now), "Claude 5h 88% 남음 · 7d 59% 남음");
    }

    #[test]
    fn agent_ids_round_trip() {
        for agent in Agent::ALL {
            assert_eq!(Agent::from_id(agent.id()), Some(agent));
            assert_eq!(serde_json::to_value(agent).unwrap(), agent.id());
        }
        assert_eq!(Agent::from_id("Claude"), None);
    }

    #[test]
    fn serializes_snapshot_in_camel_case_with_iso_times() {
        let usage = AgentUsage {
            windows: vec![window(1.0, datetime!(2026-12-01 0:00 UTC))],
            ..AgentUsage::pending(Agent::Kiro)
        };
        let json = serde_json::to_value(&usage).unwrap();
        assert_eq!(json["agent"], "kiro");
        assert_eq!(json["windows"][0]["id"], "month");
        assert_eq!(json["windows"][0]["resetsAt"], "2026-12-01T00:00:00Z");
        assert_eq!(json["windows"][0]["resetPassed"], false);
        assert!(json["observedAt"].is_null() && json["checkedAt"].is_null());
        assert!(json["reason"].is_null() && json["reasonKind"].is_null());
        let setup = AgentUsage {
            reason: Some("sign in".into()),
            reason_kind: Some(ReasonKind::Setup),
            ..usage
        };
        assert_eq!(serde_json::to_value(&setup).unwrap()["reasonKind"], "setup");
    }
}
