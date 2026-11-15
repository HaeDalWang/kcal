import { describe, expect, it } from "vitest";

import {
  compositionOf,
  kiroMonthlyLimit,
  kiroTurnShare,
  shareOf,
  tokensPerPercent,
  windowShare,
} from "@/lib/breakdown";
import type { AgentUsage, CodexRate } from "@/lib/ipc";

const rate: CodexRate = {
  status: "estimated",
  r2: 0.92,
  windows: 19,
  excludedWindows: 0,
  perPercent: { cachedInput: 742_000, uncachedInput: 66_000, output: 5_500 },
  foldRange: {
    cachedInput: { min: 716_800, max: 810_300 },
    uncachedInput: { min: 56_200, max: 75_600 },
    output: { min: 4_900, max: 6_200 },
  },
  pattern: {
    days: 14,
    windows: 19,
    requests: { median: 139, p90: 213 },
    misses: { median: 2, p90: 5 },
  },
  composition: null,
  sessions: [],
};

describe("windowShare", () => {
  it("is 1%p for one cached request of the cached rate and one missed request of the uncached rate", () => {
    const per = { cachedInput: 742_000, uncachedInput: 66_000 };
    expect(windowShare(742_000, 1, 0, per)).toBeCloseTo(1, 12);
    expect(windowShare(66_000, 1, 1, per)).toBeCloseTo(1, 12);
  });

  it("drops a term whose rate is null (coefficient 0)", () => {
    expect(windowShare(1_000, 10, 2, { cachedInput: null, uncachedInput: 1_000 })).toBe(2);
    expect(windowShare(1_000, 10, 2, { cachedInput: 1_000, uncachedInput: null })).toBe(8);
  });
});

describe("shareOf", () => {
  it("uses the median window for the point and fold bounds with median/p90 windows for the range", () => {
    const share = shareOf(4_457, rate)!;
    expect(share.point).toBeCloseTo((4_457 * 137) / 742_000 + (4_457 * 2) / 66_000, 12);
    expect(share.low).toBeCloseTo((4_457 * 137) / 810_300 + (4_457 * 2) / 75_600, 12);
    expect(share.high).toBeCloseTo((4_457 * 208) / 716_800 + (4_457 * 5) / 56_200, 12);
    expect(share.low).toBeLessThan(share.point);
    expect(share.high).toBeGreaterThan(share.point);
  });

  it("returns null unless the rate is estimated and a window pattern exists", () => {
    expect(shareOf(100, { ...rate, status: "range_only", perPercent: null })).toBeNull();
    expect(shareOf(100, { ...rate, pattern: null })).toBeNull();
  });
});

describe("tokensPerPercent", () => {
  it("is about 1.11M for cached 1.8M, uncached 55.7K, 201 requests with 4 misses", () => {
    const value = tokensPerPercent(201, 4, { cachedInput: 1_800_000, uncachedInput: 55_700 })!;
    expect(value).toBeGreaterThan(1_100_000);
    expect(value).toBeLessThan(1_120_000);
  });

  it("is null when both coefficients are 0", () => {
    expect(tokensPerPercent(10, 1, { cachedInput: null, uncachedInput: null })).toBeNull();
  });
});

describe("compositionOf", () => {
  // 1%당 캐시 1,000 · 비캐시 100 토큰. 창 패턴 중앙값 N=10, M=1(고른 구간은 N=12, M=3).
  const withComposition: CodexRate = {
    ...rate,
    perPercent: { cachedInput: 1_000, uncachedInput: 100, output: 10 },
    pattern: { ...rate.pattern!, requests: { median: 10, p90: 20 }, misses: { median: 1, p90: 2 } },
    composition: {
      windows: 5,
      requests: 12,
      misses: 3,
      inputShare: 40,
      outputShare: 8,
      usedPercent: 50,
    },
  };

  it("subtracts the startup share shown on the startup line from the input share", () => {
    // 시작 100토큰, 창 패턴 중앙값으로: 100 × 9 / 1,000 + 100 × 1 / 100 = 1.9(고른 구간 N·M이 아니다)
    const got = compositionOf(withComposition, 100)!;
    expect(got.startup).toBeCloseTo(1.9, 12);
    expect(got.conversation).toBeCloseTo(38.1, 12);
    expect(got.output).toBe(8);
    expect(got.predicted).toBe(48);
    expect(got.used).toBe(50);
  });

  it("clamps the conversation share to 0 when startup exceeds the input share", () => {
    expect(compositionOf(withComposition, 100_000)!.conversation).toBe(0);
  });

  it("keeps the whole input share as conversation when startup tokens are unknown", () => {
    const got = compositionOf(withComposition, null)!;
    expect(got.startup).toBeNull();
    expect(got.conversation).toBe(40);
  });

  it("returns null unless the rate is estimated with a composition and a positive total", () => {
    expect(compositionOf(rate, 100)).toBeNull();
    expect(compositionOf({ ...withComposition, pattern: null }, 100)).toBeNull();
    expect(compositionOf({ ...withComposition, status: "range_only" }, 100)).toBeNull();
    const empty = { ...withComposition.composition!, inputShare: 0, outputShare: 0 };
    expect(compositionOf({ ...withComposition, composition: empty }, 100)).toBeNull();
  });
});

describe("kiroTurnShare", () => {
  const usage = (limit?: number): AgentUsage => ({
    agent: "kiro",
    source: "cli",
    observedAt: null,
    windows: [{ id: "month", usedPercent: 10, resetsAt: null, resetPassed: false, limit }],
    reason: null,
    reasonKind: null,
    checkedAt: null,
  });

  it("is credits per turn over one percent of the monthly limit", () => {
    // 9.5 크레딧 ÷ (5,000 / 100) = 0.19%
    expect(kiroTurnShare(9.5, kiroMonthlyLimit(usage(5_000)))).toBeCloseTo(0.19, 12);
  });

  it("is null when the monthly limit is unknown or not positive", () => {
    expect(kiroMonthlyLimit(usage())).toBeNull();
    expect(kiroMonthlyLimit(usage(0))).toBeNull();
    expect(kiroMonthlyLimit(undefined)).toBeNull();
    expect(kiroTurnShare(9.5, null)).toBeNull();
  });
});
