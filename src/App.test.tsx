import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { mockIPC } from "@tauri-apps/api/mocks";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "@/App";
import type { CoachCheck, CoachSwitch, Settings } from "@/lib/ipc";
import settings from "@/test/fixtures/ipc/settings.json";
import usage from "@/test/fixtures/ipc/usage-snapshot.json";
import { ALL_HIDDEN, EFFORT_CHECK, THIS_MAC_CHECKS } from "@/test/preview/checks-fixtures";
import { DEFAULT_TUNING } from "@/test/tuning";

type Calls = { cmd: string; args: unknown }[];

// 계약 fixture는 상태 견본으로 Claude를 두 번 싣는다(`pending`). 실제 앱처럼 에이전트마다 하나만 둔다
// (같은 값의 탭 두 개는 탭 전환이 무한 갱신된다).
const ONE_PER_AGENT = {
  agents: usage.agents.filter((a, i, all) => all.findIndex((b) => b.agent === a.agent) === i),
};

function mock(checks: CoachCheck[], checkAlertMin = 1, switches: CoachSwitch[] = []): Calls {
  const calls: Calls = [];
  const current: Settings = {
    ...(settings as Settings),
    enabledAgents: ["claude", "codex", "kiro"],
    tuning: { ...DEFAULT_TUNING, checkAlertMin },
  };
  mockIPC(
    (cmd, args) => {
      calls.push({ cmd, args });
      if (cmd === "get_settings") return current;
      if (cmd === "get_usage") return ONE_PER_AGENT;
      if (cmd === "get_checks") return checks;
      if (cmd === "get_switches") return switches;
      return null;
    },
    { shouldMockEvents: true },
  );
  return calls;
}

const menu = () => screen.getByRole("navigation", { name: "메뉴" });
/** 사용량 탭까지 그려진 뒤(이벤트 구독이 끝난 뒤) "점검" 줄. */
async function checksItem() {
  await screen.findByRole("tablist");
  return within(menu()).getByRole("button", { name: /^점검/ });
}

beforeEach(() => {
  // jsdom에는 ResizeObserver가 없다(팝오버 높이 맞추기).
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      disconnect() {}
    },
  );
});

describe("popover menu", () => {
  it("has one 점검 row instead of 진단하기·최적화, with a red dot and count at the threshold", async () => {
    mock([EFFORT_CHECK, { ...EFFORT_CHECK, id: "D-claude-always-thinking" }, THIS_MAC_CHECKS[1]]);
    render(<App />);
    const item = await checksItem();
    expect(await within(item).findByRole("img", { name: "점검할 것 2개" })).toBeTruthy();
    expect(within(item).getByText("2")).toBeTruthy();
    expect(within(menu()).queryByRole("button", { name: /진단하기|최적화/ })).toBeNull();
  });

  it("shows no dot below the threshold or when only keep and hidden cards remain", async () => {
    const calls = mock(THIS_MAC_CHECKS, 2);
    const { unmount } = render(<App />);
    const item = await checksItem();
    await waitFor(() => expect(calls.some((c) => c.cmd === "get_checks")).toBe(true));
    expect(within(item).queryByRole("img")).toBeNull();
    unmount();

    mock(ALL_HIDDEN);
    render(<App />);
    const next = await checksItem();
    expect(within(next).queryByRole("img")).toBeNull();
    expect(next.textContent).toBe("점검");
  });

  it("counts every enabled agent on 개요 and only that agent's cards on an agent tab", async () => {
    mock(THIS_MAC_CHECKS);
    render(<App />);
    const item = await checksItem();
    await within(item).findByRole("img", { name: "점검할 것 1개" });
    fireEvent.click(screen.getByRole("tab", { name: "Kiro" }));
    await waitFor(() => expect(within(item).queryByRole("img")).toBeNull());
    fireEvent.click(screen.getByRole("tab", { name: "Codex" }));
    expect(within(item).queryByRole("img")).toBeNull();
    fireEvent.click(screen.getByRole("tab", { name: "Claude" }));
    expect(await within(item).findByRole("img", { name: "점검할 것 1개" })).toBeTruthy();
  });

  it("reads the checks again when the popover gets focus", async () => {
    const calls = mock(THIS_MAC_CHECKS);
    render(<App />);
    await within(await checksItem()).findByRole("img", { name: "점검할 것 1개" });
    const reads = () => calls.filter((c) => c.cmd === "get_checks").length;
    expect(reads()).toBe(1);
    act(() => {
      window.dispatchEvent(new Event("focus"));
    });
    await waitFor(() => expect(reads()).toBe(2));
  });

  it("opens the dashboard on the checks tab", async () => {
    const calls = mock(THIS_MAC_CHECKS);
    render(<App />);
    fireEvent.click(await checksItem());
    await waitFor(() =>
      expect(calls.find((c) => c.cmd === "open_dashboard")?.args).toEqual({ tab: "checks" }),
    );
    fireEvent.click(within(menu()).getByRole("button", { name: "대시보드" }));
    await waitFor(() =>
      expect(calls.filter((c) => c.cmd === "open_dashboard").map((c) => c.args)).toEqual([
        { tab: "checks" },
        {},
      ]),
    );
  });
});

const suggested = (agent: CoachSwitch["agent"], name: string, tokens: number | null) =>
  ({
    target: `${agent}/plugin:${name}`,
    agent,
    kind: "plugin",
    name,
    enabled: true,
    tokens,
    loading: "startup",
    isProposed: true,
    usage: null,
    scope: null,
    scopeMove: null,
    contents: null,
    format: "json",
    file: "/h/settings.json",
    instruction: null,
  }) satisfies CoachSwitch;

describe("popover trim entry", () => {
  const SWITCHES: CoachSwitch[] = [
    suggested("claude", "a@m", 1200),
    {
      ...suggested("claude", "b@m", 300),
      isProposed: false,
      scopeMove: { projects: [], skipped: [] },
    },
    { ...suggested("claude", "kept@m", 9000), isProposed: false },
    suggested("codex", "c@m", 500),
  ];
  const entry = () => screen.queryByRole("button", { name: /줄일 수 있는 것/ });

  it("shows one line with the count and startup tokens on the agent tab only, and opens the checks tab", async () => {
    const calls = mock([], 1, SWITCHES);
    render(<App />);
    await screen.findByRole("tablist");
    expect(entry()).toBeNull();
    fireEvent.click(screen.getByRole("tab", { name: "Claude" }));
    const claude = await screen.findByRole("button", { name: /줄일 수 있는 것/ });
    // 제안 2개(끄기 1 + 프로젝트에서만 켜기 1), 쓰는 중인 kept@m은 세지 않는다. 1200 + 300.
    expect(claude.textContent).toBe("줄일 수 있는 것 2개 · 시작 때 ~1,500 토큰보기");
    fireEvent.click(screen.getByRole("tab", { name: "Codex" }));
    await waitFor(() => expect(entry()?.textContent).toContain("줄일 수 있는 것 1개"));
    fireEvent.click(entry() as HTMLElement);
    await waitFor(() =>
      expect(calls.find((c) => c.cmd === "open_dashboard")?.args).toEqual({ tab: "checks" }),
    );
  });

  it("hides the line when nothing is suggested for that agent, or sizes are unknown for the tokens part", async () => {
    mock([], 1, [SWITCHES[2], suggested("kiro", "k@m", null)]);
    render(<App />);
    await screen.findByRole("tablist");
    fireEvent.click(screen.getByRole("tab", { name: "Claude" }));
    await waitFor(() => expect(entry()).toBeNull());
    fireEvent.click(screen.getByRole("tab", { name: "Kiro" }));
    expect((await screen.findByRole("button", { name: /줄일 수 있는 것/ })).textContent).toBe(
      "줄일 수 있는 것 1개보기",
    );
  });
});
