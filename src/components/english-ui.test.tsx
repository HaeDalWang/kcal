// 영어 화면(docs/decisions/0021): 주요 화면을 IPC fixture(Rust가 만든 계약 견본)로 영어로 그려 한글이 한 글자도
// 없는지 본다. 글자뿐 아니라 접근성 이름·툴팁(aria-label·title)도 본다.
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { mockIPC } from "@tauri-apps/api/mocks";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "@/App";
import { Dashboard } from "@/components/dashboard";
import { healthStore } from "@/components/health-compare";
import { currentLanguage, resolveLanguage } from "@/lib/i18n";
import { en } from "@/lib/i18n-en";
import type { Settings } from "@/lib/ipc";
import checks from "@/test/fixtures/ipc/coach-checks.json";
import coachLog from "@/test/fixtures/ipc/coach-log.json";
import switches from "@/test/fixtures/ipc/coach-switches.json";
import context from "@/test/fixtures/ipc/context-snapshot.json";
import health from "@/test/fixtures/ipc/health-report.json";
import metrics from "@/test/fixtures/ipc/metrics-snapshot.json";
import rates from "@/test/fixtures/ipc/rate-snapshot.json";
import settings from "@/test/fixtures/ipc/settings.json";
import usage from "@/test/fixtures/ipc/usage-snapshot.json";

const HANGUL = /[\uAC00-\uD7A3]/;

const ENGLISH: Settings = {
  ...(settings as Settings),
  enabledAgents: ["claude", "codex", "kiro"],
  language: "en",
  resolvedLanguage: "en",
};

// 계약 fixture는 상태 견본으로 Claude를 두 번 싣는다. 실제 앱처럼 에이전트마다 하나만 둔다.
const ONE_PER_AGENT = {
  agents: usage.agents.filter((a, i, all) => all.findIndex((b) => b.agent === a.agent) === i),
};

const FIXTURE: Record<string, unknown> = {
  list_claude_sessions: { sessions: [], canStop: true, idleHours: 24 },
  get_settings: ENGLISH,
  get_usage: ONE_PER_AGENT,
  get_metrics: metrics,
  get_rates: rates,
  get_context: context,
  get_health: health,
  get_checks: checks,
  get_switches: switches,
  get_coach_log: coachLog,
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

/** 화면 글자와 aria-label·title·alt에서 한글이 든 것만 모은다. */
function hangulIn(root: HTMLElement): string[] {
  const found: string[] = [];
  if (HANGUL.test(root.textContent ?? "")) {
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      if (HANGUL.test(node.textContent ?? "")) found.push(`text: ${node.textContent}`);
    }
  }
  for (const element of root.querySelectorAll("[aria-label], [title], [alt]")) {
    for (const name of ["aria-label", "title", "alt"]) {
      const value = element.getAttribute(name);
      if (value && HANGUL.test(value)) found.push(`${name}: ${value}`);
    }
  }
  return found;
}

beforeEach(() => {
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      disconnect() {}
    },
  );
  healthStore.reset();
});

// 대시보드 전체를 그리는 두 테스트는 단독 3초 안팎이라, 기계가 바쁠 때 기본 5초를 넘는다.
const FULL_RENDER_MS = 15_000;

describe("English UI", () => {
  it("auto picks Korean only when the browser language starts with ko", () => {
    expect(resolveLanguage("auto", "ko-KR")).toBe("ko");
    expect(resolveLanguage("auto", "ko")).toBe("ko");
    expect(resolveLanguage("auto", "en-US")).toBe("en");
    expect(resolveLanguage("auto", "ja-JP")).toBe("en");
    expect(resolveLanguage("ko", "en-US")).toBe("ko");
    expect(resolveLanguage("en", "ko-KR")).toBe("en");
  });

  it("shows no Hangul on the popover overview, each agent tab and settings", async () => {
    mock();
    const { container } = render(<App />);
    await screen.findByRole("tab", { name: /Overview/ });
    expect(currentLanguage()).toBe("en");
    // 개요 카드의 시작 컨텍스트 줄까지 그려진 뒤.
    await waitFor(() => expect(container.textContent).toContain("Startup context"));
    expect(hangulIn(container)).toEqual([]);
    // 시안 K: 다이얼 숫자 아래 "left", 회색 줄 앞에 한도 이름(fixture의 Claude 다이얼은 5시간 창).
    const claude = screen.getByRole("group", { name: "Claude" });
    expect(within(claude).getByRole("img", { name: /% left$/ }).textContent).toMatch(/^\d+%left$/);
    expect(within(claude).getByText("5-hour limit")).toBeTruthy();

    for (const agent of ["Claude", "Codex", "Kiro"]) {
      fireEvent.click(screen.getByRole("tab", { name: new RegExp(agent) }));
      await screen.findByRole("button", { name: "Open dashboard" });
      await waitFor(() => expect(screen.queryAllByText("Loading…")).toHaveLength(0));
      expect(hangulIn(container), agent).toEqual([]);
      // 계약 fixture에는 Codex 끄기 제안이 있다: 영어 입구 줄(getSwitches는 열 때 읽는다).
      if (agent === "Codex") {
        await waitFor(() => expect(container.textContent).toContain("thing you could trim"));
        expect(hangulIn(container), "trim entry").toEqual([]);
      }
    }

    fireEvent.click(
      within(screen.getByRole("navigation", { name: "Menu" })).getByText("Settings…"),
    );
    await screen.findByRole("combobox");
    expect(screen.getByRole("option", { name: "한국어" })).toBeTruthy();
    // 언어 선택지의 "한국어"는 모국어 이름이라 예외.
    expect(
      hangulIn(container).filter((x) => x !== "text: 한국어"),
      "settings",
    ).toEqual([]);
  });

  it(
    "shows no Hangul on the dashboard usage and checks tabs",
    { timeout: FULL_RENDER_MS },
    async () => {
      mock();
      const { container } = render(<Dashboard />);
      await screen.findByRole("tab", { name: "Usage" });
      await screen.findByText("Change effects");
      await waitFor(() => expect(screen.queryAllByText("Loading…")).toHaveLength(0));
      expect(hangulIn(container), "usage").toEqual([]);

      fireEvent.click(screen.getByRole("tab", { name: "Checks" }));
      await screen.findAllByRole("group", { name: "Optimize" });
      await waitFor(() => expect(screen.queryAllByText("Loading…")).toHaveLength(0));
      expect(hangulIn(container), "checks").toEqual([]);

      // 진단하기: 펼친 줄(근거 칩·함께 꺼지는 것)과 "그대로 둠", 읽는 법도 영어만.
      // 탭 전체에서 펼친 줄은 하나라 줄마다 펼친 그때 본다.
      let losesShown = 0;
      for (const toggle of container.querySelectorAll<HTMLElement>("li button[aria-expanded]")) {
        fireEvent.click(toggle);
        expect(hangulIn(container), `open ${toggle.textContent}`).toEqual([]);
        losesShown += screen.queryAllByText(/^Turns off with it/).length;
      }
      container.querySelectorAll("details").forEach((d) => (d.open = true));
      screen
        .getAllByRole("button", { name: en.coach.howToRead })
        .forEach((b) => fireEvent.click(b));
      expect(losesShown).toBeGreaterThan(0);
      expect(hangulIn(container), "diagnose open").toEqual([]);
    },
  );

  it(
    "shows no Hangul and says what was not checked when Claude Code is missing or has little history",
    { timeout: FULL_RENDER_MS },
    async () => {
      const few = (switches as { target: string; usage: object | null }[]).filter((s) =>
        s.target.startsWith("claude/plugin"),
      );
      mockIPC(
        (cmd) => {
          if (cmd === "get_installed_agents") return ["codex"];
          if (cmd === "get_checks") return [];
          if (cmd === "get_switches") return [];
          return FIXTURE[cmd] ?? null;
        },
        { shouldMockEvents: true },
      );
      const { container, unmount } = render(<Dashboard />);
      await screen.findByRole("tab", { name: "Usage" });
      fireEvent.click(screen.getByRole("tab", { name: "Checks" }));
      await screen.findAllByText(
        "Nothing was checked: kcal didn’t find Claude Code on this computer. Run claude once and sign in, then press Refresh. Not installed yet? https://code.claude.com/docs/en/setup",
      );
      // 찾은 Codex는 이제 점검한다: 그쪽의 "고칠 것 없음"은 맞다. Claude 칸만 아니어야 한다.
      expect(
        within(screen.getByRole("region", { name: "Claude" })).queryByText(
          "Nothing to fix right now.",
        ),
      ).toBeNull();
      expect(hangulIn(container)).toEqual([]);
      unmount();

      // 기록이 적으면 "안 쓴다"가 아니라 "알 수 없다".
      const plugin = {
        ...few[0],
        usage: {
          days: 30,
          total: 0,
          projects: [],
          projectCount: 0,
          isPartial: false,
          sessions: 3,
          minSessions: 10,
          scopeMinUses: 3,
          lastUsed: null,
          lastProject: null,
        },
        isProposed: false,
        scopeMove: null,
      };
      mockIPC(
        (cmd) => {
          if (cmd === "get_installed_agents") return ["claude"];
          if (cmd === "get_switches") return [plugin];
          return FIXTURE[cmd] ?? null;
        },
        { shouldMockEvents: true },
      );
      const second = render(<Dashboard />);
      await screen.findByRole("tab", { name: "Usage" });
      fireEvent.click(screen.getByRole("tab", { name: "Checks" }));
      const reason = await screen.findByText("Can’t tell · Little history (3 sessions)");
      expect(reason.getAttribute("title")).toMatch(
        /^Not enough Claude history to tell if you use this \(3 sessions in 30 days\)/,
      );
      expect(hangulIn(second.container)).toEqual([]);
    },
  );

  it("uses singular wording for one day and one switch, and names what the weekly row measures", () => {
    const last = { at: Date.UTC(2026, 10, 5, 12) / 1000, project: "side-app" };
    expect(en.coach.used(30, 227, 1, last)).toBe(
      "1 use in 227 Claude sessions over 30 days · last used Nov 5 in side-app",
    );
    expect(en.coach.used(1, 1, 3, null)).toBe("3 uses in 1 Claude session over 1 day");
    expect(en.popover.trim(1, "~900")).toBe("1 thing you could trim · ~900 tokens at startup");
    expect(en.popover.trim(2, null)).toBe("2 things you could trim");
    expect(en.coach.summary(1)).toBe("Change 1 switch and every session loads this much less");
    expect(en.coach.summary(3)).toContain("Change 3 switches");
    expect(en.coach.groupOffWhy(1)).toBe("not used in 1 day");
    expect(en.health.dailyWeekly).toBe("Share of weekly limit used per day");
    expect(en.popover.limit(en.popover.windowShort["7d"])).toBe("Weekly limit");
    expect(en.rate.claudeNoneHint).toContain("press Refresh");
  });

  it("tells Rust the resolved language when it differs from the saved one", async () => {
    const calls: { cmd: string; args: unknown }[] = [];
    mockIPC((cmd, args) => {
      calls.push({ cmd, args });
      if (cmd === "get_settings") return { ...ENGLISH, language: "auto", resolvedLanguage: "en" };
      return FIXTURE[cmd] ?? null;
    });
    render(<App />);
    // setup.ts가 navigator.language를 ko-KR로 둔다: 자동 = 한국어.
    await screen.findByRole("tab", { name: /개요/ });
    expect(calls.filter((c) => c.cmd === "set_ui_language").map((c) => c.args)).toEqual([
      { language: "ko" },
    ]);
  });
});
