import { t } from "@/lib/i18n";
import type { LimitWindow, WindowId } from "@/lib/ipc";

const MINUTE_MS = 60_000;
const HOUR_MS = 60 * MINUTE_MS;
const FIXED_LENGTH_MS: Record<Exclude<WindowId, "month">, number> = {
  "5h": 5 * HOUR_MS,
  "7d": 7 * 24 * HOUR_MS,
};

/** 밀리초를 "12m", "4h 35m", "2d 3h"로. 음수는 0m. */
export function formatDuration(ms: number): string {
  const minutes = Math.max(0, Math.round(ms / MINUTE_MS));
  if (minutes < 60) return `${minutes}m`;
  if (minutes < 1440) return `${Math.floor(minutes / 60)}h ${minutes % 60}m`;
  return `${Math.floor(minutes / 1440)}d ${Math.floor((minutes % 1440) / 60)}h`;
}

/** 창 길이(ms). 월 창은 `resetsAt`에서 달력 1개월을 뺀 시각이 시작이다. */
function windowLength(id: WindowId, resetsAt: number): number {
  if (id !== "month") return FIXED_LENGTH_MS[id];
  const start = new Date(resetsAt);
  start.setMonth(start.getMonth() - 1);
  return resetsAt - start.getTime();
}

export type Pace = {
  /** 균등하게 썼다면 지금 남아 있을 %. 막대의 표시선 위치. */
  expectedRemaining: number;
  /** "예상 속도", "고르게 쓸 때보다 N% 적게/많이 씀". */
  deltaText: string;
  /** "초기화까지 버팀" 또는 "3h 41m 후 소진". */
  outlookText: string;
  /** 예상보다 빨리 쓰고 있다. */
  isShort: boolean;
};

/**
 * 지금 속도 평가. `remaining`은 표시용 남은 %.
 * 초기화 시각이 없거나 지났거나 창이 막 시작했으면 null.
 */
export function paceOf(window: LimitWindow, remaining: number, now: number): Pace | null {
  if (!window.resetsAt || window.resetPassed) return null;
  const resetsAt = Date.parse(window.resetsAt);
  const untilReset = resetsAt - now;
  if (untilReset <= 0) return null;
  const length = windowLength(window.id, resetsAt);
  const elapsedMs = now - (resetsAt - length);
  const elapsed = Math.min(1, Math.max(0, elapsedMs / length));
  if (elapsed === 0) return null;

  const expectedRemaining = 100 * (1 - elapsed);
  const delta = Math.round(remaining - expectedRemaining);
  const deltaText =
    delta === 0 ? t.pace.onPace : delta > 0 ? t.pace.ahead(delta) : t.pace.behind(-delta);

  const used = 100 - remaining;
  const untilEmpty = used === 0 ? Infinity : remaining / (used / elapsedMs);
  const outlookText =
    untilEmpty >= untilReset ? t.pace.lasts : t.pace.runsOut(formatDuration(untilEmpty));
  return { expectedRemaining, deltaText, outlookText, isShort: delta < 0 };
}
