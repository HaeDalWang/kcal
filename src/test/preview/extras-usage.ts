// 미리보기 전용(`?s=extras`): 모델 전용 한도·추가 사용량·초기화권이 보이는 합성 사용량(0027 ③). 값은 모두 가짜다.
import type { LimitWindow, UsageSnapshot, WindowId } from "@/lib/ipc";

const MINUTE_MS = 60_000;
const HOUR_MS = 60 * MINUTE_MS;
const DAY_MS = 24 * HOUR_MS;

export function extrasUsage(now: number): UsageSnapshot {
  const iso = (ms: number) => new Date(now + ms).toISOString();
  const seen = iso(-2 * MINUTE_MS);
  const common = {
    source: "api" as const,
    observedAt: seen,
    checkedAt: seen,
    reason: null,
    reasonKind: null,
  };
  const win = (
    id: WindowId,
    usedPercent: number,
    resetsInMs: number,
    extra: Partial<LimitWindow> = {},
  ): LimitWindow => ({
    id,
    usedPercent,
    resetsAt: iso(resetsInMs),
    resetPassed: false,
    ...extra,
  });
  return {
    agents: [
      {
        agent: "claude",
        ...common,
        windows: [win("5h", 40, 2.7 * HOUR_MS), win("7d", 58, 3.1 * DAY_MS)],
        modelWindows: [win("7d", 31, 3.1 * DAY_MS, { group: "Fable" })],
        credits: { used: 12.5, limit: 100, currency: "USD" },
      },
      {
        agent: "codex",
        ...common,
        windows: [win("5h", 21, 3.7 * HOUR_MS), win("7d", 33, 4.4 * DAY_MS)],
        modelWindows: [win("7d", 12, 4.4 * DAY_MS, { group: "GPT-5.3-Codex-Spark" })],
        resetPasses: 2,
      },
      {
        agent: "kiro",
        ...common,
        source: "cli",
        windows: [win("month", 30, 12 * DAY_MS, { used: 1500, limit: 5000 })],
      },
    ],
  };
}
