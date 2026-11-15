import { describe, expect, it } from "vitest";

import { formatDuration, paceOf } from "@/lib/pace";
import type { LimitWindow, WindowId } from "@/lib/ipc";

const HOUR_MS = 3_600_000;
const now = Date.parse("2026-11-05T12:00:00Z");

/** 5시간 창에서 `elapsedHours`가 지난 상태. */
function window5h(elapsedHours: number, id: WindowId = "5h"): LimitWindow {
  return {
    id,
    usedPercent: 0,
    resetsAt: new Date(now + (5 - elapsedHours) * HOUR_MS).toISOString(),
    resetPassed: false,
  };
}

describe("paceOf", () => {
  it("reports spare percent when remaining is above the even-use line", () => {
    // 2.5h 경과 → 기대 50% 남음. 실제 60% 남음.
    const pace = paceOf(window5h(2.5), 60, now);
    expect(pace?.expectedRemaining).toBe(50);
    expect(pace?.deltaText).toBe("고르게 쓸 때보다 10% 적게 씀");
    expect(pace?.isShort).toBe(false);
    expect(pace?.outlookText).toBe("초기화까지 버팀");
  });

  it("reports shortage and time until empty when remaining is below the line", () => {
    // 2.5h 경과, 40% 남음(60% 사용) → 속도 24%/h, 바닥까지 1h 40m.
    const pace = paceOf(window5h(2.5), 40, now);
    expect(pace?.deltaText).toBe("고르게 쓸 때보다 10% 많이 씀");
    expect(pace?.isShort).toBe(true);
    expect(pace?.outlookText).toBe("1h 40m 후 소진");
  });

  it("reports expected speed when the difference rounds to zero", () => {
    expect(paceOf(window5h(2.5), 50.4, now)?.deltaText).toBe("예상 속도");
  });

  it("holds until reset when nothing is used yet", () => {
    expect(paceOf(window5h(1), 100, now)?.outlookText).toBe("초기화까지 버팀");
  });

  it("uses the calendar month before resetsAt as the month window start", () => {
    // 11/05 12:00Z 리셋 기준 시작은 10/05(지역 달력). 길이는 30일 안팎.
    const resetsAt = new Date(now + 15 * 24 * HOUR_MS).toISOString();
    const pace = paceOf({ id: "month", usedPercent: 0, resetsAt, resetPassed: false }, 50, now);
    expect(pace?.expectedRemaining).toBeGreaterThan(45);
    expect(pace?.expectedRemaining).toBeLessThan(55);
  });

  it("returns null without resetsAt, after reset, or before any time has elapsed", () => {
    const base = window5h(2);
    expect(paceOf({ ...base, resetsAt: null }, 50, now)).toBeNull();
    expect(paceOf({ ...base, resetPassed: true }, 50, now)).toBeNull();
    expect(paceOf(window5h(5.5), 50, now)).toBeNull();
    expect(paceOf(window5h(0), 100, now)).toBeNull();
  });
});

describe("formatDuration", () => {
  it("formats durations as minutes, hours and days", () => {
    expect(formatDuration(-5)).toBe("0m");
    expect(formatDuration(59 * 60_000)).toBe("59m");
    expect(formatDuration(2 * HOUR_MS + 13 * 60_000)).toBe("2h 13m");
    expect(formatDuration(76 * HOUR_MS)).toBe("3d 4h");
  });
});
