// 프론트엔드가 Rust command·event를 쓰는 유일한 경로. 컴포넌트에서 invoke·listen을 직접 쓰지 않는다.
// 코치 스위치는 `ipc-coach.ts`, 점검·Claude에게 맡기기 부분은 `ipc-checks.ts`에 두고 여기서 다시 내보낸다.
// 타입은 src-tauri/src/usage/mod.rs, context/mod.rs, rate/mod.rs, metrics/mod.rs, health/mod.rs, settings.rs, tray.rs, coach/, lib.rs, error.rs와 짝을 이룬다. 계약을 바꾸면 양쪽과 테스트를 같이 고친다.
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

import type { Lang, LanguageChoice } from "@/lib/i18n";

/** Rust `usage::Agent`. */
export type Agent = "claude" | "codex" | "kiro";

/** Rust `Agent::ALL` 순서. */
export const AGENTS: readonly Agent[] = ["claude", "codex", "kiro"];

/** Rust `usage::UsageSource`. 모두 실측 값이다. */
export type UsageSource = "api" | "log" | "cli";

/** Rust `usage::ReasonKind`. `setup`은 사용자가 할 일(설치·로그인·허용), `failure`는 진짜 실패. */
export type ReasonKind = "setup" | "failure";

/** Rust `usage::WindowId`. */
export type WindowId = "5h" | "7d" | "month";

/** Rust `usage::LimitWindow`. 시각은 RFC 3339 문자열. */
export type LimitWindow = {
  id: WindowId;
  usedPercent: number;
  resetsAt: string | null;
  resetPassed: boolean;
  used?: number;
  limit?: number;
  /** 모델 전용 한도의 모델 이름(`modelWindows`에만 있다). */
  group?: string;
};

/** Rust `usage::Credits`. Claude 추가 사용량(통화 단위 금액). */
export type Credits = { used: number; limit: number; currency: string };

/** Rust `usage::AgentUsage`. */
export type AgentUsage = {
  agent: Agent;
  source: UsageSource | null;
  observedAt: string | null;
  windows: LimitWindow[];
  /** 모델 전용 한도(0027 ③). 트레이 막대·알림은 읽지 않는다(`windows`만). 없으면 키가 없다. */
  modelWindows?: LimitWindow[];
  credits?: Credits;
  /** Codex 한도 초기화권 남은 개수. */
  resetPasses?: number;
  loginExpiresAt?: string;
  reason: string | null;
  /** `reason`이 있을 때만 값이 있다. */
  reasonKind: ReasonKind | null;
  /** `null`이면 아직 첫 조회 중. */
  checkedAt: string | null;
};

/** Rust `usage::UsageSnapshot`. */
export type UsageSnapshot = {
  agents: AgentUsage[];
};

/** Rust `error::AppError`. */
export type AppError = {
  kind: "internal" | "invalid_input";
  message: string;
};

export function isAppError(value: unknown): value is AppError {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as AppError).kind === "string" &&
    typeof (value as AppError).message === "string"
  );
}

/** AppError, Tauri의 문자열 오류, 그 밖의 예외를 표시용 문자열로 바꾼다. */
export function errorMessage(error: unknown): string {
  if (isAppError(error)) return error.message;
  if (error instanceof Error) return error.message;
  return String(error);
}

/** Rust `usage::USAGE_UPDATED_EVENT`. */
export const USAGE_UPDATED_EVENT = "usage-updated";

export function getUsage(): Promise<UsageSnapshot> {
  return invoke<UsageSnapshot>("get_usage");
}

export function onUsageUpdated(handler: (snapshot: UsageSnapshot) => void): Promise<UnlistenFn> {
  return listen<UsageSnapshot>(USAGE_UPDATED_EVENT, (event) => handler(event.payload));
}

/** Rust `context::ContextKind`. 화면은 항목별로 보인다(`context-list.tsx`). */
export type ContextKind =
  | "instructions"
  | "rules"
  | "skills"
  | "agents"
  | "hooks"
  | "mcp"
  | "mcpInstructions"
  | "plugins"
  | "system"
  | "tools";

/** Rust `context::ContextSource`. measured = 실제로 보낸 요청(Claude api-request-shape), estimated = 바이트 ÷ `Tuning.bytesPerToken`(기본 4). */
export type ContextSource = "measured" | "estimated";

/** Rust `context::Loading`. 합계에는 startup만 들어간다. */
export type Loading = "startup" | "conditional" | "deferred";

/** Rust `context::ContextItem`. `bytes`·`tokens`가 null이면 정적으로 알 수 없다(hook 출력, MCP 스키마). */
export type ContextItem = {
  kind: ContextKind;
  name: string;
  path?: string;
  bytes: number | null;
  tokens: number | null;
  source: ContextSource;
  loading: Loading;
};

/** Rust `context::AgentContext`. `items`는 startup → conditional → deferred, 같은 시점 안에서 토큰 큰 순. */
export type AgentContext = {
  agent: Agent;
  items: ContextItem[];
  totalStartupTokens: number;
  /** 첫 요청의 실제 토크나이저 입력 토큰(Codex). 첫 사용자 메시지가 들어 있어 시작 합계와 다른 양이다. 모르면 null. */
  firstRequestTokens: number | null;
  /** Kiro 세션 시작에 조립된 문자 수(실측, 토큰 아님). 모르면 null. */
  firstRequestChars: number | null;
  /** 실측값을 잰 세션의 모델(Claude `api-request.params.model`, Codex `turn_context.model`). 모르면 null. */
  model: string | null;
  /** 그 모델의 컨텍스트 창(토큰). Claude는 `/context`와 같은 분모(자동 압축 창이 있으면 그 값). 모르면 null이고 화면은 %를 숨긴다. */
  contextWindow: number | null;
  warnings: string[];
};

/** Rust `context::ContextSnapshot`. */
export type ContextSnapshot = {
  /** 잰 시각(RFC 3339). */
  computedAt: string;
  agents: AgentContext[];
};

/** Rust `context::CONTEXT_UPDATED_EVENT`. 뒤에서 다시 잰 전역 측정이 끝나면 온다. */
export const CONTEXT_UPDATED_EVENT = "context-updated";

/** 세션 시작 컨텍스트. `workspace`가 null이면 전역 설정만이고 Rust가 마지막 결과를 바로 준다. 경로는 Rust가 검증한다(절대 경로, 존재하는 폴더). */
export function getContext(workspace: string | null = null): Promise<ContextSnapshot> {
  return invoke<ContextSnapshot>("get_context", { workspace });
}

export function onContextUpdated(
  handler: (snapshot: ContextSnapshot) => void,
): Promise<UnlistenFn> {
  return listen<ContextSnapshot>(CONTEXT_UPDATED_EVENT, (event) => handler(event.payload));
}

/** Rust `rate::RateStatus`. `estimated`만 환율 숫자가 있고, `range_only`는 LOO fold 범위만 있다. */
export type RateStatus = "estimated" | "range_only" | "unreliable" | "insufficient" | "no_data";

/** Rust `rate::TokenRate`. 5시간 한도 1%p당 토큰. null이면 그 토큰의 계수가 0. */
export type TokenRate = {
  cachedInput: number | null;
  uncachedInput: number | null;
  output: number | null;
};

/** Rust `rate::Bounds`. `max`가 null이면 상한 없음(어떤 fold에서 계수 0). */
export type Bounds = { min: number | null; max: number | null };

/** Rust `rate::TokenRange`. LOO fold별 1%p당 토큰 범위. */
export type TokenRange = {
  cachedInput: Bounds;
  uncachedInput: Bounds;
  output: Bounds;
};

/** Rust `rate::Spread`. 창 값들의 중앙값·p90(선형 보간). */
export type Spread = { median: number; p90: number };

/** Rust `rate::WindowPattern`. 최근 `days`일 5시간 창의 사용 패턴(실측). */
export type WindowPattern = {
  /** 기준 기간(일), settings `tuning.breakdownDays`. */
  days: number;
  windows: number;
  /** 창당 요청 수(반복 보고 제외, 입력 > 0). */
  requests: Spread;
  /** 창당 캐시 미스 수(`cached / input < tuning.advanced.cacheMissRatio`). */
  misses: Spread;
};

/** Rust `rate::WindowComposition`. 보통 5시간 구간(요청 수 중앙값 구간 하나)이 무엇으로 찼나. 몫은 %(예상). */
export type WindowComposition = {
  /** 기간 안의 5시간 구간 수. */
  windows: number;
  /** 고른 구간의 요청 수·캐시 미스 수(실측). 검증 줄에 보인다. */
  requests: number;
  misses: number;
  /** 캐시 입력 + 비캐시 입력의 몫. */
  inputShare: number;
  /** 출력의 몫. */
  outputShare: number;
  /** 고른 구간의 실제 사용 %(실측). */
  usedPercent: number;
};

/** Rust `rate::CodexSession`. 기간 안 Codex 세션(rollout 하나). */
export type CodexSession = {
  /** 작업 폴더 이름. 모르면 rollout 파일 이름의 시작 시각 부분. */
  name: string;
  /** `session_meta`의 `cwd` 전체 경로. 같은 `name`을 구분할 때 쓴다. 모르면 null. */
  cwd: string | null;
  /** 기간 안 첫 요청 시각(RFC 3339). */
  startedAt: string;
  requests: number;
  /** 요청당 평균 입력 토큰(캐시 포함). */
  avgInputTokens: number;
  misses: number;
  /** 요청마다의 몫(%)을 더한 값(예상). 여러 5시간 구간에 걸치면 100을 넘는다. */
  share: number;
  /** 요청 수 ≥ `tuning.marathonRequests`. */
  isMarathon: boolean;
  /** 평균 입력 ≥ `tuning.largeContextTokens`. */
  isLargeContext: boolean;
};

/** Rust `rate::CodexRate`. 추정 값(회귀). */
export type CodexRate = {
  status: RateStatus;
  /** 검증 R² = min(LOO, 시간순 holdout). */
  r2: number | null;
  windows: number;
  excludedWindows: number;
  perPercent: TokenRate | null;
  foldRange: TokenRange | null;
  /** 기간 안에 창이 없으면 null. 환율 판정과 따로 계산한다. */
  pattern: WindowPattern | null;
  /** 환율이 `estimated`가 아니거나 기간 안에 구간이 없으면 null. */
  composition: WindowComposition | null;
  /** 한도를 많이 쓴 세션 상위 `tuning.topSessions`개. 환율이 `estimated`가 아니면 빈 목록. */
  sessions: CodexSession[];
};

/** Rust `rate::KiroModelCredits`. 실측 값. */
export type KiroModelCredits = {
  model: string;
  turns: number;
  creditsPerTurn: number;
};

/** Rust `rate::ClaudeRate`(= `CodexRate`). `composition`은 null, `sessions`는 빈 목록이다. */
export type ClaudeRate = CodexRate;

/** Rust `rate::RateSnapshot`. */
export type RateSnapshot = {
  /** 계산한 시각(RFC 3339). */
  computedAt: string;
  codex: CodexRate;
  /** 턴이 많은 순. 비어 있으면 기록 없음. */
  kiro: KiroModelCredits[];
  claude: ClaudeRate;
};

/** Rust `rate::RATES_UPDATED_EVENT`. 뒤에서 다시 계산한 환율이 오면 온다. */
export const RATES_UPDATED_EVENT = "rates-updated";

/** 에이전트별 환율. Rust가 마지막 결과를 바로 주고, 오래됐으면 뒤에서 다시 계산한다. */
export function getRates(): Promise<RateSnapshot> {
  return invoke<RateSnapshot>("get_rates");
}

export function onRatesUpdated(handler: (snapshot: RateSnapshot) => void): Promise<UnlistenFn> {
  return listen<RateSnapshot>(RATES_UPDATED_EVENT, (event) => handler(event.payload));
}

/** Rust `metrics::BarUnit`. percent = 그 5시간 구간의 가장 큰 사용 %, credits = 그 하루(24시간) 크레딧 합. */
export type BarUnit = "percent" | "credits";

/** Rust `metrics::UsageBar`. */
export type UsageBar = {
  /** 구간 시작(RFC 3339). 5시간 구간은 `resetsAt − 5h`. */
  startsAt: string;
  value: number;
  /** 지금 진행 중인 구간. */
  isCurrent: boolean;
};

/** Rust `metrics::HourBucket`. 한 시간(UTC 정시부터)의 활동. 지역 날짜로 묶는 것은 프론트(`src/lib/daily.ts`)가 한다. */
export type HourBucket = {
  /** 시간 시작(RFC 3339, UTC 정시). */
  startsAt: string;
  /** 요청 수(Kiro는 질문 수). */
  requests: number;
  /** 쓴 양. `barUnit`이 percent면 주간(7d) 한도 %p 증가분, credits면 크레딧. */
  used: number;
};

/** Rust `metrics::AgentMetrics`. 값을 모르면 null(화면은 그 줄을 숨긴다). */
export type AgentMetrics = {
  agent: Agent;
  /** 지금 5시간 한도 구간의 요청 수. Kiro는 최근 5시간 질문 수. */
  recentRequests: number | null;
  /** 같은 구간의 캐시 읽기 ÷ 전체 입력(0~1, 토큰 기준). */
  cacheHitRatio: number | null;
  barUnit: BarUnit;
  /** 오래된 순. */
  bars: UsageBar[];
  /** 최근 `barDays + 1`일의 시간 버킷. 오래된 순, 빈 시간도 0으로 들어 있다. */
  hourly: HourBucket[];
};

/** Rust `metrics::MetricsSnapshot`. */
export type MetricsSnapshot = {
  /** 계산한 시각(RFC 3339). */
  computedAt: string;
  agents: AgentMetrics[];
};

/** Rust `metrics::METRICS_UPDATED_EVENT`. 뒤에서 다시 계산한 지표가 오면 온다. */
export const METRICS_UPDATED_EVENT = "metrics-updated";

/** 트레이 지표(최근 5시간 요청·캐시 적중률·구간 막대). Rust가 마지막 결과를 바로 주고, 오래됐으면 뒤에서 다시 계산한다. */
export function getMetrics(): Promise<MetricsSnapshot> {
  return invoke<MetricsSnapshot>("get_metrics");
}

export function onMetricsUpdated(
  handler: (snapshot: MetricsSnapshot) => void,
): Promise<UnlistenFn> {
  return listen<MetricsSnapshot>(METRICS_UPDATED_EVENT, (event) => handler(event.payload));
}

/** Rust `health::ModelInputShare`. null model = other; unknown = missing model. */
export type ModelInputShare = { model: string | null; share: number };
/** Rust `health::PeriodRate`. Same judgement as the existing Codex rate. */
export type PeriodRate = {
  status: RateStatus;
  windows: number;
  perPercent: TokenRate | null;
  foldRange: TokenRange | null;
};

/** Rust `health::PeriodStats`. 구간 하나의 지표(실측). 값을 모르면 null(0으로 채우지 않는다). */
export type PeriodStats = {
  /** 시작(포함, RFC 3339). */
  from: string;
  /** 끝(포함하지 않음). 후 구간은 지금·되돌린 시각에서 잘린다. */
  to: string;
  /** 구간 길이(일, 소수). 하루 평균의 분모. */
  elapsedDays: number;
  /** 기록이 덮은 비율(0~1). 1 미만이면 "기록 일부만". 기록이 없으면 null. */
  coverage: number | null;
  /** 요청 수(Kiro는 질문 수). */
  requests: number | null;
  /** 요청당 평균 입력 토큰(캐시 포함, Claude·Codex). */
  avgInputTokens: number | null;
  /** 질문당 평균 크레딧(Kiro). */
  avgCredits: number | null;
  /** 하루 평균 쓴 양. `usedUnit`이 percent면 주간 한도 %p, credits면 크레딧. */
  dailyUsed: number | null;
  usedUnit: BarUnit;
  /** 압축 횟수(Codex만). */
  compactions: number | null;
  /** Input-token shares (0–100), weekly summary only. */
  automatedInputShare: number | null;
  modelInputShares: ModelInputShare[] | null;
  codexRate: PeriodRate | null;
};

/** Rust `health::AgentComparison`. 주간 요약 한 줄. */
export type AgentComparison = {
  agent: Agent;
  before: PeriodStats;
  after: PeriodStats;
};

/** Rust `health::PrescriptionEffect`. 적용 기록 하나의 전후(그 기록의 에이전트만). */
export type PrescriptionEffect = {
  /** 적용 기록 ID(`AppliedSwitch.id`). */
  id: string;
  at: string;
  agent: Agent;
  name: string;
  /** `AppliedSwitch.target`. 점검 기록(`CHECK_TARGET_PREFIX`)이면 이름만 보인다. */
  target: string;
  /** 적용 뒤 상태. */
  enabled: boolean;
  /** 되돌렸으면 그 시각. 후 구간은 여기서 잘린다. */
  undoneAt: string | null;
  before: PeriodStats;
  after: PeriodStats;
};

/** Rust `health::HealthReport`(docs/decisions/0016). */
export type HealthReport = {
  computedAt: string;
  /** 비교 기간(일), `tuning.compareDays`. */
  compareDays: number;
  /** 켠 에이전트마다 최근 N일 vs 그 전 N일. */
  weekly: AgentComparison[];
  /** 켠 에이전트의 적용 기록, 최근순. */
  prescriptions: PrescriptionEffect[];
};

/** Rust `health::HEALTH_UPDATED_EVENT`. 뒤에서 다시 계산한 처방 효과가 오면 온다. */
export const HEALTH_UPDATED_EVENT = "health-updated";

/** 처방 전후 비교와 주간 요약. Rust가 마지막 결과를 바로 주고, 오래됐거나 적용 기록이 바뀌었으면 뒤에서 다시 계산한다. */
export function getHealth(): Promise<HealthReport> {
  return invoke<HealthReport>("get_health");
}

export function onHealthUpdated(handler: (report: HealthReport) => void): Promise<UnlistenFn> {
  return listen<HealthReport>(HEALTH_UPDATED_EVENT, (event) => handler(event.payload));
}

/** 사용량 조회와 1%당 토큰·세션 시작 토큰·지표·처방 효과 재계산을 바로 시작한다. 결과는 각 `*-updated` event로 온다. */
export function refreshAll(): Promise<void> {
  return invoke("refresh_all");
}

/** Rust `tray::DASHBOARD`. */
export const DASHBOARD_WINDOW = "dashboard";

/** Rust `tray::DashboardTab`. 대시보드 창 머리 아래 탭. */
export type DashboardTab = "usage" | "checks";

/** Rust `tray::DASHBOARD_TAB_EVENT`. 이미 열린 대시보드 창에만 온다. */
export const DASHBOARD_TAB_EVENT = "dashboard-tab";

/** Rust `tray::DashboardTarget`(event payload). null인 칸은 그대로 둔다. */
export type DashboardTarget = { tab: DashboardTab | null; agent: Agent | null };

/** 대시보드 창을 열고(없으면 만든다) 팝오버를 숨긴다. `tab`·`agent`를 주면 그 탭·에이전트로 연다(열린 창이면 바꾼다). */
export function openDashboard(tab?: DashboardTab, agent?: Agent): Promise<void> {
  return invoke("open_dashboard", { ...(tab && { tab }), ...(agent && { agent }) });
}

/** 대시보드 창: `openDashboard`가 이미 열린 창에 보낸 탭·에이전트 전환. */
export function onDashboardTab(handler: (target: DashboardTarget) => void): Promise<UnlistenFn> {
  return listen<DashboardTarget>(DASHBOARD_TAB_EVENT, (event) => handler(event.payload));
}

/** 새 대시보드 창의 시작 탭(Rust가 `?tab=`으로 넘긴다). 모르는 값이면 null. */
export function initialDashboardTab(search: string): DashboardTab | null {
  const tab = new URLSearchParams(search).get("tab");
  return tab === "usage" || tab === "checks" ? tab : null;
}

/** 새 대시보드 창의 시작 에이전트(`?agent=`). 모르는 값이면 null. 켜 있는지는 화면이 본다. */
export function initialDashboardAgent(search: string): Agent | null {
  const agent = new URLSearchParams(search).get("agent");
  return AGENTS.find((a) => a === agent) ?? null;
}

/** 지금 웹뷰 창의 label(`popover` 또는 `dashboard`). IPC 없이 Tauri가 넣어 둔 값을 읽는다. */
export function currentWindowLabel(): string {
  return getCurrentWindow().label;
}

/** Rust `tuning::AdvancedTuning`. 환율 판정 기준이라 바꾸면 "추정" 표기의 뜻이 달라진다. */
export type AdvancedTuning = {
  /** 검증 R²가 이 값 이상이면 환율 숫자를 보여 준다. */
  goodR2: number;
  /** 이 값 이상 `goodR2` 미만이면 범위만. `goodR2` 이하여야 한다. */
  rangeR2: number;
  resetTolSecs: number;
  holdoutTrain: number;
  minSamplesPerVar: number;
  claudeRateDays: number;
  /** 3층 분해: `cached / input`이 이 값 미만인 요청을 캐시 미스로 센다. */
  cacheMissRatio: number;
};

/** Rust `tuning::Tuning`. 범위는 Rust가 검증한다(docs/decisions/0007). */
export type Tuning = {
  apiIntervalSecs: number;
  kiroIntervalSecs: number;
  /** 새로고침을 연달아 눌러도 같은 에이전트를 이보다 자주 조회하지 않는다. */
  refreshMinGapSecs: number;
  maxBackoffSecs: number;
  ratesTtlSecs: number;
  contextTtlSecs: number;
  /** 트레이 지표를 이만큼 지나 호출되면 뒤에서 다시 계산한다. */
  metricsTtlSecs: number;
  httpTimeoutSecs: number;
  keychainTimeoutSecs: number;
  kiroTimeoutSecs: number;
  codexSessionsToSearch: number;
  kiroSessionsToSearch: number;
  shapeSessionsToSearch: number;
  codexRolloutsChecked: number;
  claudeLogTailBytes: number;
  /** 파일 바이트 ÷ 이 값 = 토큰 추정. 화면의 문자 → 토큰 환산에도 쓴다. */
  bytesPerToken: number;
  /** 3층 분해의 창 사용 패턴을 보는 최근 기간(일). */
  breakdownDays: number;
  /** 남은 %가 이 값 이하면 막대를 경고색으로 그린다. */
  lowPercent: number;
  /** 트레이 막대 그래프의 최근 5시간 구간 수(Claude·Codex). */
  recentBars: number;
  /** 막대 그래프가 보는 최근 기간(일). Kiro는 이 수만큼 하루 막대를 그린다. */
  barDays: number;
  loginWarnDays: number;
  /** 최근 5시간 요청을 셀 때 여는 Claude 대화 기록 파일 수(최근에 바뀐 순). */
  claudeLogFiles: number;
  /** 한도를 많이 쓴 Codex 세션을 이 수만큼 보여 준다. */
  topSessions: number;
  /** 요청 수가 이 값 이상인 세션을 "긴 세션"으로 표시한다. */
  marathonRequests: number;
  /** 요청당 평균 입력 토큰이 이 값 이상인 세션을 "큰 컨텍스트"로 표시한다. */
  largeContextTokens: number;
  /** 대시보드 하루 몫(Claude·Codex, 주간 한도 %p). */
  dailyGoalPercent: number;
  /** 대시보드 Kiro 하루 몫(크레딧). 0이면 월 한도 ÷ 그 달 일수. */
  kiroDailyCredits: number;
  /** 처방 효과: 기준 시각 앞뒤로 비교하는 기간(일). 주간 요약도 이 기간. */
  compareDays: number;
  /** 최적화 점검이 효과 없는 설정을 판정할 때 보는 최근 Claude 기록 기간(일). */
  checkModelDays: number;
  /** 적용을 추천하는 점검 카드가 이 수 이상이면 팝오버 "점검" 옆에 점과 개수를 붙인다. */
  checkAlertMin: number;
  /** Claude에게 맡기기: 실행이 이보다 오래 걸리면 끝낸다(초). */
  claudeHandoffTimeoutSecs: number;
  /** Claude에게 맡기기: 가장 적게 남은 Claude 창의 남은 %가 이보다 적거나 모르면 실행하지 않는다. */
  claudeHandoffMinRemaining: number;
  /** 진단하기: 플러그인·MCP 사용 횟수를 세는 최근 Claude 기록 기간(일). 0회면 끄기를 권한다. */
  diagnoseRecentFolders: number;
  diagnoseUsageDays: number;
  /** 진단하기: 그 기간에 여는 Claude 기록 파일 수 상한. */
  diagnoseUsageFiles: number;
  /** 진단하기: 읽는 기록 크기 합계 상한(MiB). */
  diagnoseUsageMaxMib: number;
  /** 진단하기: 기간 안 Claude 세션이 이 수 이상일 때만 0회를 근거로 끄기를 권한다. */
  diagnoseMinSessions: number;
  /** 진단하기: 기간 안 사용이 이 횟수 이상일 때만 "프로젝트에서만 켜기"를 제안한다. */
  scopeMinUses: number;
  sessionIdleHours: number;
  sessionChildCpuPercent: number;
  sessionProtectedFolders: string[];
  advanced: AdvancedTuning;
};

/** tuning 필드의 JSON 이름. 묶음은 `advanced.goodR2`처럼 점으로 잇는다(Rust `TuningFieldSpec.key`). */
export type TuningKey =
  | Exclude<keyof Tuning, "advanced" | "sessionProtectedFolders">
  | `advanced.${keyof AdvancedTuning}`;

/** Rust `tuning::TuningFieldSpec`. 범위(양 끝 포함)·기본값은 저장 단위(초·바이트 등)이고 Rust가 정한다. */
export type TuningFieldSpec = {
  key: TuningKey;
  min: number;
  max: number;
  default: number;
  isInteger: boolean;
};

export type TrayIcon = "bars" | "percent" | "icon";

/** Rust `settings::Settings`. */
export type Settings = {
  /** 팝오버(탭·개요)와 트레이 값에 표시하고 조회하는 에이전트. `AGENTS` 순서, 1개 이상. */
  enabledAgents: Agent[];
  trayIcon: TrayIcon;
  tuning: Tuning;
  /** tuning 필드별 범위·기본값. 설정 화면은 이 값으로만 검사·안내한다. */
  tuningSpec: TuningFieldSpec[];
  /** 설정 파일을 못 읽어 기본값을 쓰는 이유. */
  reason: string | null;
  /** 화면 언어 선택(0021). 기본 자동. */
  language: LanguageChoice;
  /** 마지막으로 `setUiLanguage`로 알린 화면 언어. 자동일 때 Rust 문구(트레이·오류·항목 이름)가 쓴다. */
  resolvedLanguage: Lang;
};

/** Rust `settings::SETTINGS_UPDATED_EVENT`. */
export const SETTINGS_UPDATED_EVENT = "settings-updated";

export function getSettings(): Promise<Settings> {
  return invoke<Settings>("get_settings");
}

/** OS의 로그인 시 시작 등록 상태. settings.json에는 저장하지 않는다(0024). */
export function getAutostart(): Promise<boolean> {
  return invoke<boolean>("get_autostart");
}

/** 등록/해제 후 OS에서 다시 읽은 상태를 반환한다. */
export function setAutostart(enabled: boolean): Promise<boolean> {
  return invoke<boolean>("set_autostart", { enabled });
}

/** macOS에서 현재 실행 경로가 /Applications 밖이면 이동 주의 문구를 보인다. */
export function getAutostartPathWarning(): Promise<boolean> {
  return invoke<boolean>("get_autostart_path_warning");
}

/**
 * 켤 에이전트를 저장한다. 빈 목록·알 수 없는 에이전트는 Rust가 `invalid_input`으로 거부한다.
 * `tuning`을 주면 통째로 바꾸고(범위 밖이면 `invalid_input`), 생략하면 지금 값을 유지한다.
 */
export function setSettings(
  enabledAgents: Agent[],
  tuning?: Tuning,
  language?: LanguageChoice,
  trayIcon?: TrayIcon,
): Promise<Settings> {
  return invoke<Settings>("set_settings", {
    enabledAgents,
    ...(tuning && { tuning }),
    ...(language && { language }),
    ...(trayIcon && { trayIcon }),
  });
}

/** 프론트엔드가 정한 화면 언어를 Rust에 알린다(0021). Rust는 남겨 두고, 바뀌었으면 자기 문구를 다시 만든다. */
export function setUiLanguage(language: Lang): Promise<void> {
  return invoke<void>("set_ui_language", { language });
}

export function onSettingsUpdated(handler: (settings: Settings) => void): Promise<UnlistenFn> {
  return listen<Settings>(SETTINGS_UPDATED_EVENT, (event) => handler(event.payload));
}

export * from "@/lib/ipc-coach";
export * from "@/lib/ipc-checks";

/** Rust `fit_popover_height`가 받는 논리 px 범위. */
export const POPOVER_HEIGHT_RANGE = { min: 100, max: 800 } as const;

/** 팝오버 창 높이(논리 px)를 내용에 맞춘다. 범위 밖 값은 범위로 자른다. */
export function fitPopoverHeight(height: number): Promise<void> {
  const { min, max } = POPOVER_HEIGHT_RANGE;
  return invoke("fit_popover_height", { height: Math.min(max, Math.max(min, Math.ceil(height))) });
}

export function quitApp(): Promise<void> {
  return invoke("quit_app");
}

/** 앱 버전(Cargo/tauri.conf의 version). 설정 화면 하단 표시용. */
export function getAppVersion(): Promise<string> {
  return getVersion();
}

/** Rust coach::mcp_guide::McpScopeGuide. Masked commands only; kcal never executes them. */
export type McpScopeGuide = { commands: string[]; undoCommands: string[] };

/** Rust sessions DTOs (0030). Only the displayed, confirmed identities may be stopped. */
export type ClaudeSession = {
  pid: number | null;
  sessionId: string | null;
  cwd: string | null;
  status: string | null;
  startedAt: number | null;
  kind: string | null;
  idleSeconds: number | null;
  stopEligible: boolean;
  exclusion: string | null;
};
export type ClaudeSessions = { sessions: ClaudeSession[]; canStop: boolean; idleHours: number };
export type SessionTarget = { pid: number; sessionId: string };
export type StopSessionsResult = { stopped: number; skipped: string[] };
export function listClaudeSessions(): Promise<ClaudeSessions> {
  return invoke<ClaudeSessions>("list_claude_sessions");
}
export function stopIdleClaudeSessions(targets: SessionTarget[]): Promise<StopSessionsResult> {
  return invoke<StopSessionsResult>("stop_idle_claude_sessions", { targets });
}
