import { describe, expect, it } from "vitest";

import { barTitle, startupTitle, startupValue } from "@/components/agent-metrics";
import type { AgentContext } from "@/lib/ipc";

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

describe("startupValue", () => {
  it("shows startup tokens as a share of the context window only when the window is known", () => {
    expect(startupValue(context)).toBe("12K 토큰");
    const withWindow = { ...context, model: "gpt-6.1-sol", contextWindow: 400_000 };
    expect(startupValue(withWindow)).toBe("3% (12K / 400K 토큰)");
    expect(startupTitle(withWindow)).toContain("gpt-6.1-sol 컨텍스트 창 400K 토큰");
    // 예상 항목이 섞이면 %와 토큰 모두 `~`.
    const mixed: AgentContext = {
      ...withWindow,
      items: [
        ...context.items,
        { ...context.items[0], kind: "rules", tokens: 0, source: "estimated" },
      ],
    };
    expect(startupValue(mixed)).toBe("~3% (~12K / 400K 토큰)");
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
