// 개발용 미리보기: 대시보드를 IPC fixture + 합성 시간 버킷으로 브라우저에 그린다(Tauri 없이 화면 확인용).
// `pnpm exec vite --port 1480` 뒤 http://127.0.0.1:1480/src/test/preview/dashboard.html
// `?empty`면 시간 버킷·처방 기록을 비워 빈 상태를 본다. `?tab=checks`면 점검 탭으로 연다(이 Mac D·C + Codex·Kiro 스위치, 숨김 1개).
// `&handoff=ok|opus&remaining=N`이면 긴 안내문 카드(Claude에게 맡기기)를 더한다.
// `&rules=ok|opus&remaining=N`이면 늘 실리는 규칙 카드(0018 작업 B)를 더한다.
// `?blank`면 갓 설치한 Mac(기록 없음)을 그린다: 지표·컨텍스트·처방 효과·사용량·환율이 null이 아니라 Rust처럼 빈 스냅샷이다.
// `&agents=claude,codex`(쉼표 목록)면 켠 에이전트를 그것만으로 덮고(기본 3개), `&lang=en|ko`면 화면 언어를 고른다(기본은 브라우저 언어를 따르는 auto라, ko 브라우저면 한국어로 나온다).
// 앱 번들에는 들어가지 않는다(index.html이 참조하지 않음).
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { Dashboard } from "@/components/dashboard";
import type {
  Agent,
  HandoffError,
  HealthReport,
  HourBucket,
  MetricsSnapshot,
  PeriodStats,
  Settings,
  UsageSnapshot,
} from "@/lib/ipc";
import { followSystemTheme } from "@/lib/theme";
import context from "@/test/fixtures/ipc/context-snapshot.json";
import health from "@/test/fixtures/ipc/health-report.json";
import metrics from "@/test/fixtures/ipc/metrics-snapshot.json";
import rates from "@/test/fixtures/ipc/rate-snapshot.json";
import usage from "@/test/fixtures/ipc/usage-snapshot.json";
import {
  CODEX_APPLIED,
  DASHBOARD_CHECKS,
  DASHBOARD_SWITCHES,
  EFFORT_APPLIED,
  EFFORT_PREVIEW,
  LONG_APPLIED,
  LONG_CHECK,
  LONG_PREVIEW,
  RULES_APPLIED,
  RULES_CHECK,
  RULES_PREVIEW,
  SCOPE_PREVIEW,
} from "@/test/preview/checks-fixtures";
import {
  freshContext,
  freshHealth,
  freshMetrics,
  freshRates,
  freshUsage,
  previewSettings,
} from "@/test/preview/fresh";
import "@/index.css";

const HOUR_MS = 3_600_000;
const isEmpty = new URLSearchParams(location.search).has("empty");
const isBlank = new URLSearchParams(location.search).has("blank");

/** 에이전트별 하루 모양(지역 시각별 요청 수 가중치)과 요청당 쓴 양. 결정적이다. */
const SHAPE: Record<Agent, { peak: number; scale: number; perRequest: number }> = {
  claude: { peak: 14, scale: 30, perRequest: 0.05 },
  codex: { peak: 11, scale: 60, perRequest: 0.06 },
  kiro: { peak: 16, scale: 6, perRequest: 0.9 },
};

function synthetic(agent: Agent, now: number): HourBucket[] {
  const end = Math.floor(now / HOUR_MS) * HOUR_MS;
  const { peak, scale, perRequest } = SHAPE[agent];
  return Array.from({ length: 8 * 24 }, (_, i) => {
    const start = end - (8 * 24 - 1 - i) * HOUR_MS;
    const date = new Date(start);
    const hour = date.getHours();
    const isWork = hour >= 9 && hour <= 22;
    const weight = isWork ? Math.max(0, 1 - Math.abs(hour - peak) / 7) : 0;
    const day = date.getDate();
    const requests = Math.round(weight * scale * (0.6 + ((day * 7 + hour * 3) % 10) / 10));
    return { startsAt: new Date(start).toISOString(), requests, used: requests * perRequest };
  });
}

const now = Date.now();
const metricsSnapshot: MetricsSnapshot = {
  ...(metrics as MetricsSnapshot),
  computedAt: new Date(now).toISOString(),
  agents: (metrics as MetricsSnapshot).agents.map((a) => ({
    ...a,
    // fixture 막대는 시각이 모두 같아 React key가 겹친다. 미리보기에서는 5시간씩 띄운다.
    bars: a.bars.map((b, i) => ({
      ...b,
      startsAt: new Date(now - (a.bars.length - i) * 5 * HOUR_MS).toISOString(),
    })),
    hourly: isEmpty ? [] : synthetic(a.agent, now),
  })),
};
const fixtureHealth = health as HealthReport;
const unknown = (p: PeriodStats): PeriodStats => ({
  ...p,
  coverage: null,
  requests: null,
  avgInputTokens: null,
  avgCredits: null,
  dailyUsed: null,
  compactions: null,
});
// 아직 덜 지난 처방 하나를 더해 "N일째"도 보인다.
const running = {
  ...fixtureHealth.prescriptions[0],
  id: "running",
  name: "synthetic-mcp",
  undoneAt: null,
  after: { ...fixtureHealth.prescriptions[0].after, elapsedDays: 2.4 },
};
const healthReport: HealthReport = isEmpty
  ? {
      ...fixtureHealth,
      weekly: fixtureHealth.weekly.map((c) => ({
        ...c,
        before: unknown(c.before),
        after: unknown(c.after),
      })),
      prescriptions: [],
    }
  : { ...fixtureHealth, prescriptions: [running, ...fixtureHealth.prescriptions] };
const allAgents: Settings = previewSettings(new URLSearchParams(location.search));

// `?handoff=ok|opus&remaining=64`: 점검 탭 맨 앞에 긴 안내문 카드(0018 작업 A)를 두고 Claude 남은 한도를 그 %로 둔다.
// ok면 "Claude에게 줄여 달라고 하기"가 미리보기를, opus면 "Opus 5.5를 쓸 수 없음" 오류를 돌려준다.
const query = new URLSearchParams(location.search);
const handoff = query.get("handoff");
const rules = query.get("rules");
const remaining = Number(query.get("remaining") ?? "64");
const future = new Date(now + 3 * HOUR_MS).toISOString();
const usageSnapshot: UsageSnapshot =
  handoff || rules
    ? {
        agents: (usage as UsageSnapshot).agents.map((a) =>
          a.agent === "claude"
            ? {
                ...a,
                windows: [
                  { id: "5h", usedPercent: 100 - remaining, resetsAt: future, resetPassed: false },
                  { id: "7d", usedPercent: 10, resetsAt: future, resetPassed: false },
                ],
              }
            : a,
        ),
      }
    : (usage as UsageSnapshot);
const checks = [
  ...(rules ? [RULES_CHECK] : []),
  ...(handoff ? [LONG_CHECK] : []),
  ...DASHBOARD_CHECKS,
];
const OPUS_UNAVAILABLE: HandoffError = { kind: "opusUnavailable" };
const askHandoff = () =>
  handoff === "opus" ? Promise.reject(OPUS_UNAVAILABLE) : Promise.resolve(LONG_PREVIEW);
const askRules = () =>
  rules === "opus" ? Promise.reject(OPUS_UNAVAILABLE) : Promise.resolve(RULES_PREVIEW);

mockWindows("dashboard");
mockIPC(
  (cmd) => {
    if (isBlank) {
      switch (cmd) {
        case "get_settings":
          return allAgents;
        case "get_usage":
          return freshUsage(now);
        case "get_metrics":
          return freshMetrics(now, allAgents.tuning.barDays);
        case "get_rates":
          return freshRates(now);
        case "get_context":
          return freshContext(now);
        case "get_health":
          return freshHealth(now, allAgents.enabledAgents, allAgents.tuning.compareDays);
        case "get_checks":
        case "get_switches":
        case "get_coach_log":
          return [];
        default:
          return null;
      }
    }
    switch (cmd) {
      case "get_settings":
        return allAgents;
      case "get_usage":
        return usageSnapshot;
      case "get_metrics":
        return metricsSnapshot;
      case "get_rates":
        return rates;
      case "get_context":
        return context;
      case "get_health":
        return healthReport;
      case "get_checks":
        return checks;
      case "ask_claude_handoff":
        return askHandoff();
      case "apply_claude_handoff":
        return LONG_APPLIED;
      case "ask_claude_rules":
        return askRules();
      case "apply_claude_rule":
        return RULES_APPLIED;
      case "get_switches":
        return DASHBOARD_SWITCHES;
      case "preview_scope_move":
        return SCOPE_PREVIEW;
      case "get_coach_log":
        return [CODEX_APPLIED];
      case "preview_check":
        return EFFORT_PREVIEW;
      case "apply_check":
        return EFFORT_APPLIED;
      default:
        return null;
    }
  },
  { shouldMockEvents: true },
);

followSystemTheme();
const root = document.getElementById("root");
if (!root) throw new Error("#root 요소가 없습니다.");
createRoot(root).render(
  <StrictMode>
    <Dashboard />
  </StrictMode>,
);
