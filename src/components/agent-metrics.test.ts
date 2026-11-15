import { describe, expect, it } from "vitest";

import { barTitle, startupLine } from "@/components/agent-metrics";
import type { AgentContext, CodexRate } from "@/lib/ipc";

const context: AgentContext = {
  agent: "codex",
  items: [
    {
      kind: "system",
      name: "base_instructions",
      bytes: 48_000,
      tokens: 12_000,
      source: "measured",
      loading: "startup",
    },
  ],
  totalStartupTokens: 12_000,
  firstRequestTokens: null,
  firstRequestChars: null,
  model: null,
  contextWindow: null,
  warnings: [],
};

const rate: CodexRate = {
  status: "estimated",
  r2: 0.9,
  windows: 20,
  excludedWindows: 0,
  perPercent: { cachedInput: 600_000, uncachedInput: 60_000, output: 5_000 },
  foldRange: {
    cachedInput: { min: 500_000, max: 700_000 },
    uncachedInput: { min: 50_000, max: 70_000 },
    output: { min: 4_000, max: 6_000 },
  },
  pattern: {
    days: 14,
    windows: 20,
    requests: { median: 100, p90: 150 },
    misses: { median: 0, p90: 2 },
  },
  composition: null,
  sessions: [],
};

describe("startupLine", () => {
  it("labels the share of the 5-hour limit separately from the context window share", () => {
    // 12K × 100 요청 / 600K = 2%. 창 몫(%)이 아니라 한도 몫 문장으로 따로 둔다.
    const line = startupLine(context, rate);
    expect(line?.value).toBe("12K 토큰");
    expect(line?.limitShare).toBe("요청마다 다시 실려서 5시간 한도의 ~2%");
    expect(startupLine(context, null)?.limitShare).toBeNull();
    expect(startupLine({ ...context, items: [] }, rate)).toBeNull();
  });

  it("hides the limit share while the Claude rate is collected", () => {
    const claude = { ...context, agent: "claude" as const };
    expect(startupLine(claude, rate)?.limitShare).toBe("요청마다 다시 실려서 5시간 한도의 ~2%");
    const collecting: CodexRate = { ...rate, status: "unreliable", perPercent: null };
    expect(startupLine(claude, collecting)?.limitShare).toBeNull();
  });

  it("shows startup tokens as a share of the context window only when the window is known", () => {
    const withWindow = { ...context, model: "gpt-6.1-sol", contextWindow: 400_000 };
    expect(startupLine(withWindow, null)?.value).toBe("3% (12K / 400K 토큰)");
    expect(startupLine(withWindow, null)?.title).toContain("gpt-6.1-sol 컨텍스트 창 400K 토큰");
    // 예상 항목이 섞이면 %와 토큰 모두 `~`.
    const mixed: AgentContext = {
      ...withWindow,
      items: [
        ...context.items,
        { ...context.items[0], kind: "rules", tokens: 0, source: "estimated" },
      ],
    };
    expect(startupLine(mixed, null)?.value).toBe("~3% (~12K / 400K 토큰)");
  });

  it("groups startup items in /context order and skips conditional items", () => {
    const item = context.items[0];
    const grouped: AgentContext = {
      ...context,
      items: [
        { ...item, kind: "skills", tokens: 300, source: "measured" },
        { ...item, kind: "plugins", tokens: 100, source: "estimated" },
        { ...item, kind: "rules", tokens: 50 },
        { ...item, kind: "instructions", tokens: 25 },
        { ...item, kind: "tools", tokens: 1_000 },
        { ...item, kind: "agents", tokens: 900, loading: "conditional" },
        { ...item, kind: "hooks", tokens: null },
      ],
    };
    expect(
      startupLine(grouped, null)?.groups.map((g) => [g.label, g.tokens, g.isEstimated]),
    ).toEqual([
      ["기본 도구", 1_000, false],
      ["지시 파일", 75, false],
      ["스킬", 400, true],
    ]);
  });
});

describe("barTitle", () => {
  it("shows the local start time and the value with its unit", () => {
    const startsAt = new Date(2026, 10, 5, 9, 5).toISOString();
    expect(barTitle({ startsAt, value: 31.4, isCurrent: false }, "percent")).toBe(
      "11/05 09:05 시작 · 31%",
    );
    expect(barTitle({ startsAt, value: 1.256, isCurrent: true }, "credits")).toBe(
      "11/05 09:05 시작 · 1.26 크레딧",
    );
  });
});
