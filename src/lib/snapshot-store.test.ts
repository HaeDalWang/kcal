import { describe, expect, it } from "vitest";

import { formatAge } from "@/lib/snapshot-store";

describe("formatAge", () => {
  const now = Date.parse("2026-11-04T12:00:00Z");
  const ago = (ms: number) => new Date(now - ms).toISOString();

  it("says 방금 under one minute, including a clock slightly ahead", () => {
    expect(formatAge(ago(59_000), now)).toBe("방금");
    expect(formatAge(ago(-5_000), now)).toBe("방금");
  });

  it("counts whole minutes, then hours", () => {
    expect(formatAge(ago(60_000), now)).toBe("1분 전 기준");
    expect(formatAge(ago(59 * 60_000), now)).toBe("59분 전 기준");
    expect(formatAge(ago(125 * 60_000), now)).toBe("2시간 전 기준");
  });
});
