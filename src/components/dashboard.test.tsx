import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Dashboard } from "@/components/dashboard";
import { healthStore } from "@/components/health-compare";
import { DASHBOARD_TAB_EVENT, initialDashboardTab, type Settings } from "@/lib/ipc";
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

    await act(() => emit(DASHBOARD_TAB_EVENT, "usage"));
    await waitFor(() => expect(isSelected("사용량")).toBe(true));
    await act(() => emit(DASHBOARD_TAB_EVENT, "checks"));
    await waitFor(() => expect(isSelected("점검")).toBe(true));
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

  it("shows the request count, cache hit rate and recent window bars moved off the tray", async () => {
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
