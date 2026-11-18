import { render, screen } from "@testing-library/react";
import { mockIPC } from "@tauri-apps/api/mocks";
import { describe, expect, it, vi } from "vitest";

import {
  CodexBehaviorDetails,
  compositionParts,
  legendText,
  sessionLabels,
} from "@/components/behavior-section";
import { formatShare } from "@/components/rate-line";
import type { CodexRate, CodexSession, ContextSnapshot, RateSnapshot } from "@/lib/ipc";
import { contextStore, ratesStore } from "@/lib/snapshot-store";

const AT = "2026-11-05T00:00:00Z";

const session: CodexSession = {
  name: "kcal",
  cwd: "/w/kcal",
  startedAt: "2026-10-30T09:00:00Z",
  requests: 421,
  avgInputTokens: 126_000,
  misses: 17,
  share: 175,
  isMarathon: true,
  isLargeContext: false,
};

// 1%당 캐시 1,000 · 비캐시 100 · 출력 10. 구간 N=10, M=1 → 시작 100토큰 = 1.9%.
const codex: CodexRate = {
  status: "estimated",
  r2: 0.9,
  windows: 20,
  excludedWindows: 0,
  perPercent: { cachedInput: 1_000, uncachedInput: 100, output: 10 },
  foldRange: null,
  pattern: {
    days: 14,
    windows: 20,
    requests: { median: 10, p90: 20 },
    misses: { median: 1, p90: 2 },
  },
  composition: {
    windows: 20,
    requests: 10,
    misses: 1,
    inputShare: 40,
    outputShare: 8,
    usedPercent: 50,
  },
  sessions: [session],
};

function rates(rate: CodexRate): RateSnapshot {
  return {
    computedAt: AT,
    codex: rate,
    kiro: [],
    claude: { ...rate, status: "no_data", perPercent: null },
  };
}

const context: ContextSnapshot = {
  computedAt: AT,
  agents: [
    {
      agent: "codex",
      items: [
        {
          kind: "instructions",
          name: "AGENTS.md",
          bytes: 400,
          tokens: 100,
          source: "estimated",
          loading: "startup",
        },
      ],
      totalStartupTokens: 100,
      firstRequestTokens: null,
      firstRequestChars: null,
      model: null,
      contextWindow: null,
      warnings: [],
    },
  ],
};

function load(snapshot: RateSnapshot) {
  mockIPC((cmd) => {
    if (cmd === "get_rates") return snapshot;
    if (cmd === "get_context") return context;
    return undefined;
  });
  ratesStore.refresh();
  contextStore.refresh();
}

/** 저장소에 값이 들어올 때까지 기다린다(숨김 확인은 값이 온 뒤에 한다). */
async function loaded() {
  await vi.waitFor(() => {
    expect(ratesStore.getSnapshot().value).toBeDefined();
    expect(contextStore.getSnapshot().value).toBeDefined();
  });
}

describe("compositionParts", () => {
  it("drops parts that are 0 so no empty slot or 0% is shown", () => {
    const parts = compositionParts({
      startup: 2,
      conversation: 0,
      output: 8,
      predicted: 10,
      used: 9,
    });
    expect(parts.map((p) => p.label)).toEqual(["설정", "답변"]);
  });

  it("labels the whole input as 입력 when startup tokens are unknown", () => {
    const parts = compositionParts({
      startup: null,
      conversation: 40,
      output: 8,
      predicted: 48,
      used: 50,
    });
    expect(legendText(parts)).toBe("입력 ~40% · 답변 ~8% (예상)");
  });

  it("formats small shares with two significant digits", () => {
    expect([formatShare(0.1875), formatShare(2.14), formatShare(38.1)]).toEqual([
      "0.19",
      "2.1",
      "38",
    ]);
  });
});

// 구성 막대·많이 쓴 세션은 대시보드 Codex 카드에만 있다(0019 D-06). 트레이에 없는 것은 usage-popover.test.tsx.
describe("CodexBehaviorDetails", () => {
  it("shows the three shares and a top session with its pattern badge on the dashboard", async () => {
    load(rates(codex));
    render(<CodexBehaviorDetails />);
    expect(await screen.findByText("설정 ~1.9% · 대화 누적 ~38% · 답변 ~8%")).toBeTruthy();
    const row = screen.getByText("kcal").closest("li")!;
    expect(row.textContent).toContain("긴 세션");
    expect(row.textContent).not.toContain("큰 컨텍스트");
    expect(row.textContent).toContain("421번");
    expect(row.textContent).toContain("~175%");
  });

  it("hides the whole section when the rate is not estimated", async () => {
    load(rates({ ...codex, status: "range_only", composition: null, sessions: [] }));
    const { container } = render(<CodexBehaviorDetails />);
    await loaded();
    expect(container.textContent).toBe("");
  });

  it("puts the validation line and session details on the dashboard", async () => {
    load(rates(codex));
    render(<CodexBehaviorDetails />);
    const value = await screen.findByTitle(/실제 50% · 예상 48%/);
    expect(value.textContent).not.toContain("실제");
    expect(screen.getByText(/요청당 입력 126K 토큰 · 캐시 미스 17번/)).toBeTruthy();
  });
});

describe("sessionLabels", () => {
  const at = (hour: number) => new Date(2026, 9, 30, hour, 5).toISOString();
  const make = (name: string, cwd: string | null, hour: number): CodexSession => ({
    ...session,
    name,
    cwd,
    startedAt: at(hour),
  });

  it("keeps the plain name and date when no name repeats", () => {
    const got = sessionLabels([make("a", "/w/a", 9), make("b", "/w/b", 9)]);
    expect(got).toEqual([
      { name: "a", when: "10/30" },
      { name: "b", when: "10/30" },
    ]);
  });

  it("adds the parent folder when the same name comes from different paths", () => {
    const got = sessionLabels([make("app", "/work/web/app", 9), make("app", "/play/app", 10)]);
    expect(got.map((l) => [l.name, l.when])).toEqual([
      ["web/app", "10/30"],
      ["play/app", "10/30"],
    ]);
    const windows = sessionLabels([
      make("app", "C:\\w\\x\\app", 9),
      make("app", "C:\\w\\y\\app", 9),
    ]);
    expect(windows.map((l) => l.name)).toEqual(["x/app", "y/app"]);
  });

  it("adds the start time when the same folder appears more than once", () => {
    const got = sessionLabels([make("app", "/w/app", 9), make("app", "/w/app", 14)]);
    expect(got.map((l) => [l.name, l.when])).toEqual([
      ["app", "10/30 09:05"],
      ["app", "10/30 14:05"],
    ]);
  });

  it("falls back to the start time when the path is unknown or the parents match too", () => {
    const unknown = sessionLabels([make("app", null, 9), make("app", null, 14)]);
    expect(unknown.map((l) => l.when)).toEqual(["10/30 09:05", "10/30 14:05"]);
    const same = sessionLabels([make("app", "/a/x/app", 9), make("app", "/b/x/app", 14)]);
    expect(same.map((l) => [l.name, l.when])).toEqual([
      ["x/app", "10/30 09:05"],
      ["x/app", "10/30 14:05"],
    ]);
  });
});
