import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";
import { describe, expect, it, vi } from "vitest";

import { isLow, remainingPercent, UsagePopover } from "@/components/usage-popover";
import {
  AGENTS,
  USAGE_UPDATED_EVENT,
  type ContextItem,
  type ContextSnapshot,
  type LimitWindow,
  type UsageSnapshot,
} from "@/lib/ipc";
import { ratesStore } from "@/lib/snapshot-store";
import { extrasUsage } from "@/test/preview/extras-usage";
import rateFixture from "@/test/fixtures/ipc/rate-snapshot.json";

const HOUR_MS = 3_600_000;

function snapshot(claudePercent: number): UsageSnapshot {
  const now = Date.now();
  return {
    agents: [
      {
        agent: "claude",
        source: "log",
        observedAt: new Date(now - 5 * 60_000).toISOString(),
        windows: [
          {
            id: "5h",
            usedPercent: claudePercent,
            resetsAt: new Date(now + 2 * HOUR_MS).toISOString(),
            resetPassed: false,
          },
        ],
        reason: "HTTP 401",
        reasonKind: "failure",
        checkedAt: new Date(now).toISOString(),
      },
      {
        agent: "codex",
        source: null,
        observedAt: null,
        windows: [],
        reason: "네트워크 오류",
        reasonKind: "failure",
        checkedAt: new Date(now).toISOString(),
      },
      {
        agent: "kiro",
        source: null,
        observedAt: null,
        windows: [],
        reason: null,
        reasonKind: null,
        checkedAt: null,
      },
    ],
  };
}

const BAR_START = "2026-11-05T05:00:00Z";

/** 에이전트 탭은 지표 3줄에 쓰는 세션 시작 토큰(`get_context`)·환율(`get_rates`)·지표(`get_metrics`)도 부른다. */
function mockCommands(cmd: string) {
  if (cmd === "get_usage") return snapshot(42);
  if (cmd === "get_context") return { computedAt: new Date().toISOString(), agents: [] };
  if (cmd === "get_metrics") {
    return {
      computedAt: new Date().toISOString(),
      agents: [
        {
          agent: "claude",
          recentRequests: 42,
          cacheHitRatio: 0.92,
          barUnit: "percent",
          bars: [
            { startsAt: "2026-11-05T00:00:00Z", value: 31, isCurrent: false },
            { startsAt: BAR_START, value: 12, isCurrent: true },
          ],
        },
        { agent: "kiro", recentRequests: 3, cacheHitRatio: null, barUnit: "credits", bars: [] },
      ],
    };
  }
  if (cmd === "get_rates") {
    return {
      computedAt: new Date().toISOString(),
      codex: {
        status: "no_data",
        r2: null,
        windows: 0,
        excludedWindows: 0,
        perPercent: null,
        foldRange: null,
        pattern: null,
        composition: null,
        sessions: [],
      },
      kiro: [],
      claude: {
        status: "no_data",
        r2: null,
        windows: 0,
        excludedWindows: 0,
        perPercent: null,
        foldRange: null,
        pattern: null,
        composition: null,
        sessions: [],
      },
    };
  }
  return undefined;
}

const startupItem = (
  kind: ContextItem["kind"],
  tokens: number,
  source: ContextItem["source"] = "measured",
): ContextItem => ({ kind, name: kind, bytes: tokens * 4, tokens, source, loading: "startup" });

/** 이 Mac 2026-11-11 Claude 실측과 같은 크기(합 34,173). 지시 파일은 예상 표시를 보려고 estimated. */
const startupContexts: ContextSnapshot = {
  computedAt: new Date().toISOString(),
  agents: [
    {
      agent: "claude",
      items: [
        startupItem("system", 3_325),
        startupItem("tools", 17_132),
        startupItem("mcpInstructions", 2_352),
        startupItem("agents", 342),
        startupItem("rules", 2_301, "estimated"),
        startupItem("instructions", 1_545),
        startupItem("skills", 6_125),
        startupItem("plugins", 1_051),
        { ...startupItem("agents", 1_692), loading: "conditional" },
      ],
      totalStartupTokens: 34_173,
      firstRequestTokens: null,
      firstRequestChars: null,
      model: "claude-opus-5-5",
      contextWindow: 400_000,
      warnings: [],
    },
    {
      agent: "kiro",
      items: [startupItem("skills", 4_107, "estimated")],
      totalStartupTokens: 4_107,
      firstRequestTokens: null,
      firstRequestChars: null,
      model: null,
      contextWindow: null,
      warnings: [],
    },
  ],
};

describe("UsagePopover", () => {
  it("shows only the selected agent as remaining percent and updates on usage-updated", async () => {
    mockIPC(mockCommands, { shouldMockEvents: true });
    const onSelect = vi.fn();
    render(
      <UsagePopover
        agents={[...AGENTS]}
        selected="claude"
        onSelect={onSelect}
        lowPercent={20}
        onOpenDashboard={() => {}}
      />,
    );

    const claude = await screen.findByRole("group", { name: "Claude" });
    expect(within(claude).getByText("5시간 58% 남음")).toBeTruthy();
    expect(within(claude).getByText("5m 전 업데이트 · 기록 파일")).toBeTruthy();
    expect(within(claude).getByText(/(1h 59m|2h 0m) 후 초기화/)).toBeTruthy();
    expect(within(claude).getByText("HTTP 401")).toBeTruthy();
    expect(screen.queryByText("네트워크 오류")).toBeNull();

    fireEvent.click(screen.getByRole("tab", { name: "Codex" }));
    expect(onSelect).toHaveBeenCalledWith("codex");

    await act(() => emit(USAGE_UPDATED_EVENT, snapshot(90)));
    expect(await within(claude).findByText("5시간 10% 남음")).toBeTruthy();
  });

  it("shows model weekly limits, extra usage and reset passes", async () => {
    const extras = (cmd: string) =>
      cmd === "get_usage" ? extrasUsage(Date.now()) : (mockCommands(cmd) ?? null);
    mockIPC(extras, { shouldMockEvents: true });
    const { unmount } = render(
      <UsagePopover
        agents={[...AGENTS]}
        selected="claude"
        onSelect={() => {}}
        lowPercent={20}
        onOpenDashboard={() => {}}
      />,
    );
    const claude = await screen.findByRole("group", { name: "Claude" });
    expect(within(claude).getByText("Fable 주간")).toBeTruthy();
    expect(within(claude).getByRole("progressbar", { name: "Fable 주간 남은 비율" })).toBeTruthy();
    expect(within(claude).getByText(/추가 사용량 (US)?\$12\.50 \/ (US)?\$100\.00/)).toBeTruthy();
    expect(within(claude).queryByText(/초기화권/)).toBeNull();
    unmount();

    render(
      <UsagePopover
        agents={[...AGENTS]}
        selected="codex"
        onSelect={() => {}}
        lowPercent={20}
        onOpenDashboard={() => {}}
      />,
    );
    const codex = await screen.findByRole("group", { name: "Codex" });
    expect(within(codex).getByText("한도 초기화권 2개")).toBeTruthy();
    expect(within(codex).getByText("GPT-5.3-Codex-Spark 주간")).toBeTruthy();
  });

  it("shows a setup step in muted text without an alert and a real failure as a red alert", async () => {
    mockIPC(mockCommands, { shouldMockEvents: true });
    render(
      <UsagePopover
        agents={[...AGENTS]}
        selected="codex"
        onSelect={() => {}}
        lowPercent={20}
        onOpenDashboard={() => {}}
      />,
    );
    const codex = await screen.findByRole("group", { name: "Codex" });
    const failure = within(codex).getByRole("alert");
    expect(failure.textContent).toBe("네트워크 오류");
    expect(failure.className).toContain("text-destructive");

    const sign = "Codex isn't installed.";
    const withCodex = (reason: string, reasonKind: "setup" | "failure"): UsageSnapshot => {
      const base = snapshot(42);
      return {
        agents: base.agents.map((a) => (a.agent === "codex" ? { ...a, reason, reasonKind } : a)),
      };
    };
    await act(() => emit(USAGE_UPDATED_EVENT, withCodex(sign, "setup")));
    const setup = await within(codex).findByText(sign);
    expect(within(codex).queryByRole("alert")).toBeNull();
    expect(setup.closest("p")?.className).toContain("text-muted-foreground");
    expect(setup.closest("p")?.className).not.toContain("text-destructive");
    expect(setup.closest("p")?.querySelector("svg")).not.toBeNull();

    await act(() => emit(USAGE_UPDATED_EVENT, withCodex("Can't reach Codex.", "failure")));
    expect((await within(codex).findByRole("alert")).textContent).toBe("Can't reach Codex.");
  });

  it("shows tabs only for overview and the enabled agents", async () => {
    mockIPC(mockCommands, { shouldMockEvents: true });
    render(
      <UsagePopover
        agents={["claude", "kiro"]}
        selected="overview"
        onSelect={() => {}}
        lowPercent={20}
        onOpenDashboard={() => {}}
      />,
    );

    const tabs = await screen.findAllByRole("tab");
    expect(tabs.map((tab) => tab.textContent)).toEqual(["개요", "Claude", "Kiro"]);
    expect(screen.getByRole("tab", { name: "개요" }).getAttribute("aria-selected")).toBe("true");
  });

  it("renders an overview summary for each enabled agent", async () => {
    mockIPC(mockCommands, { shouldMockEvents: true });
    render(
      <UsagePopover
        agents={["claude", "kiro"]}
        selected="overview"
        onSelect={() => {}}
        lowPercent={20}
        onOpenDashboard={() => {}}
      />,
    );

    const claude = await screen.findByRole("group", { name: "Claude" });
    expect(within(claude).getByText("5m 전 · 기록 파일")).toBeTruthy();
    const dial = within(claude).getByRole("img", { name: "58% 남음" });
    expect(dial.textContent).toBe("58%남음");
    const limit = within(claude).getByText("5시간 한도");
    expect(limit.className).toContain("text-foreground");
    expect(limit.parentElement?.textContent).toMatch(/^5시간 한도 · (1h 59m|2h 0m) 후 초기화$/);
    expect(
      within(screen.getByRole("group", { name: "Kiro" })).getByText("불러오는 중…"),
    ).toBeTruthy();
    expect(screen.queryByRole("group", { name: "Codex" })).toBeNull();
  });

  it("names the weekly limit when the dial shows the weekly window", async () => {
    const now = Date.now();
    const weeklyFirst = snapshot(10);
    weeklyFirst.agents[0].windows.push({
      id: "7d",
      usedPercent: 43,
      resetsAt: new Date(now + 82 * HOUR_MS).toISOString(),
      resetPassed: false,
    });
    mockIPC((cmd) => (cmd === "get_usage" ? weeklyFirst : mockCommands(cmd)), {
      shouldMockEvents: true,
    });
    render(
      <UsagePopover
        agents={["claude"]}
        selected="overview"
        onSelect={() => {}}
        lowPercent={20}
        onOpenDashboard={() => {}}
      />,
    );

    const claude = await screen.findByRole("group", { name: "Claude" });
    expect(within(claude).getByRole("img", { name: "57% 남음" }).textContent).toBe("57%남음");
    expect(within(claude).getByText("7일 한도").parentElement?.textContent).toMatch(
      /^7일 한도 · 3d (9h 59m|10h) 후 초기화$/,
    );
    // 다른 창(5시간)은 막대 줄: 이름 · 막대 · 다이얼처럼 "남음"을 붙인 값과 남은 시간.
    expect(within(claude).getByText("5시간")).toBeTruthy();
    expect(
      within(claude)
        .getByText(/^90% 남음 · \d/)
        .getAttribute("title"),
    ).toMatch(/후 초기화$/);
  });

  it("shows startup context as a share of the window on the overview, tokens only when the window is unknown", async () => {
    mockIPC((cmd) => (cmd === "get_context" ? startupContexts : mockCommands(cmd)), {
      shouldMockEvents: true,
    });
    render(
      <UsagePopover
        agents={["claude", "kiro"]}
        selected="overview"
        onSelect={() => {}}
        lowPercent={20}
        onOpenDashboard={() => {}}
      />,
    );

    const claude = await screen.findByRole("group", { name: "Claude" });
    const line = await within(claude).findByTitle(/바이트 ÷ 4/);
    expect(line.textContent).toBe("시작 컨텍스트 · ~8.5% (~34.2K / 400K 토큰)");
    expect(line.getAttribute("title")).toBe(
      "첫 메시지 전에 기본으로 실리는 지시문·도구 설명이에요.\n예상: 기록으로 계산한 값 · 토큰 = 바이트 ÷ 4 · claude-opus-5-5 컨텍스트 창 400K 토큰",
    );
    const kiro = screen.getByRole("group", { name: "Kiro" });
    expect(within(kiro).getByTitle(/바이트 ÷ 4/).textContent).toBe("시작 컨텍스트 · ~4.1K 토큰");
    expect(within(kiro).queryByText(/%/)).toBeNull();
  });

  it("shows only the startup context line in the agent tab, details stay on the dashboard", async () => {
    mockIPC((cmd) => (cmd === "get_context" ? startupContexts : mockCommands(cmd)), {
      shouldMockEvents: true,
    });
    render(
      <UsagePopover
        agents={[...AGENTS]}
        selected="claude"
        onSelect={() => {}}
        lowPercent={20}
        onOpenDashboard={() => {}}
      />,
    );

    const value = await screen.findByTitle(/바이트 ÷ 4/);
    expect(value.textContent).toBe("~8.5% (~34.2K / 400K 토큰)");
    expect(value.parentElement?.textContent).toBe("시작 컨텍스트~8.5% (~34.2K / 400K 토큰)");
    // 트레이에서 뺀 것: 구성 목록·한도 몫 문장·요청 수·캐시(대시보드에 있음, 0023).
    expect(screen.queryByRole("list", { name: "시작 컨텍스트 구성" })).toBeNull();
    expect(screen.queryByText(/요청마다 다시 실려서/)).toBeNull();
    expect(screen.queryByRole("list", { name: "지표" })).toBeNull();
    expect(document.querySelector("details")).toBeNull();
  });

  it("keeps the recent window bars in the agent tab and shows a bar's value while the cursor is on it", async () => {
    mockIPC(mockCommands, { shouldMockEvents: true });
    render(
      <UsagePopover
        agents={[...AGENTS]}
        selected="claude"
        onSelect={() => {}}
        lowPercent={20}
        onOpenDashboard={() => {}}
      />,
    );
    const bars = await screen.findByRole("list", { name: "최근 5시간 구간 사용량" });
    const items = within(bars).getAllByRole("listitem");
    expect(items).toHaveLength(2);
    expect(items[1].getAttribute("aria-current")).toBe("true");
    expect(screen.getByText("최고 31%")).toBeTruthy();

    fireEvent.pointerEnter(items[1]);
    expect(screen.getByText(/^\d\d\/\d\d \d\d:\d\d 시작 · 12%$/)).toBeTruthy();
    expect(screen.queryByText("최고 31%")).toBeNull();
    fireEvent.pointerLeave(bars);
    expect(screen.getByText("최고 31%")).toBeTruthy();
  });

  it("keeps behavior lists off the tray even when sessions and Kiro credits are known", async () => {
    mockIPC((cmd) => (cmd === "get_rates" ? rateFixture : mockCommands(cmd)), {
      shouldMockEvents: true,
    });
    ratesStore.refresh();
    await vi.waitFor(() => expect(ratesStore.getSnapshot().value?.codex.sessions).toHaveLength(1));
    const props = {
      agents: [...AGENTS],
      onSelect: () => {},
      lowPercent: 20,
      onOpenDashboard: () => {},
    };
    const { rerender } = render(<UsagePopover {...props} selected="codex" />);
    await screen.findByRole("group", { name: "Codex" });
    expect(screen.queryByRole("list", { name: "많이 쓴 세션" })).toBeNull();
    expect(screen.queryByText("5시간 한도는 보통 무엇으로 찼나")).toBeNull();
    rerender(<UsagePopover {...props} selected="kiro" />);
    await screen.findByRole("group", { name: "Kiro" });
    expect(screen.queryByText("질문당 크레딧")).toBeNull();
  });

  it("colors a bar as low when remaining is at or below lowPercent", async () => {
    mockIPC(mockCommands, { shouldMockEvents: true });
    const props = { agents: [...AGENTS], onSelect: () => {}, onOpenDashboard: () => {} };
    // Claude: 42% 사용 → 58% 남음.
    const { rerender } = render(
      <UsagePopover {...props} selected="claude" lowPercent={20} onOpenDashboard={() => {}} />,
    );
    const claude = await screen.findByRole("group", { name: "Claude" });
    const bar = () => within(claude).getByRole("progressbar");
    expect(bar().querySelector(".bg-destructive")).toBeNull();

    rerender(<UsagePopover {...props} selected="claude" lowPercent={60} />);
    expect(bar().querySelector(".bg-destructive")).not.toBeNull();
  });

  it("shows the rejection reason as an alert instead of an endless loading state when get_usage fails", async () => {
    const message = "사용량을 읽지 못했어요.";
    mockIPC(
      (cmd) =>
        cmd === "get_usage" ? Promise.reject({ kind: "internal", message }) : mockCommands(cmd),
      { shouldMockEvents: true },
    );
    render(
      <UsagePopover
        agents={[...AGENTS]}
        selected="claude"
        onSelect={() => {}}
        lowPercent={20}
        onOpenDashboard={() => {}}
      />,
    );
    expect((await screen.findByRole("alert")).textContent).toBe(message);
    expect(screen.queryByText("불러오는 중…")).toBeNull();
  });

  it("opens the dashboard from the chevron button in the agent header", async () => {
    mockIPC(mockCommands, { shouldMockEvents: true });
    const onOpenDashboard = vi.fn();
    render(
      <UsagePopover
        agents={[...AGENTS]}
        selected="claude"
        onSelect={() => {}}
        lowPercent={20}
        onOpenDashboard={onOpenDashboard}
      />,
    );
    fireEvent.click(await screen.findByRole("button", { name: "대시보드 열기" }));
    expect(onOpenDashboard).toHaveBeenCalledTimes(1);
    expect(onOpenDashboard).toHaveBeenCalledWith("claude");
  });
});

describe("remainingPercent", () => {
  const now = Date.parse("2026-11-04T12:00:00Z");
  const window = (usedPercent: number, resetsAt: string | null): LimitWindow => ({
    id: "5h",
    usedPercent,
    resetsAt,
    resetPassed: false,
  });

  it("returns 100 minus used, clamped to 0..100", () => {
    expect(remainingPercent(window(2, "2026-11-04T13:00:00Z"), now)).toBe(98);
    expect(remainingPercent(window(130, null), now)).toBe(0);
  });

  it("returns 100 when the reset time has passed", () => {
    expect(remainingPercent(window(80, "2026-11-04T11:59:00Z"), now)).toBe(100);
    expect(remainingPercent({ ...window(80, null), resetPassed: true }, now)).toBe(100);
  });

  it("warns at lowPercent remaining or less", () => {
    expect(isLow(20, 20)).toBe(true);
    expect(isLow(21, 20)).toBe(false);
    expect(isLow(40, 50)).toBe(true);
  });
});
