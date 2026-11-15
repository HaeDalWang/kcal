import { describe, expect, it } from "vitest";

import {
  busiestHour,
  dailyGoal,
  dayActivity,
  hourRangeText,
  recentDays,
  ringFraction,
} from "@/lib/daily";
import type { AgentUsage, HourBucket } from "@/lib/ipc";

/** 지역 시각으로 만든 버킷(어느 시간대에서 돌아도 같은 뜻). */
function bucket(day: number, hour: number, requests: number, used: number): HourBucket {
  return { startsAt: new Date(2026, 10, day, hour).toISOString(), requests, used };
}

const KIRO: AgentUsage = {
  agent: "kiro",
  source: "cli",
  observedAt: null,
  windows: [{ id: "month", usedPercent: 10, resetsAt: null, resetPassed: false, limit: 3000 }],
  reason: null,
  reasonKind: null,
  checkedAt: null,
};
const TUNING = { dailyGoalPercent: 14, kiroDailyCredits: 0 };

describe("daily activity", () => {
  it("groups hour buckets by local date and local hour", () => {
    const hourly = [
      bucket(5, 23, 4, 1.5), // 전날
      bucket(6, 0, 1, 0.5),
      bucket(6, 13, 7, 2),
      bucket(6, 23, 2, 0.25),
      bucket(7, 0, 9, 9), // 다음 날
    ];
    const got = dayActivity(hourly, new Date(2026, 10, 6));
    expect(got.used).toBe(2.75);
    expect(got.requests).toHaveLength(24);
    expect([got.requests[0], got.requests[13], got.requests[23]]).toEqual([1, 7, 2]);
    expect(got.requests.reduce((a, b) => a + b, 0)).toBe(10);
  });

  it("lists the last seven local days ending today across a month boundary", () => {
    const days = recentDays(new Date(2026, 10, 3, 15, 30), 7);
    expect(days.map((d) => `${d.getMonth() + 1}/${d.getDate()}`)).toEqual([
      "10/28",
      "10/29",
      "10/30",
      "10/31",
      "11/1",
      "11/2",
      "11/3",
    ]);
    expect(days.every((d) => d.getHours() === 0)).toBe(true);
  });

  it("uses the percent goal for Claude and Codex and splits the Kiro monthly limit by days", () => {
    const november = new Date(2026, 10, 6);
    expect(dailyGoal("codex", TUNING, undefined, november)).toBe(14);
    expect(dailyGoal("kiro", TUNING, KIRO, november)).toBe(100); // 3000 ÷ 30
    expect(dailyGoal("kiro", TUNING, KIRO, new Date(2026, 11, 1))).toBeCloseTo(3000 / 31);
    expect(dailyGoal("kiro", { ...TUNING, kiroDailyCredits: 250 }, KIRO, november)).toBe(250);
    expect(dailyGoal("kiro", TUNING, undefined, november)).toBeNull();
  });

  it("fills the ring up to one turn and leaves it empty without a goal", () => {
    expect(ringFraction(7, 14)).toBe(0.5);
    expect(ringFraction(30, 14)).toBe(1);
    expect(ringFraction(5, null)).toBe(0);
  });

  it("picks the earliest busiest hour and returns null when there were no requests", () => {
    const requests = new Array<number>(24).fill(0);
    expect(busiestHour(requests)).toBeNull();
    requests[9] = 5;
    requests[13] = 8;
    requests[15] = 8;
    expect(busiestHour(requests)).toBe(13);
  });

  it("writes hour ranges with the period on both ends only when it changes", () => {
    expect(hourRangeText(13)).toBe("오후 1:00–2:00");
    expect(hourRangeText(0)).toBe("오전 12:00–1:00");
    expect(hourRangeText(11)).toBe("오전 11:00–오후 12:00");
    expect(hourRangeText(23)).toBe("오후 11:00–오전 12:00");
  });
});
