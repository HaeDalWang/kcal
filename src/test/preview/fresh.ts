// 미리보기 공용: 쿼리(`agents`, `lang`)로 설정을 덮고, 갓 설치한 Mac(HOME에 기록이 없음)의 빈 스냅샷을 만든다.
// 실제 Rust command는 기록이 없어도 null이 아니라 빈 스냅샷을 돌려준다(`metrics::compute`, `context::measure`, `health::compute`).
import type {
  Agent,
  AgentMetrics,
  CodexRate,
  ContextSnapshot,
  HealthReport,
  MetricsSnapshot,
  PeriodStats,
  RateSnapshot,
  Settings,
  UsageSnapshot,
} from "@/lib/ipc";
import settings from "@/test/fixtures/ipc/settings.json";

const HOUR_MS = 3_600_000;
const ALL: readonly Agent[] = ["claude", "codex", "kiro"];

/** `?agents=claude,codex`(쉼표 목록)면 그 에이전트만, `?lang=en|ko`면 그 언어로 설정을 덮는다. 없으면 3개, 언어는 브라우저 언어를 따른다(`language: "auto"`, 화면 언어는 `use-settings`가 이 값과 `navigator.language`로 정한다). */
export function previewSettings(query: URLSearchParams): Settings {
  const raw = query.get("agents");
  const lang = query.get("lang");
  const picked = raw ? ALL.filter((a) => raw.split(",").includes(a)) : [...ALL];
  return {
    ...(settings as Settings),
    enabledAgents: picked,
    ...(lang === "en" || lang === "ko" ? { language: lang, resolvedLanguage: lang } : {}),
  };
}

/** 지표: 에이전트마다 막대 없음, 시간 버킷은 `barDays + 1`일치 0(Rust `hourly`처럼 빈 시간도 0). */
export function freshMetrics(now: number, barDays: number): MetricsSnapshot {
  const end = Math.floor(now / HOUR_MS) * HOUR_MS;
  const count = (barDays + 1) * 24;
  return {
    computedAt: new Date(now).toISOString(),
    agents: ALL.map((agent): AgentMetrics => ({
      agent,
      recentRequests: null,
      cacheHitRatio: null,
      barUnit: agent === "kiro" ? "credits" : "percent",
      bars: [],
      hourly: Array.from({ length: count }, (_, i) => ({
        startsAt: new Date(end - (count - 1 - i) * HOUR_MS).toISOString(),
        requests: 0,
        used: 0,
      })),
    })),
  };
}

/** 컨텍스트: 항목 없음, 합계 0, 실측값 없음. */
export function freshContext(now: number): ContextSnapshot {
  return {
    computedAt: new Date(now).toISOString(),
    agents: ALL.map((agent) => ({
      agent,
      items: [],
      totalStartupTokens: 0,
      firstRequestTokens: null,
      firstRequestChars: null,
      model: null,
      contextWindow: null,
      warnings: [],
    })),
  };
}

/** 처방 효과: 켠 에이전트마다 값을 모르는 주간 비교 한 줄, 적용 기록 없음. */
export function freshHealth(now: number, enabled: Agent[], days: number): HealthReport {
  const to = new Date(now).toISOString();
  const mid = new Date(now - days * 24 * HOUR_MS).toISOString();
  const from = new Date(now - 2 * days * 24 * HOUR_MS).toISOString();
  const period = (agent: Agent, f: string, t: string): PeriodStats => ({
    from: f,
    to: t,
    elapsedDays: days,
    coverage: null,
    requests: null,
    avgInputTokens: null,
    avgCredits: null,
    dailyUsed: null,
    usedUnit: agent === "kiro" ? "credits" : "percent",
    compactions: null,
  });
  return {
    computedAt: to,
    compareDays: days,
    weekly: enabled.map((agent) => ({
      agent,
      before: period(agent, from, mid),
      after: period(agent, mid, to),
    })),
    prescriptions: [],
  };
}

/** 사용량: 조회는 끝났지만 한도 값이 없다. */
export function freshUsage(now: number): UsageSnapshot {
  return {
    agents: ALL.map((agent) => ({
      agent,
      source: null,
      observedAt: null,
      windows: [],
      reason: null,
      reasonKind: null,
      checkedAt: new Date(now).toISOString(),
    })),
  };
}

/** 환율: 기록이 없어 no_data. */
export function freshRates(now: number): RateSnapshot {
  const rate: CodexRate = {
    status: "no_data",
    r2: null,
    windows: 0,
    excludedWindows: 0,
    perPercent: null,
    foldRange: null,
    pattern: null,
    composition: null,
    sessions: [],
  };
  return { computedAt: new Date(now).toISOString(), codex: rate, kiro: [], claude: rate };
}
