//! 사용자가 조절할 수 있는 값(docs/decisions/0007). `settings.json`의 `tuning` 객체.
//! TS 쪽 짝은 `src/lib/ipc.ts`의 `Tuning`·`AdvancedTuning`.
//!
//! 기본값은 예전 상수와 같다. 파일을 읽을 때는 필드마다 검증해 잘못된 필드만 기본값으로 되돌리고,
//! `set_settings`로 들어온 값은 하나라도 범위 밖이면 통째로 거부한다.

use std::ops::RangeInclusive;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

const SESSION_IDLE_HOURS_RANGE: RangeInclusive<u64> = 1..=720;
const SESSION_CHILD_CPU_PERCENT_RANGE: RangeInclusive<f64> = 0.1..=100.0;

const MINUTE: u64 = 60;
const DAY_SECS: u64 = 24 * 60 * 60;

// ---- 범위(양 끝 포함) ----
const API_INTERVAL_SECS_RANGE: RangeInclusive<u64> = MINUTE..=DAY_SECS;
const KIRO_INTERVAL_SECS_RANGE: RangeInclusive<u64> = MINUTE..=DAY_SECS;
const MAX_BACKOFF_SECS_RANGE: RangeInclusive<u64> = MINUTE..=DAY_SECS;
const RATES_TTL_SECS_RANGE: RangeInclusive<u64> = MINUTE..=DAY_SECS;
const CONTEXT_TTL_SECS_RANGE: RangeInclusive<u64> = MINUTE..=DAY_SECS;
const HTTP_TIMEOUT_SECS_RANGE: RangeInclusive<u64> = 1..=5 * MINUTE;
const KEYCHAIN_TIMEOUT_SECS_RANGE: RangeInclusive<u64> = 1..=5 * MINUTE;
const KIRO_TIMEOUT_SECS_RANGE: RangeInclusive<u64> = 1..=5 * MINUTE;
const SESSIONS_RANGE: RangeInclusive<usize> = 1..=500;
const CLAUDE_LOG_TAIL_BYTES_RANGE: RangeInclusive<u64> = 4 * 1024..=16 * 1024 * 1024;
const BYTES_PER_TOKEN_RANGE: RangeInclusive<u64> = 1..=16;
const R2_RANGE: RangeInclusive<f64> = 0.0..=1.0;
const RESET_TOL_SECS_RANGE: RangeInclusive<u64> = 1..=3600;
/// 1.0이면 시험 구간이 비고, 0이면 학습 구간이 빈다.
const HOLDOUT_TRAIN_RANGE: RangeInclusive<f64> = 0.1..=0.9;
const CLAUDE_RATE_DAYS_RANGE: RangeInclusive<u64> = 3..=90;
const MIN_SAMPLES_PER_VAR_RANGE: RangeInclusive<usize> = 1..=20;
const BREAKDOWN_DAYS_RANGE: RangeInclusive<u64> = 1..=90;
const CACHE_MISS_RATIO_RANGE: RangeInclusive<f64> = 0.05..=0.95;
const LOW_PERCENT_RANGE: RangeInclusive<u64> = 1..=99;
const REFRESH_MIN_GAP_SECS_RANGE: RangeInclusive<u64> = 5..=10 * MINUTE;
const METRICS_TTL_SECS_RANGE: RangeInclusive<u64> = MINUTE..=DAY_SECS;
const RECENT_BARS_RANGE: RangeInclusive<usize> = 1..=48;
const LOGIN_WARN_DAYS_RANGE: RangeInclusive<u64> = 0..=30;
const BAR_DAYS_RANGE: RangeInclusive<u64> = 1..=90;
const CLAUDE_LOG_FILES_RANGE: RangeInclusive<usize> = 1..=1000;
const TOP_SESSIONS_RANGE: RangeInclusive<usize> = 1..=20;
const MARATHON_REQUESTS_RANGE: RangeInclusive<usize> = 10..=10_000;
const LARGE_CONTEXT_TOKENS_RANGE: RangeInclusive<u64> = 1_000..=2_000_000;

/// Claude Code 모델별 컨텍스트 창(토큰). 출처: code.claude.com/docs/en/model-config "Extended context"
/// (2026-11-11 확인): Fable·Sonnet 5 이상·Haiku 5.5·Opus 4.7 이상은 기본 1M, Opus 4.6·Sonnet 4.6은 `[1m]` 없이 200K.
/// 표에 없는 모델은 창을 모른다(%를 보이지 않는다). 날짜 접미사(`-20251001`)는 떼고 찾는다.
pub const CLAUDE_CONTEXT_WINDOWS: &[(&str, u64)] = &[
    ("claude-fable-5-2", 1_000_000),
    ("claude-fable-5-1", 1_000_000),
    ("claude-fable-5", 1_000_000),
    ("claude-opus-5-5", 1_000_000),
    ("claude-opus-5", 1_000_000),
    ("claude-opus-4-8", 1_000_000),
    ("claude-opus-4-7", 1_000_000),
    ("claude-sonnet-5-5", 1_000_000),
    ("claude-sonnet-5", 1_000_000),
    ("claude-haiku-5-5", 1_000_000),
    ("claude-opus-4-6", 200_000),
    ("claude-sonnet-4-6", 200_000),
];
/// `[1m]` 접미사 모델과 `CLAUDE_CODE_DISABLE_1M_CONTEXT=1`일 때의 창(같은 문서 "Turn off 1M context").
pub const CLAUDE_1M_WINDOW: u64 = 1_000_000;
pub const CLAUDE_DISABLED_1M_WINDOW: u64 = 200_000;
/// 자동 압축 창(`autoCompactWindow`·`CLAUDE_CODE_AUTO_COMPACT_WINDOW`)이 받는 범위. 밖이면 끝값으로 본다(같은 문서
/// "Values and warnings"). `/context`는 이 창을 분모로 쓴다(이 Mac 2026-11-11: `modelSettings.claude-opus-5-5.autoCompactWindow`
/// 400000 → `/context` "20k / 400k").
pub const CLAUDE_AUTO_COMPACT_WINDOW_RANGE: RangeInclusive<u64> = 100_000..=1_000_000;
const DAILY_GOAL_PERCENT_RANGE: RangeInclusive<u64> = 1..=100;
/// 0 = 자동(월 한도 ÷ 그 달 일수).
const KIRO_DAILY_CREDITS_RANGE: RangeInclusive<u64> = 0..=100_000;
const COMPARE_DAYS_RANGE: RangeInclusive<u64> = 1..=30;
const CHECK_MODEL_DAYS_RANGE: RangeInclusive<u64> = 1..=180;
const CHECK_ALERT_MIN_RANGE: RangeInclusive<u64> = 1..=20;
const CLAUDE_HANDOFF_TIMEOUT_SECS_RANGE: RangeInclusive<u64> = MINUTE..=30 * MINUTE;
const CLAUDE_HANDOFF_MIN_REMAINING_RANGE: RangeInclusive<u64> = 0..=90;
const DIAGNOSE_RECENT_FOLDERS_RANGE: RangeInclusive<usize> = 1..=20;
const DIAGNOSE_USAGE_DAYS_RANGE: RangeInclusive<u64> = 1..=365;
const DIAGNOSE_USAGE_FILES_RANGE: RangeInclusive<usize> = 1..=10_000;
const DIAGNOSE_USAGE_MAX_MIB_RANGE: RangeInclusive<u64> = 16..=65_536;
const DIAGNOSE_MIN_SESSIONS_RANGE: RangeInclusive<usize> = 0..=1_000;
const SCOPE_MIN_USES_RANGE: RangeInclusive<usize> = 1..=1_000;

/// 필드 하나의 범위(양 끝 포함)·기본값. 값은 저장 단위(초·바이트 등). TS 짝은 `ipc.ts`의 `TuningFieldSpec`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TuningFieldSpec {
    /// JSON 이름. 묶음은 `advanced.goodR2`처럼 점으로 잇는다.
    pub key: &'static str,
    pub min: f64,
    pub max: f64,
    pub default: f64,
    /// 정수만 받는 필드(`u64`·`usize`).
    pub is_integer: bool,
}

/// `fields()` 표의 값 형식.
trait FieldValue: Copy + PartialOrd {
    const IS_INTEGER: bool;
    fn to_f64(self) -> f64;
}

impl FieldValue for u64 {
    const IS_INTEGER: bool = true;
    fn to_f64(self) -> f64 {
        self as f64
    }
}

impl FieldValue for usize {
    const IS_INTEGER: bool = true;
    fn to_f64(self) -> f64 {
        self as f64
    }
}

impl FieldValue for f64 {
    const IS_INTEGER: bool = false;
    fn to_f64(self) -> f64 {
        self
    }
}

/// (spec, 범위 안인가). 범위 검사는 원래 형식으로 한다.
fn field<T: FieldValue>(
    key: &'static str,
    value: T,
    range: &RangeInclusive<T>,
) -> (TuningFieldSpec, bool) {
    let spec = TuningFieldSpec {
        key,
        min: range.start().to_f64(),
        max: range.end().to_f64(),
        default: value.to_f64(),
        is_integer: T::IS_INTEGER,
    };
    (spec, range.contains(&value))
}

/// 환율 판정 기준. 바꾸면 "추정" 표기의 뜻이 달라진다(0007).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct AdvancedTuning {
    /// 검증 R²가 이 값 이상이면 환율 숫자를 보여 준다(`estimated`).
    pub good_r2: f64,
    /// 이 값 이상 `good_r2` 미만이면 범위만 보여 준다(`range_only`). 그 아래는 `unreliable`.
    pub range_r2: f64,
    /// 같은 5시간 창의 `resets_at` 흔들림 허용치(초).
    pub reset_tol_secs: u64,
    /// 시간순 holdout에서 학습에 쓰는 앞쪽 창 비율.
    pub holdout_train: f64,
    /// 창 수 < 변수 수 × 이 값이면 회귀하지 않는다(`insufficient`).
    pub min_samples_per_var: usize,
    /// Claude 환율 회귀에 쓰는 최근 기간(일).
    pub claude_rate_days: u64,
    /// 3층 분해: 요청의 `cached / input`이 이 값 미만이면 캐시 미스로 센다.
    pub cache_miss_ratio: f64,
}

impl Default for AdvancedTuning {
    fn default() -> Self {
        Self {
            good_r2: 0.7,
            range_r2: 0.3,
            reset_tol_secs: 600,
            holdout_train: 0.7,
            min_samples_per_var: 3,
            claude_rate_days: 7,
            cache_miss_ratio: 0.5,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct Tuning {
    /// 한도 조회 간격(Claude·Codex).
    pub api_interval_secs: u64,
    pub kiro_interval_secs: u64,
    /// 새로고침을 연달아 눌러도 같은 에이전트를 이보다 자주 조회하지 않는다.
    pub refresh_min_gap_secs: u64,
    /// 429 뒤 간격을 두 배씩 늘릴 때의 상한.
    pub max_backoff_secs: u64,
    /// 환율·시작 컨텍스트를 이만큼 지나 호출되면 뒤에서 다시 계산한다.
    pub rates_ttl_secs: u64,
    pub context_ttl_secs: u64,
    /// 트레이 지표(최근 5시간 요청·캐시 적중률·막대)를 이만큼 지나 호출되면 뒤에서 다시 계산한다.
    pub metrics_ttl_secs: u64,
    pub http_timeout_secs: u64,
    pub keychain_timeout_secs: u64,
    pub kiro_timeout_secs: u64,
    /// 가장 최근 세션부터 이 수만큼만 훑는다.
    pub codex_sessions_to_search: usize,
    pub kiro_sessions_to_search: usize,
    pub shape_sessions_to_search: usize,
    /// Codex 한도 대체 경로에서 볼 rollout 수.
    pub codex_rollouts_checked: usize,
    pub claude_log_tail_bytes: u64,
    /// 파일 크기로 토큰을 추정할 때의 바이트 ÷ 토큰. 화면의 문자 → 토큰 환산에도 쓴다.
    pub bytes_per_token: u64,
    /// 3층 분해의 창 사용 패턴(창당 요청·캐시 미스 수)을 보는 최근 기간(일).
    pub breakdown_days: u64,
    /// 남은 %가 이 값 이하면 막대를 경고색으로 그린다(화면 전용).
    pub low_percent: u64,
    /// 트레이 막대 그래프에 보일 최근 5시간 구간 수(Claude·Codex).
    pub recent_bars: usize,
    /// 막대 그래프가 보는 최근 기간(일). Kiro는 이 수만큼 하루(24시간) 막대를 그린다.
    pub bar_days: u64,
    pub login_warn_days: u64,
    /// 최근 5시간 요청을 셀 때 여는 Claude 대화 기록(`~/.claude/projects/**/*.jsonl`) 수. 최근에 바뀐 순.
    pub claude_log_files: usize,
    /// 행동 패턴(docs/decisions/0013): 한도를 많이 쓴 Codex 세션을 이 수만큼 보여 준다.
    pub top_sessions: usize,
    /// 기간 안 요청 수가 이 값 이상인 세션을 "긴 세션"으로 표시한다.
    pub marathon_requests: usize,
    /// 요청당 평균 입력 토큰이 이 값 이상인 세션을 "큰 컨텍스트"로 표시한다.
    pub large_context_tokens: u64,
    /// 대시보드(0015): Claude·Codex 하루 몫(주간 한도 %p). 기본 14 ≈ 100 ÷ 7.
    pub daily_goal_percent: u64,
    /// 대시보드(0015): Kiro 하루 몫(크레딧). 0이면 월 한도 ÷ 그 달 일수.
    pub kiro_daily_credits: u64,
    /// 처방 효과(0016): 기준 시각 앞뒤로 비교하는 기간(일). 주간 요약도 이 기간이다.
    pub compare_days: u64,
    /// 최적화 점검(0017 첫 점검): 효과 없는 설정을 판정할 때 보는 최근 Claude 기록 기간(일).
    pub check_model_days: u64,
    /// 점검 알림(0017 "화면 자리"): 적용을 추천하는 카드가 이 수 이상이면 팝오버 "점검" 옆에 점과 개수를 붙인다.
    pub check_alert_min: u64,
    /// Claude에게 맡기기(0018 "실행 경계"): 실행이 이보다 오래 걸리면 프로세스를 죽인다.
    pub claude_handoff_timeout_secs: u64,
    /// Claude에게 맡기기: Claude 창 중 가장 적게 남은 창의 남은 %가 이 값보다 적거나 모르면 실행하지 않는다.
    pub claude_handoff_min_remaining: u64,
    /// 진단하기(0022): 플러그인·MCP 사용 횟수를 세는 최근 Claude 기록 기간(일). 이 기간에 0회면 끄기를 권한다.
    pub diagnose_usage_days: u64,
    /// Number of distinct recent session folders to inspect in Diagnose.
    pub diagnose_recent_folders: usize,
    /// 진단하기: 그 기간에 여는 Claude 기록 파일 수 상한(최근에 바뀐 순).
    pub diagnose_usage_files: usize,
    /// 진단하기: 여는 기록 크기 합계 상한(MiB). 넘으면 더 오래된 파일은 읽지 않고 "일부만 봄"으로 표시한다.
    pub diagnose_usage_max_mib: u64,
    /// 진단하기: 기간 안에 이 수 이상의 Claude 세션을 읽었을 때만 "안 씀"을 근거로 끄기를 권한다(0017: 공식 기준 10세션). 0 = 세션 수를 안 본다.
    pub diagnose_min_sessions: usize,
    /// 진단하기: 기간 안 사용 횟수가 이 수 이상일 때만 "프로젝트에서만 켜기"를 제안한다(1~1000). 적은 횟수는 자주 쓰는지 알 수 없다.
    pub scope_min_uses: usize,
    pub session_idle_hours: u64,
    pub session_child_cpu_percent: f64,
    pub session_protected_folders: Vec<String>,
    pub advanced: AdvancedTuning,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            api_interval_secs: 5 * MINUTE,
            kiro_interval_secs: 10 * MINUTE,
            refresh_min_gap_secs: 30,
            max_backoff_secs: 60 * MINUTE,
            rates_ttl_secs: 30 * MINUTE,
            context_ttl_secs: 10 * MINUTE,
            metrics_ttl_secs: 5 * MINUTE,
            http_timeout_secs: 15,
            keychain_timeout_secs: 10,
            kiro_timeout_secs: 30,
            codex_sessions_to_search: 50,
            kiro_sessions_to_search: 50,
            shape_sessions_to_search: 5,
            codex_rollouts_checked: 3,
            claude_log_tail_bytes: 64 * 1024,
            bytes_per_token: 4,
            breakdown_days: 14,
            low_percent: 20,
            recent_bars: 8,
            bar_days: 7,
            login_warn_days: 3,
            claude_log_files: 50,
            top_sessions: 3,
            marathon_requests: 150,
            large_context_tokens: 130_000,
            daily_goal_percent: 14,
            kiro_daily_credits: 0,
            compare_days: 7,
            check_model_days: 30,
            check_alert_min: 1,
            claude_handoff_timeout_secs: 5 * MINUTE,
            claude_handoff_min_remaining: 20,
            diagnose_usage_days: 30,
            diagnose_recent_folders: 5,
            diagnose_usage_files: 1000,
            diagnose_usage_max_mib: 2048,
            diagnose_min_sessions: 10,
            scope_min_uses: 3,
            session_idle_hours: 24,
            session_child_cpu_percent: 2.0,
            session_protected_folders: Vec::new(),
            advanced: AdvancedTuning::default(),
        }
    }
}

impl Tuning {
    pub fn api_interval(&self) -> Duration {
        Duration::from_secs(self.api_interval_secs)
    }

    pub fn kiro_interval(&self) -> Duration {
        Duration::from_secs(self.kiro_interval_secs)
    }

    pub fn refresh_min_gap(&self) -> Duration {
        Duration::from_secs(self.refresh_min_gap_secs)
    }

    pub fn max_backoff(&self) -> Duration {
        Duration::from_secs(self.max_backoff_secs)
    }

    pub fn rates_ttl(&self) -> Duration {
        Duration::from_secs(self.rates_ttl_secs)
    }

    pub fn context_ttl(&self) -> Duration {
        Duration::from_secs(self.context_ttl_secs)
    }

    pub fn metrics_ttl(&self) -> Duration {
        Duration::from_secs(self.metrics_ttl_secs)
    }

    pub fn http_timeout(&self) -> Duration {
        Duration::from_secs(self.http_timeout_secs)
    }

    pub fn keychain_timeout(&self) -> Duration {
        Duration::from_secs(self.keychain_timeout_secs)
    }

    pub fn kiro_timeout(&self) -> Duration {
        Duration::from_secs(self.kiro_timeout_secs)
    }

    pub fn claude_handoff_timeout(&self) -> Duration {
        Duration::from_secs(self.claude_handoff_timeout_secs)
    }

    /// 필드마다 (spec, 범위 안인가). `spec.default`에는 `self`의 값이 들어간다(`spec()`은 기본값으로 부른다).
    /// 검증과 화면용 spec이 이 표 하나를 쓴다. `good_r2 < range_r2`이면 두 필드 모두 범위 밖이다.
    fn fields(&self) -> [(TuningFieldSpec, bool); 47] {
        let a = &self.advanced;
        let r2_order = a.good_r2 >= a.range_r2;
        let (good_r2, good_ok) = field("advanced.goodR2", a.good_r2, &R2_RANGE);
        let (range_r2, range_ok) = field("advanced.rangeR2", a.range_r2, &R2_RANGE);
        [
            field(
                "sessionIdleHours",
                self.session_idle_hours,
                &SESSION_IDLE_HOURS_RANGE,
            ),
            field(
                "sessionChildCpuPercent",
                self.session_child_cpu_percent,
                &SESSION_CHILD_CPU_PERCENT_RANGE,
            ),
            field(
                "apiIntervalSecs",
                self.api_interval_secs,
                &API_INTERVAL_SECS_RANGE,
            ),
            field(
                "kiroIntervalSecs",
                self.kiro_interval_secs,
                &KIRO_INTERVAL_SECS_RANGE,
            ),
            field(
                "refreshMinGapSecs",
                self.refresh_min_gap_secs,
                &REFRESH_MIN_GAP_SECS_RANGE,
            ),
            field(
                "maxBackoffSecs",
                self.max_backoff_secs,
                &MAX_BACKOFF_SECS_RANGE,
            ),
            field("ratesTtlSecs", self.rates_ttl_secs, &RATES_TTL_SECS_RANGE),
            field(
                "contextTtlSecs",
                self.context_ttl_secs,
                &CONTEXT_TTL_SECS_RANGE,
            ),
            field(
                "metricsTtlSecs",
                self.metrics_ttl_secs,
                &METRICS_TTL_SECS_RANGE,
            ),
            field(
                "httpTimeoutSecs",
                self.http_timeout_secs,
                &HTTP_TIMEOUT_SECS_RANGE,
            ),
            field(
                "keychainTimeoutSecs",
                self.keychain_timeout_secs,
                &KEYCHAIN_TIMEOUT_SECS_RANGE,
            ),
            field(
                "kiroTimeoutSecs",
                self.kiro_timeout_secs,
                &KIRO_TIMEOUT_SECS_RANGE,
            ),
            field(
                "codexSessionsToSearch",
                self.codex_sessions_to_search,
                &SESSIONS_RANGE,
            ),
            field(
                "kiroSessionsToSearch",
                self.kiro_sessions_to_search,
                &SESSIONS_RANGE,
            ),
            field(
                "shapeSessionsToSearch",
                self.shape_sessions_to_search,
                &SESSIONS_RANGE,
            ),
            field(
                "codexRolloutsChecked",
                self.codex_rollouts_checked,
                &SESSIONS_RANGE,
            ),
            field(
                "claudeLogTailBytes",
                self.claude_log_tail_bytes,
                &CLAUDE_LOG_TAIL_BYTES_RANGE,
            ),
            field(
                "bytesPerToken",
                self.bytes_per_token,
                &BYTES_PER_TOKEN_RANGE,
            ),
            field("breakdownDays", self.breakdown_days, &BREAKDOWN_DAYS_RANGE),
            field("lowPercent", self.low_percent, &LOW_PERCENT_RANGE),
            field("recentBars", self.recent_bars, &RECENT_BARS_RANGE),
            field("barDays", self.bar_days, &BAR_DAYS_RANGE),
            field(
                "loginWarnDays",
                self.login_warn_days,
                &LOGIN_WARN_DAYS_RANGE,
            ),
            field(
                "claudeLogFiles",
                self.claude_log_files,
                &CLAUDE_LOG_FILES_RANGE,
            ),
            field("topSessions", self.top_sessions, &TOP_SESSIONS_RANGE),
            field(
                "marathonRequests",
                self.marathon_requests,
                &MARATHON_REQUESTS_RANGE,
            ),
            field(
                "largeContextTokens",
                self.large_context_tokens,
                &LARGE_CONTEXT_TOKENS_RANGE,
            ),
            field(
                "dailyGoalPercent",
                self.daily_goal_percent,
                &DAILY_GOAL_PERCENT_RANGE,
            ),
            field(
                "kiroDailyCredits",
                self.kiro_daily_credits,
                &KIRO_DAILY_CREDITS_RANGE,
            ),
            field("compareDays", self.compare_days, &COMPARE_DAYS_RANGE),
            field(
                "checkModelDays",
                self.check_model_days,
                &CHECK_MODEL_DAYS_RANGE,
            ),
            field(
                "checkAlertMin",
                self.check_alert_min,
                &CHECK_ALERT_MIN_RANGE,
            ),
            field(
                "claudeHandoffTimeoutSecs",
                self.claude_handoff_timeout_secs,
                &CLAUDE_HANDOFF_TIMEOUT_SECS_RANGE,
            ),
            field(
                "claudeHandoffMinRemaining",
                self.claude_handoff_min_remaining,
                &CLAUDE_HANDOFF_MIN_REMAINING_RANGE,
            ),
            field(
                "diagnoseUsageDays",
                self.diagnose_usage_days,
                &DIAGNOSE_USAGE_DAYS_RANGE,
            ),
            field(
                "diagnoseUsageFiles",
                self.diagnose_usage_files,
                &DIAGNOSE_USAGE_FILES_RANGE,
            ),
            field(
                "diagnoseUsageMaxMib",
                self.diagnose_usage_max_mib,
                &DIAGNOSE_USAGE_MAX_MIB_RANGE,
            ),
            field(
                "diagnoseMinSessions",
                self.diagnose_min_sessions,
                &DIAGNOSE_MIN_SESSIONS_RANGE,
            ),
            field(
                "diagnoseRecentFolders",
                self.diagnose_recent_folders,
                &DIAGNOSE_RECENT_FOLDERS_RANGE,
            ),
            field("scopeMinUses", self.scope_min_uses, &SCOPE_MIN_USES_RANGE),
            (good_r2, good_ok && r2_order),
            (range_r2, range_ok && r2_order),
            field(
                "advanced.resetTolSecs",
                a.reset_tol_secs,
                &RESET_TOL_SECS_RANGE,
            ),
            field(
                "advanced.holdoutTrain",
                a.holdout_train,
                &HOLDOUT_TRAIN_RANGE,
            ),
            field(
                "advanced.minSamplesPerVar",
                a.min_samples_per_var,
                &MIN_SAMPLES_PER_VAR_RANGE,
            ),
            field(
                "advanced.claudeRateDays",
                a.claude_rate_days,
                &CLAUDE_RATE_DAYS_RANGE,
            ),
            field(
                "advanced.cacheMissRatio",
                a.cache_miss_ratio,
                &CACHE_MISS_RATIO_RANGE,
            ),
        ]
    }

    /// 범위를 벗어난 필드 이름(JSON 이름, 묶음은 `advanced.goodR2`처럼 점으로 잇는다).
    /// `good_r2 < range_r2`이면 두 필드를 모두 돌려준다.
    pub fn invalid_fields(&self) -> Vec<&'static str> {
        let mut invalid: Vec<_> = self
            .fields()
            .into_iter()
            .filter_map(|(spec, is_valid)| (!is_valid).then_some(spec.key))
            .collect();
        if self
            .session_protected_folders
            .iter()
            .any(|p| !std::path::Path::new(p).is_absolute())
        {
            invalid.push("sessionProtectedFolders");
        }
        invalid
    }

    /// 화면이 입력을 검사하고 범위·기본값을 보여 줄 표. 값은 저장 단위다. `Settings.tuningSpec`으로 내려간다.
    pub fn spec() -> Vec<TuningFieldSpec> {
        Self::default()
            .fields()
            .into_iter()
            .map(|(spec, _)| spec)
            .collect()
    }

    /// `set_settings`용: 하나라도 범위 밖이면 필드 이름을 담은 오류.
    pub fn validate(&self) -> Result<(), String> {
        match self.invalid_fields().as_slice() {
            [] => Ok(()),
            fields => Err(crate::tr!(
                "tuning 값이 범위를 벗어났어요: {}",
                "Some values are out of range: {}",
                fields.join(", ")
            )),
        }
    }

    /// 파일의 `tuning` 값을 읽는다. 모르는 필드·형식이 틀린 필드·범위 밖 필드는 그 필드만
    /// 기본값으로 두고 stderr에 이름과 이유를 남긴다.
    pub fn from_json(file: &Value) -> Self {
        let Value::Object(top) = file else {
            eprintln!("kcal: 설정 tuning이 객체가 아니어서 기본값을 씁니다");
            return Self::default();
        };
        let mut current = serde_json::to_value(Self::default()).expect("Tuning은 JSON이 된다");
        for (pointer, value) in leaves(top) {
            let name = pointer.trim_start_matches('/').replace('/', ".");
            let mut candidate = current.clone();
            match candidate.pointer_mut(&pointer) {
                Some(slot) if !slot.is_object() => *slot = value,
                _ => {
                    eprintln!("kcal: 설정 tuning.{name}은(는) 알 수 없는 필드라 무시합니다");
                    continue;
                }
            }
            match serde_json::from_value::<Self>(candidate.clone()) {
                Err(_) => eprintln!("kcal: 설정 tuning.{name}의 형식이 틀려 기본값을 씁니다"),
                Ok(next) if next.invalid_fields().contains(&name.as_str()) => {
                    eprintln!("kcal: 설정 tuning.{name}이(가) 범위를 벗어나 기본값을 씁니다");
                }
                Ok(_) => current = candidate,
            }
        }
        serde_json::from_value(current).unwrap_or_default()
    }
}

/// (JSON pointer, 값) 목록. `advanced`만 한 단계 더 내려간다.
fn leaves(top: &serde_json::Map<String, Value>) -> Vec<(String, Value)> {
    top.iter()
        .flat_map(|(key, value)| match (key.as_str(), value) {
            ("advanced", Value::Object(group)) => group
                .iter()
                .map(|(k, v)| (format!("/advanced/{k}"), v.clone()))
                .collect(),
            _ => vec![(format!("/{key}"), value.clone())],
        })
        .collect()
}

#[cfg(test)]
#[path = "tuning_tests.rs"]
mod tests;
