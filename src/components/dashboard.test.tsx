import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Dashboard } from "@/components/dashboard";
import { healthStore } from "@/components/health-compare";
import { metricsStore } from "@/lib/snapshot-store";
import {
  DASHBOARD_TAB_EVENT,
  initialDashboardAgent,
  initialDashboardTab,
  type Settings,
} from "@/lib/ipc";
import context from "@/test/fixtures/ipc/context-snapshot.json";
import health from "@/test/fixtures/ipc/health-report.json";
import metrics from "@/test/fixtures/ipc/metrics-snapshot.json";
import rates from "@/test/fixtures/ipc/rate-snapshot.json";
import settings from "@/test/fixtures/ipc/settings.json";
import usage from "@/test/fixtures/ipc/usage-snapshot.json";
import {
  CODEX_APPLIED,
  DASHBOARD_CHECKS,
  DASHBOARD_SWITCHES,
} from "@/test/preview/checks-fixtures";

const ALL_AGENTS: Settings = {
  ...(settings as Settings),
  enabledAgents: ["claude", "codex", "kiro"],
};

const FIXTURE: Record<string, unknown> = {
  get_settings: ALL_AGENTS,
  get_usage: usage,
  get_metrics: metrics,
  get_rates: rates,
  get_context: context,
  get_health: health,
  get_checks: DASHBOARD_CHECKS,
  get_switches: DASHBOARD_SWITCHES,
  get_coach_log: [CODEX_APPLIED],
};

function mock(): string[] {
  const calls: string[] = [];
  mockIPC(
    (cmd) => {
      calls.push(cmd);
      return FIXTURE[cmd] ?? null;
    },
    { shouldMockEvents: true },
  );
  return calls;
}

const tab = (name: string) => screen.getByRole("tab", { name });
const isSelected = (name: string) => tab(name).getAttribute("aria-selected") === "true";

afterEach(() => {
  healthStore.reset();
  metricsStore.reset();
  window.history.replaceState(null, "", "/");
});

describe("Dashboard", () => {
  it("starts on the usage tab and switches to checks, which reads the checks", async () => {
    const calls = mock();
    render(<Dashboard />);
    await screen.findByRole("tablist");
    expect(isSelected("사용량")).toBe(true);
    expect(screen.getByRole("heading", { name: "처방 효과" })).toBeTruthy();
    expect(calls).not.toContain("get_checks");

    fireEvent.click(tab("점검"));
    expect(isSelected("점검")).toBe(true);
    expect(await screen.findByRole("region", { name: "Codex" })).toBeTruthy();
    await waitFor(() => expect(calls).toContain("get_checks"));
    expect(calls).toContain("get_switches");
    expect(calls).toContain("get_coach_log");
  });

  it("opens on the tab Rust put in the URL and switches when the event arrives", async () => {
    expect(initialDashboardTab("?tab=checks")).toBe("checks");
    expect(initialDashboardTab("?tab=settings")).toBeNull();
    expect(initialDashboardTab("")).toBeNull();

    window.history.replaceState(null, "", "/?tab=checks");
    mock();
    render(<Dashboard />);
    await screen.findByRole("tablist");
    expect(isSelected("점검")).toBe(true);

    await act(() => emit(DASHBOARD_TAB_EVENT, { tab: "usage", agent: null }));
    await waitFor(() => expect(isSelected("사용량")).toBe(true));
    await act(() => emit(DASHBOARD_TAB_EVENT, { tab: "checks", agent: null }));
    await waitFor(() => expect(isSelected("점검")).toBe(true));
  });

  it("reads the starting agent from the URL and falls back to all for unknown ids", () => {
    expect(initialDashboardAgent("?agent=kiro")).toBe("kiro");
    expect(initialDashboardAgent("?tab=usage&agent=codex")).toBe("codex");
    expect(initialDashboardAgent("?agent=gemini")).toBeNull();
    expect(initialDashboardAgent("?agent=")).toBeNull();
    expect(initialDashboardAgent("")).toBeNull();
  });

  const pickerTab = (name: string) => screen.getByRole("tab", { name });
  const isPicked = (name: string) => pickerTab(name).getAttribute("aria-selected") === "true";
  const cardHeading = (name: string) => !!screen.queryByRole("heading", { level: 2, name });
  const picker = () => screen.findByRole("tablist", { name: "에이전트 고르기" });

  it("shows every agent card under All and only one card under an agent tab", async () => {
    mock();
    render(<Dashboard />);
    await picker();
    expect(isPicked("전체")).toBe(true);
    for (const name of ["Claude", "Codex", "Kiro"]) expect(cardHeading(name)).toBe(true);

    fireEvent.click(pickerTab("Kiro"));
    expect(isPicked("Kiro")).toBe(true);
    expect(cardHeading("Kiro")).toBe(true);
    expect(cardHeading("Claude")).toBe(false);
    expect(cardHeading("Codex")).toBe(false);
    // 처방 효과 주간 요약도 그 에이전트 줄만 남는다.
    const health = screen.getByRole("region", { name: "처방 효과" });
    expect(within(health).queryByText("Claude")).toBeNull();

    fireEvent.click(pickerTab("전체"));
    for (const name of ["Claude", "Codex", "Kiro"]) expect(cardHeading(name)).toBe(true);
  });

  it("starts on the agent in the URL and follows the event on an open window", async () => {
    window.history.replaceState(null, "", "/?agent=codex");
    mock();
    render(<Dashboard />);
    await picker();
    expect(isPicked("Codex")).toBe(true);
    expect(cardHeading("Claude")).toBe(false);

    await act(() => emit(DASHBOARD_TAB_EVENT, { tab: null, agent: "kiro" }));
    await waitFor(() => expect(isPicked("Kiro")).toBe(true));
    expect(isSelected("사용량")).toBe(true);
    expect(cardHeading("Codex")).toBe(false);
  });

  it("falls back to All for an unknown or disabled agent and hides the picker for one agent", async () => {
    window.history.replaceState(null, "", "/?agent=gemini");
    mock();
    const first = render(<Dashboard />);
    await picker();
    expect(isPicked("전체")).toBe(true);
    expect(cardHeading("Claude") && cardHeading("Codex") && cardHeading("Kiro")).toBe(true);
    first.unmount();

    // 끈 에이전트: 켠 것은 claude·codex뿐이라 kiro 요청은 전체로 읽는다.
    window.history.replaceState(null, "", "/?agent=kiro");
    mockIPC(
      (cmd) =>
        cmd === "get_settings"
          ? { ...ALL_AGENTS, enabledAgents: ["claude", "codex"] }
          : (FIXTURE[cmd] ?? null),
      { shouldMockEvents: true },
    );
    const second = render(<Dashboard />);
    await picker();
    expect(isPicked("전체")).toBe(true);
    expect(cardHeading("Claude") && cardHeading("Codex")).toBe(true);
    expect(screen.queryByRole("tab", { name: "Kiro" })).toBeNull();
    second.unmount();

    // 하나만 켜면 고르기를 숨긴다.
    mockIPC(
      (cmd) =>
        cmd === "get_settings"
          ? { ...ALL_AGENTS, enabledAgents: ["claude"] }
          : (FIXTURE[cmd] ?? null),
      { shouldMockEvents: true },
    );
    render(<Dashboard />);
    await screen.findByRole("tablist");
    expect(screen.queryByRole("tablist", { name: "에이전트 고르기" })).toBeNull();
    expect(cardHeading("Claude")).toBe(true);
  });

  it("shows an agent's sign-in message above the daily ring, once, without a scroll position", async () => {
    const reason = "Can't read your Claude Code login. Run claude and sign in, then press Refresh.";
    const unreadable = {
      agents: [
        {
          agent: "claude",
          source: null,
          observedAt: null,
          windows: [],
          reason,
          reasonKind: "setup",
          checkedAt: "2026-11-04T12:00:00Z",
        },
      ],
    };
    mockIPC((cmd) => (cmd === "get_usage" ? unreadable : (FIXTURE[cmd] ?? null)), {
      shouldMockEvents: true,
    });
    render(<Dashboard />);
    const notice = await screen.findByText(`Claude: ${reason}`);
    // 이유는 한 번만 나오고(카드 안에 또 나오지 않고), 일일 링 카드보다 앞에 있다.
    expect(screen.getAllByText(new RegExp("Run claude and sign in"))).toHaveLength(1);
    const cards = Array.from(document.querySelectorAll("main section"));
    expect(cards[0]).toBe(notice.closest("section"));
    expect(cards[1]?.getAttribute("aria-label")).toBeTruthy();
  });

  it("guides an empty first run when there are no metrics records, even with usage windows", async () => {
    mockIPC((cmd) => (cmd === "get_metrics" ? null : (FIXTURE[cmd] ?? null)), {
      shouldMockEvents: true,
    });
    render(<Dashboard />);
    await screen.findByText(/한 번 쓰고 새로고침하면/);
  });

  it("does not repeat the empty-run guide when every agent already shows what to do", async () => {
    const unreadable = {
      agents: (usage as { agents: object[] }).agents.map((a) => ({
        ...a,
        windows: [],
        reason: "로그인하세요.",
        reasonKind: "setup",
        checkedAt: "2026-11-04T12:00:00Z",
      })),
    };
    mockIPC(
      (cmd) =>
        cmd === "get_metrics" ? null : cmd === "get_usage" ? unreadable : (FIXTURE[cmd] ?? null),
      { shouldMockEvents: true },
    );
    render(<Dashboard />);
    await screen.findByText("Claude: 로그인하세요.");
    await waitFor(() => expect(metricsStore.getSnapshot().value).not.toBeUndefined());
    expect(screen.queryByText(/한 번 쓰고 새로고침하면/)).toBeNull();
  });

  it("does not guide while usage rows are still pending (checkedAt null)", async () => {
    const pending = {
      agents: (usage as { agents: object[] }).agents.map((a) => ({
        ...a,
        windows: [],
        reason: null,
        checkedAt: null,
      })),
    };
    mockIPC(
      (cmd) =>
        cmd === "get_metrics" ? null : cmd === "get_usage" ? pending : (FIXTURE[cmd] ?? null),
      { shouldMockEvents: true },
    );
    render(<Dashboard />);
    await screen.findByRole("region", { name: "오늘 쓴 양" });
    expect(screen.queryByText(/한 번 쓰고 새로고침하면/)).toBeNull();
  });

  it("hides the empty-run guide once metrics records exist", async () => {
    mock();
    render(<Dashboard />);
    await screen.findByRole("region", { name: "오늘 쓴 양" });
    expect(screen.queryByText(/한 번 쓰고 새로고침하면/)).toBeNull();
  });

  it("shows the rejection reason as an alert and keeps the dashboard when refresh_all fails", async () => {
    const message = "새로고침을 시작하지 못했어요.";
    mockIPC(
      (cmd) =>
        cmd === "refresh_all"
          ? Promise.reject({ kind: "internal", message })
          : (FIXTURE[cmd] ?? null),
      { shouldMockEvents: true },
    );
    render(<Dashboard />);
    await screen.findByRole("tab", { name: "사용량" });
    fireEvent.click(screen.getByRole("button", { name: "새로고침" }));
    expect((await screen.findByRole("alert")).textContent).toBe(message);
    expect(screen.getByRole("tab", { name: "사용량" })).toBeTruthy();
  });

  it("shows the no-data text instead of 0 % on a day with no usage at all", async () => {
    mockIPC((cmd) => (cmd === "get_metrics" ? null : (FIXTURE[cmd] ?? null)), {
      shouldMockEvents: true,
    });
    render(<Dashboard />);
    const card = await screen.findByRole("region", { name: "오늘 쓴 양" });
    expect(within(card).getAllByText("기록 없음").length).toBeGreaterThanOrEqual(3);
    expect(within(card).queryByText("%")).toBeNull();
    expect(within(card).queryByText("0")).toBeNull();
    // 목표 줄("하루 몫 14%")은 빈 날에 보이지 않는다.
    expect(within(card).queryByText(/^하루 몫/)).toBeNull();
    expect(within(card).getByRole("img").getAttribute("aria-label")).not.toMatch(/\d/);
    // 시간대 카드는 빈 축 대신 조용한 한 줄만 보인다(에이전트 이름을 모아서).
    const hourly = screen.getByRole("region", { name: "시간대별 요청" });
    expect(within(hourly).getAllByText(/ · 이날 기록 없음$/)).toHaveLength(1);
    expect(within(hourly).queryByRole("heading")).toBeNull();
    expect(within(hourly).queryByRole("img")).toBeNull();
  });

  it("names the daily share with its unit under each ring", async () => {
    // 지표 픽스처의 기록이 있는 날(2026-11-04 UTC 10~11시)을 오늘로 둔다.
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date("2026-11-04T14:00:00Z"));
    try {
      mock();
      render(<Dashboard />);
      const card = await screen.findByRole("region", { name: "오늘 쓴 양" });
      expect(within(card).getAllByText(/^하루 몫 [\d.,]+%$/).length).toBeGreaterThan(0);
      // 쓴 양의 분모(주간 한도)는 숫자 아래 단위 줄이 말한다. 크레딧(Kiro)은 그대로.
      expect(within(card).getAllByText("% (주간 한도)").length).toBeGreaterThan(0);
    } finally {
      vi.useRealTimers();
    }
  });

  it("shows a dash, not 0 %, for an agent whose limit was never read", async () => {
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date("2026-11-04T14:00:00Z"));
    try {
      // Claude는 요청이 있지만 한도 표본이 없다(막대 0개, 쓴 양 0).
      const unread = {
        ...metrics,
        agents: metrics.agents.map((a) =>
          a.agent === "claude"
            ? {
                ...a,
                bars: [],
                hourly: [{ startsAt: "2026-11-04T11:00:00Z", requests: 3, used: 0 }],
              }
            : a,
        ),
      };
      mockIPC((cmd) => (cmd === "get_metrics" ? unread : (FIXTURE[cmd] ?? null)), {
        shouldMockEvents: true,
      });
      render(<Dashboard />);
      const card = await screen.findByRole("region", { name: "오늘 쓴 양" });
      expect(within(card).getByText("—")).toBeTruthy();
      expect(within(card).getByText("한도 못 읽음")).toBeTruthy();
      expect(within(card).getByRole("img").getAttribute("aria-label")).toMatch(
        /Claude 한도 못 읽음/,
      );
      // 읽은 Codex는 그대로 숫자와 하루 몫을 보인다.
      expect(within(card).getByText("1.5")).toBeTruthy();
    } finally {
      vi.useRealTimers();
    }
  });

  it("shows pending, not a confident 0 %, when requests exist but no limit sample rose", async () => {
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date("2026-11-04T14:00:00Z"));
    try {
      // Claude: 막대(표본)는 있고 요청은 908회인데 오늘 오른 %p가 없다(0015: 증가분은 표본에서만 나온다).
      const pending = {
        ...metrics,
        agents: metrics.agents.map((a) =>
          a.agent === "claude"
            ? {
                ...a,
                bars: metrics.agents.find((o) => o.agent === "codex")?.bars ?? [],
                hourly: [{ startsAt: "2026-11-04T11:00:00Z", requests: 908, used: 0 }],
              }
            : a,
        ),
      };
      mockIPC((cmd) => (cmd === "get_metrics" ? pending : (FIXTURE[cmd] ?? null)), {
        shouldMockEvents: true,
      });
      render(<Dashboard />);
      const card = await screen.findByRole("region", { name: "오늘 쓴 양" });
      expect(within(card).getByText("집계 전")).toBeTruthy();
      expect(within(card).getByRole("img").getAttribute("aria-label")).toMatch(/Claude 집계 전/);
      expect(within(card).queryByText("0")).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });

  it("shows a confirmed 0 for a past day with requests, since pending only applies to today", async () => {
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date("2026-11-05T14:00:00Z"));
    try {
      const pastDay = {
        ...metrics,
        agents: metrics.agents.map((a) =>
          a.agent === "claude"
            ? {
                ...a,
                bars: metrics.agents.find((o) => o.agent === "codex")?.bars ?? [],
                hourly: [{ startsAt: "2026-11-04T11:00:00Z", requests: 908, used: 0 }],
              }
            : a,
        ),
      };
      mockIPC((cmd) => (cmd === "get_metrics" ? pastDay : (FIXTURE[cmd] ?? null)), {
        shouldMockEvents: true,
      });
      render(<Dashboard />);
      fireEvent.click(await screen.findByRole("button", { name: "이전 날" }));
      const card = await screen.findByRole("region", { name: "오늘 쓴 양" });
      expect(within(card).queryByText("집계 전")).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });

  it("effect button in diagnosis goes to the usage tab and scrolls to prescription effects", async () => {
    mock();
    const scrolled: Element[] = [];
    const spy = vi.spyOn(Element.prototype, "scrollIntoView").mockImplementation(function (
      this: Element,
    ) {
      scrolled.push(this);
    });
    render(<Dashboard />);
    await screen.findByRole("tablist");
    fireEvent.click(tab("점검"));
    const codex = await screen.findByRole("region", { name: "Codex" });
    const recent = await within(codex).findByRole("group", { name: "최근 바꾼 것" });
    fireEvent.click(within(recent).getByRole("button", { name: "효과 보기" }));
    await waitFor(() => expect(isSelected("사용량")).toBe(true));
    expect(scrolled).toEqual([screen.getByRole("heading", { name: "처방 효과" })]);
    spy.mockRestore();
  });

  it("shows the behavior blocks moved off the tray in the agent cards", async () => {
    mock();
    render(<Dashboard />);
    // 트레이에서 옮긴 Codex 구성 막대·많이 쓴 세션, Kiro 질문당 크레딧(0019 D-06).
    const codex = await screen.findByRole("region", { name: "Codex" });
    expect(await within(codex).findByRole("group", { name: "많이 쓴 세션" })).toBeTruthy();
    const kiro = screen.getByRole("region", { name: "Kiro" });
    expect(await within(kiro).findByRole("group", { name: "질문당 크레딧" })).toBeTruthy();
  });

  it("shows the request count and cache hit rate moved off the tray, beside the window bars", async () => {
    mock();
    render(<Dashboard />);
    // 트레이 에이전트 탭에서 뺀 값(0023). Codex 픽스처에 값이 있다.
    const codex = await screen.findByRole("region", { name: "Codex" });
    const lines = await within(codex).findByRole("list", { name: "지표" });
    expect([...lines.querySelectorAll("li")].map((li) => li.textContent)).toEqual([
      "최근 5시간 요청42회",
      "캐시 적중률92%",
    ]);
    expect(within(codex).getByRole("list", { name: "최근 5시간 구간 사용량" })).toBeTruthy();
    // 분모가 보이게: 캐시 적중률은 전체 입력 중 비율, 구간 막대 제목은 5시간 한도 중 비율.
    expect(within(codex).getByText("92%").closest("li")?.getAttribute("title")).toBe(
      "정확: 캐시에서 읽은 입력 ÷ 전체 입력, 현재 5시간 구간",
    );
    expect(within(codex).getByText("최근 5시간 구간").getAttribute("title")).toBe(
      "구간마다 5시간 한도에서 쓴 비율",
    );
  });

  it("shows the startup context once per agent, not also as a metric card", async () => {
    mock();
    render(<Dashboard />);
    const claude = await screen.findByRole("region", { name: "Claude" });
    // 창 몫 값 줄(`startupValue`)은 오른쪽 `ContextList`에만 한 번 보인다.
    expect(await within(claude).findAllByText(/0\.56% \(/)).toHaveLength(1);
    expect(within(claude).getAllByRole("group", { name: "시작 컨텍스트" })).toHaveLength(1);
    expect(within(claude).queryByRole("list", { name: "시작 컨텍스트 구성" })).toBeNull();
  });
});
