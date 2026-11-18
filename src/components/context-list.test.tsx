import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";
import { describe, expect, it } from "vitest";

import { ContextList, TOP_PER_GROUP } from "@/components/context-list";
import { RateLine } from "@/components/rate-line";
import { DEFAULT_TUNING } from "@/test/tuning";
import type { CodexRate, ContextItem, ContextSnapshot, RateSnapshot } from "@/lib/ipc";

const item = (
  name: string,
  tokens: number | null,
  extra: Partial<ContextItem> = {},
): ContextItem => ({
  kind: "rules",
  name,
  bytes: tokens === null ? null : tokens * 4,
  tokens,
  source: "estimated",
  loading: "startup",
  ...extra,
});

const COMPUTED_AT = new Date().toISOString();

function snapshot(): ContextSnapshot {
  const filler = Array.from({ length: 8 }, (_, i) => item(`rule-${i}.md`, 10));
  return {
    computedAt: COMPUTED_AT,
    agents: [
      {
        agent: "claude",
        items: [
          item("시스템 프롬프트 (블록 3개)", 3045, { kind: "system", source: "measured" }),
          ...filler,
          item("leak-scan.md", 1546, { kind: "agents", loading: "conditional" }),
          item("hook 19개", null, { kind: "hooks", loading: "conditional" }),
        ],
        totalStartupTokens: 3045 + 8 * 10,
        firstRequestTokens: null,
        firstRequestChars: null,
        model: null,
        contextWindow: null,
        warnings: ["hook 19개: 출력 크기는 실행할 때 정해져 세지 않았습니다."],
      },
    ],
  };
}

describe("ContextList", () => {
  it("groups items with subtotals, keeps the top rows, and unfolds the rest and the notes on demand", async () => {
    let calls = 0;
    mockIPC((cmd, args) => {
      if (cmd !== "get_context") return undefined;
      calls += 1;
      expect(args).toEqual({ workspace: null });
      return snapshot();
    });
    render(<ContextList agent="claude" charsPerToken={DEFAULT_TUNING.bytesPerToken} />);

    await screen.findByText("~3.1K 토큰");
    const group = screen.getByRole("group", { name: "시작 컨텍스트" });
    const total = within(group).getByText("~3.1K 토큰");
    expect(total.getAttribute("title")).toBe("예상: 기록으로 계산한 값 · 토큰 = 바이트 ÷ 4");
    expect(group.querySelector("[data-slot=details]")!.textContent).toContain(
      "측정한 값 3,045 토큰 · 예상한 값 80 토큰",
    );
    expect(within(group).getByRole("img", { name: "구성 비율" })).toBeTruthy();
    // 묶음 소계: 값 칸의 ~는 예상이 섞였을 때만 붙는다.
    expect(within(group).getByText("시스템·도구").parentElement!.parentElement!.textContent).toBe(
      "시스템·도구3,045",
    );
    expect(within(group).getByText("지시 파일").parentElement!.parentElement!.textContent).toBe(
      "지시 파일~80",
    );

    const rows = within(group).getAllByRole("listitem");
    expect(rows).toHaveLength(1 + TOP_PER_GROUP);
    expect(rows[0].textContent).toBe("시스템 프롬프트 (블록 3개)3,045");
    expect(rows[1].textContent).toBe("rule-0.md~10");
    // 필요할 때만 실리는 항목은 한 줄 요약으로 맨 끝에 둔다.
    expect(within(group).getByText("필요할 때만 2개 · ~1,546 토큰")).toBeTruthy();
    expect(within(group).queryByText("leak-scan.md")).toBeNull();
    expect(within(group).queryByText(/hook 19개/)).toBeNull();

    const more = within(group).getByRole("button", { name: "7개 더 보기" });
    expect(more.getAttribute("aria-expanded")).toBe("false");
    fireEvent.click(more);
    expect(more.getAttribute("aria-expanded")).toBe("true");
    expect(more.textContent).toBe("접기");
    expect(within(group).getAllByRole("listitem")).toHaveLength(1 + 8 + 2);
    expect(within(group).getByText("leak-scan.md").parentElement!.textContent).toBe(
      "leak-scan.md~1,546",
    );
    expect(within(group).getByText(/hook 19개: 출력 크기는/)).toBeTruthy();
    fireEvent.click(more);
    expect(within(group).queryByText(/hook 19개: 출력 크기는/)).toBeNull();
    expect(calls).toBe(1);
  });

  it("shows the share of the context window in its own column only when the window is known", async () => {
    const withWindow = snapshot();
    withWindow.agents[0].contextWindow = 1_000_000;
    mockIPC((cmd) => (cmd === "get_context" ? withWindow : undefined));
    render(<ContextList agent="claude" charsPerToken={DEFAULT_TUNING.bytesPerToken} />);
    await screen.findByText("시스템 프롬프트 (블록 3개)");
    const group = screen.getByRole("group", { name: "시작 컨텍스트" });
    const rows = within(group).getAllByRole("listitem");
    expect(rows[0].textContent).toBe("시스템 프롬프트 (블록 3개)3,0450.3%");
    // 0.01% 미만은 줄여 쓴다.
    expect(rows[1].textContent).toBe("rule-0.md~10<0.01%");
    expect(within(group).getAllByText("0.3%")[0].getAttribute("title")).toBe(
      "컨텍스트 창에서 차지하는 몫",
    );
  });

  it("shows the first request tokens in the details when the agent reports them", async () => {
    mockIPC((cmd) =>
      cmd === "get_context"
        ? {
            computedAt: COMPUTED_AT,
            agents: [
              {
                agent: "codex",
                items: [item("시스템 지시문", 100, { kind: "system", source: "measured" })],
                totalStartupTokens: 100,
                firstRequestTokens: 29861,
                firstRequestChars: null,
                model: null,
                contextWindow: null,
                warnings: [],
              },
            ],
          }
        : undefined,
    );
    render(<ContextList agent="codex" charsPerToken={DEFAULT_TUNING.bytesPerToken} />);
    expect(await screen.findByText("첫 요청 29,861 토큰(첫 메시지 포함)")).toBeTruthy();
  });

  it.each([
    [4, "~3,086"],
    [5, "~2,469"],
  ])(
    "converts assembled characters to tokens with charsPerToken %i",
    async (charsPerToken, expected) => {
      mockIPC((cmd) =>
        cmd === "get_context"
          ? {
              computedAt: COMPUTED_AT,
              agents: [
                {
                  agent: "kiro",
                  items: [item("a.md", 10)],
                  totalStartupTokens: 10,
                  firstRequestTokens: null,
                  firstRequestChars: 12345,
                  warnings: [],
                },
              ],
            }
          : undefined,
      );
      render(<ContextList agent="kiro" charsPerToken={charsPerToken} />);
      expect(await screen.findByText(new RegExp(expected))).toBeTruthy();
    },
  );

  it("shows the assembled characters with an approximate token conversion", async () => {
    mockIPC((cmd) =>
      cmd === "get_context"
        ? {
            computedAt: COMPUTED_AT,
            agents: [
              {
                agent: "kiro",
                items: [item("a.md", 10)],
                totalStartupTokens: 10,
                firstRequestTokens: null,
                firstRequestChars: 12345,
                warnings: [],
              },
            ],
          }
        : undefined,
    );
    render(<ContextList agent="kiro" charsPerToken={DEFAULT_TUNING.bytesPerToken} />);
    expect(await screen.findByText("시작 때 조립한 글 12,345자(~3,086 토큰)")).toBeTruthy();
  });

  it("says so when the agent is missing from the response", async () => {
    mockIPC((cmd) => (cmd === "get_context" ? { computedAt: COMPUTED_AT, agents: [] } : undefined));
    render(<ContextList agent="kiro" charsPerToken={DEFAULT_TUNING.bytesPerToken} />);
    expect(await screen.findByText("값 없음")).toBeTruthy();
  });

  it("shows the last value immediately when the tab is opened again", async () => {
    mockIPC((cmd) => (cmd === "get_context" ? snapshot() : undefined));
    const first = render(
      <ContextList agent="claude" charsPerToken={DEFAULT_TUNING.bytesPerToken} />,
    );
    await screen.findByText(/~3.1K 토큰/);
    first.unmount();

    render(<ContextList agent="claude" charsPerToken={DEFAULT_TUNING.bytesPerToken} />);
    expect(screen.getByText(/~3.1K 토큰/)).toBeTruthy();
    expect(screen.queryByText("불러오는 중…")).toBeNull();
  });

  it("replaces the value when context-updated arrives and shows how old it is", async () => {
    const old = new Date(Date.now() - 7 * 60_000).toISOString();
    mockIPC((cmd) => (cmd === "get_context" ? { ...snapshot(), computedAt: old } : undefined), {
      shouldMockEvents: true,
    });
    render(<ContextList agent="claude" charsPerToken={DEFAULT_TUNING.bytesPerToken} />);
    expect(await screen.findByText("7분 전 기준")).toBeTruthy();

    const next = snapshot();
    next.agents[0].items = [item("new.md", 5)];
    next.agents[0].totalStartupTokens = 5;
    await act(() => emit("context-updated", { ...next, computedAt: new Date().toISOString() }));
    expect(await screen.findByText(/~5 토큰/)).toBeTruthy();
    expect(screen.getByText("방금")).toBeTruthy();
  });

  it("asks again when the popover regains focus instead of waiting for an event", async () => {
    let calls = 0;
    mockIPC((cmd) => {
      if (cmd !== "get_context") return undefined;
      calls += 1;
      return snapshot();
    });
    render(<ContextList agent="claude" charsPerToken={DEFAULT_TUNING.bytesPerToken} />);
    await screen.findByText(/~3.1K 토큰/);
    await act(async () => {
      window.dispatchEvent(new Event("focus"));
    });
    expect(calls).toBe(2);
  });
});

describe("ContextList Codex share", () => {
  // 1%당 캐시 1,000 · 비캐시 100 토큰, 보통 구간 N=10·M=1 → 몫 = T×9/1000 + T×1/100 = T × 0.019.
  const codex: CodexRate = {
    status: "estimated",
    r2: 0.9,
    windows: 20,
    excludedWindows: 0,
    perPercent: { cachedInput: 1_000, uncachedInput: 100, output: 10 },
    foldRange: {
      cachedInput: { min: 500, max: 2_000 },
      uncachedInput: { min: 50, max: 200 },
      output: { min: 5, max: 20 },
    },
    pattern: {
      days: 14,
      windows: 20,
      requests: { median: 10, p90: 20 },
      misses: { median: 1, p90: 2 },
    },
    composition: null,
    sessions: [],
  };
  // Claude 환율이 판정을 통과하기 전(이 Mac 2026-11-11: R² −0.02, 구간 18개).
  const collecting: CodexRate = {
    ...codex,
    status: "unreliable",
    r2: -0.02,
    windows: 18,
    perPercent: null,
    foldRange: null,
  };
  const rates = (rate: CodexRate, claude: CodexRate = collecting): RateSnapshot => ({
    computedAt: COMPUTED_AT,
    codex: rate,
    kiro: [],
    claude,
  });
  const context = (agent: "codex" | "claude"): ContextSnapshot => ({
    computedAt: COMPUTED_AT,
    agents: [
      {
        agent,
        items: [
          item("AGENTS.md", 100, { kind: "instructions" }),
          item("skill-a", 50, { kind: "skills", loading: "conditional" }),
        ],
        totalStartupTokens: 100,
        firstRequestTokens: 300,
        firstRequestChars: null,
        model: null,
        contextWindow: null,
        warnings: [],
      },
    ],
  });

  function renderTab(agent: "codex" | "claude", rate: CodexRate, claude?: CodexRate) {
    mockIPC((cmd) => {
      if (cmd === "get_context") return context(agent);
      if (cmd === "get_rates") return rates(rate, claude);
      return undefined;
    });
    render(
      <>
        <RateLine agent={agent} breakdownDays={14} />
        <ContextList agent={agent} charsPerToken={DEFAULT_TUNING.bytesPerToken} />
      </>,
    );
  }

  it("adds the estimated limit share to the description and, once unfolded, the range and the uncounted remainder", async () => {
    renderTab("codex", codex);
    await screen.findByText("요청마다 다시 실려서 5시간 한도의 ~1.90%");
    const group = screen.getByRole("group", { name: "시작 컨텍스트" });
    // 행에는 한도 몫이 붙지 않는다(컨텍스트 창을 모르면 값 칸은 토큰 하나).
    const rows = within(group).getAllByRole("listitem");
    expect(rows[0].textContent).toBe("AGENTS.md~100");
    expect(within(group).queryByText(/한도 몫: 보통/)).toBeNull();

    fireEvent.click(within(group).getByRole("button", { name: "1개 더 보기" }));
    // 범위: 보통 구간 × fold max = 100×9/2000 + 100×1/200 = 0.95, 바쁠 때 구간 × fold min = 100×18/500 + 100×2/50 = 7.6
    expect(
      within(group).getByText("한도 몫: 보통 ~1.90% · 범위 ~0.95–7.60% (바쁠 때 포함)"),
    ).toBeTruthy();
    expect(within(group).getByText(/내장 도구·첫 메시지 ~200 토큰/)).toBeTruthy();
    // 필요할 때만 실리는 항목은 펼쳐야 줄로 보인다.
    expect(within(group).getByText("skill-a").parentElement!.textContent).toBe("skill-a~50");
  });

  it.each([
    ["the rate is not estimated", { ...codex, status: "range_only", perPercent: null } as const],
    ["there is no window pattern", { ...codex, pattern: null }],
  ])("shows no share when %s", async (_, rate: CodexRate) => {
    renderTab("codex", rate);
    await screen.findByText("~100 토큰");
    const group = screen.getByRole("group", { name: "시작 컨텍스트" });

    await screen.findByText(/1%당 토큰/);
    expect(within(group).queryByText(/요청마다 다시 실려서/)).toBeNull();
    expect(within(group).queryByText(/내장 도구/)).toBeNull();
  });

  it("shows no share for Claude while its rate is being collected, even with an estimated Codex rate", async () => {
    renderTab("claude", codex);
    // 측정된 항목이 없는 Claude 합계는 합계처럼 보이지 않게 표시한다.
    await screen.findByText("~100 토큰 (예상만, 측정할 Claude 세션 없음)");
    const group = screen.getByRole("group", { name: "시작 컨텍스트" });

    await screen.findByText("아직 알 수 없어요");
    expect(within(group).queryByText(/요청마다 다시 실려서/)).toBeNull();
    expect(within(group).queryByText(/~1.90%/)).toBeNull();
  });

  it("says nothing is measured yet when a Claude context has only estimated items", async () => {
    renderTab("claude", codex);
    await screen.findByText("예상한 값 100 토큰 (측정한 값 아직 없음)");
    expect(screen.queryByText(/측정한 값 0 토큰/)).toBeNull();
  });

  it("adds the same estimated limit share to Claude once the Claude rate is estimated", async () => {
    renderTab("claude", collecting, codex);
    const line = await screen.findByText("요청마다 다시 실려서 5시간 한도의 ~1.90%");
    expect(line.getAttribute("title")).toBe("예상: 기록으로 계산한 값");
  });
});
