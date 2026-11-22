import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { mockIPC } from "@tauri-apps/api/mocks";
import { describe, expect, it, vi } from "vitest";

import { ChecksTab } from "@/components/checks-tab";
import {
  CODEX_APPLIED,
  CODEX_FAST_CHECK,
  DASHBOARD_CHECKS,
  DASHBOARD_SWITCHES,
  EFFORT_APPLIED,
  EFFORT_PREVIEW,
  LONG_CHECK,
  THIS_MAC_CHECKS,
} from "@/test/preview/checks-fixtures";
import type { Agent, Settings, UsageSnapshot } from "@/lib/ipc";
import { DEFAULT_TUNING, TUNING_SPEC } from "@/test/tuning";

type Calls = { cmd: string; args: unknown }[];

function mock(overrides: Record<string, (args: unknown) => unknown> = {}): Calls {
  const calls: Calls = [];
  mockIPC((cmd, args) => {
    calls.push({ cmd, args });
    if (overrides[cmd]) return overrides[cmd](args);
    if (cmd === "get_checks") return DASHBOARD_CHECKS;
    if (cmd === "get_switches") return DASHBOARD_SWITCHES;
    if (cmd === "get_coach_log") return [CODEX_APPLIED];
    if (cmd === "preview_check") return EFFORT_PREVIEW;
    if (cmd === "apply_check") return EFFORT_APPLIED;
    return null;
  });
  return calls;
}

const ALL: Agent[] = ["claude", "codex", "kiro"];
const EFFORT_TITLE = "생각 깊이 설정 한 줄이 지금 모델에서 무시돼요";
const section = (name: string) => screen.getByRole("region", { name });
const column = (agent: string, name: "최적화" | "진단하기") =>
  within(section(agent)).getByRole("group", { name });
const count = (calls: Calls, cmd: string) => calls.filter((c) => c.cmd === cmd).length;
/** 최적화 줄을 눌러 펼친다. */
const open = (row: HTMLElement) => {
  fireEvent.click(row.querySelector("button[aria-expanded]")!);
  return row;
};

describe("ChecksTab", () => {
  it("puts Claude cards only in the Claude section and empty texts for Codex and Kiro", async () => {
    mock();
    render(<ChecksTab agents={ALL} focusKey={0} onShowEffect={() => {}} />);
    await within(column("Claude", "최적화")).findByRole("group", { name: EFFORT_TITLE });
    expect(
      within(column("Claude", "최적화")).getByRole("group", { name: /커밋 안내가 두 벌/ }),
    ).toBeTruthy();
    expect(within(column("Codex", "최적화")).getByText("지금 고칠 것이 없어요.")).toBeTruthy();
    expect(within(column("Kiro", "최적화")).getByText("지금 고칠 것이 없어요.")).toBeTruthy();
    expect(within(section("Codex")).queryByRole("group", { name: EFFORT_TITLE })).toBeNull();

    // 진단하기도 그 에이전트 스위치만.
    await within(column("Codex", "진단하기")).findByText("github@openai-curated");
    expect(within(column("Claude", "진단하기")).queryByText("github@openai-curated")).toBeNull();
    expect(within(column("Claude", "진단하기")).getByText("context7")).toBeTruthy();
    expect(within(column("Kiro", "진단하기")).getByText("aws-docs")).toBeTruthy();
    // 숨긴 카드 줄은 탭 맨 아래 한 번.
    expect(screen.getAllByText("숨긴 카드 1개")).toHaveLength(1);
  });

  it("shows the Codex fast-mode card in the Codex section with the /fast guide and no write button", async () => {
    mock({ get_checks: () => [...DASHBOARD_CHECKS, CODEX_FAST_CHECK] });
    render(<ChecksTab agents={ALL} focusKey={0} onShowEffect={() => {}} />);
    const row = await within(column("Codex", "최적화")).findByRole("group", {
      name: "빠른 모드라 한도가 2배로 닳아요",
    });
    expect(within(column("Claude", "최적화")).queryByText(/빠른 모드/)).toBeNull();
    open(row);
    expect(within(row).getByText("/fast").tagName).toBe("CODE");
    expect(within(row).getByText(/잃는 것: 응답 생성 속도예요/)).toBeTruthy();
    expect(within(row).queryByRole("button", { name: /Standard로 돌아가기|고치기/ })).toBeNull();
  });

  it("shows the Claude empty text when every Claude card is hidden", async () => {
    mock({ get_checks: () => [{ ...THIS_MAC_CHECKS[1], isDismissed: true }] });
    render(<ChecksTab agents={["claude"]} focusKey={0} onShowEffect={() => {}} />);
    expect(
      await within(column("Claude", "최적화")).findByText("지금 고칠 것이 없어요."),
    ).toBeTruthy();
  });

  it("says nothing was checked, not 'nothing to fix', when Claude Code is not found", async () => {
    mock({
      get_installed_agents: () => ["codex"],
      get_checks: () => [],
      get_switches: () => [],
    });
    render(<ChecksTab agents={["claude", "codex"]} focusKey={0} onShowEffect={() => {}} />);
    const message =
      "점검하지 못했어요. 이 컴퓨터에서 Claude Code를 찾지 못했어요. claude를 한 번 실행해 로그인한 뒤 새로고침하세요. 아직 설치 전이면: https://code.claude.com/docs/en/setup";
    await within(column("Claude", "최적화")).findByText(message);
    expect(await within(column("Claude", "진단하기")).findByText(message)).toBeTruthy();
    expect(within(column("Claude", "최적화")).queryByText("지금 고칠 것이 없어요.")).toBeNull();
    // 찾은 Codex(이제 점검한다): 고칠 것이 없다는 말은 맞다.
    expect(within(column("Codex", "최적화")).getByText("지금 고칠 것이 없어요.")).toBeTruthy();
    // 찾은 Claude: 점검할 것이 없다는 말은 맞다.
  });

  it("keeps 'nothing to fix' when Claude Code is found, and says no plugins are on when the list is empty", async () => {
    mock({
      get_installed_agents: () => ["claude"],
      get_checks: () => [],
      get_switches: () => [],
    });
    render(<ChecksTab agents={["claude"]} focusKey={0} onShowEffect={() => {}} />);
    expect(
      await within(column("Claude", "최적화")).findByText("지금 고칠 것이 없어요."),
    ).toBeTruthy();
    expect(
      await within(column("Claude", "진단하기")).findByText(
        "켜 둔 플러그인·MCP가 없어서 끌 것이 없어요.",
      ),
    ).toBeTruthy();
  });

  it("reads checks, switches and log again after an apply in the card's place", async () => {
    const calls = mock();
    render(<ChecksTab agents={ALL} focusKey={0} onShowEffect={() => {}} />);
    const effort = await within(column("Claude", "최적화")).findByRole("group", {
      name: EFFORT_TITLE,
    });
    fireEvent.click(within(open(effort)).getByRole("button", { name: /옛 설정 지우기/ }));
    const panel = await within(column("Claude", "최적화")).findByRole("group", {
      name: "바꿀 내용 확인",
    });
    await within(panel).findByText('- "high"');
    fireEvent.click(within(panel).getByRole("button", { name: "확인하고 바꾸기" }));
    await within(column("Claude", "최적화")).findByRole("group", { name: "적용 결과" });
    await waitFor(() => expect(count(calls, "get_checks")).toBe(2));
    expect(count(calls, "get_switches")).toBe(2);
    expect(count(calls, "get_coach_log")).toBe(2);
  });

  it("shows a check apply as a fixed line in Optimize, not under Diagnose recent changes", async () => {
    mock({ get_coach_log: () => [EFFORT_APPLIED] });
    render(<ChecksTab agents={ALL} focusKey={0} onShowEffect={() => {}} />);
    const optimize = column("Claude", "최적화");
    await within(optimize).findByRole("button", { name: "생각 깊이 옛 설정 정리 되돌리기" });
    expect(within(optimize).getByLabelText("3개 중 1개 고침")).toBeTruthy();
    expect(
      within(column("Claude", "진단하기")).queryByRole("group", { name: "최근 바꾼 것" }),
    ).toBeNull();
  });

  it("counts only check applies inside the Diagnose period in fixed lines and progress", async () => {
    const days = DEFAULT_TUNING.diagnoseUsageDays;
    const ago = (d: number) => new Date(Date.now() - d * 86_400_000).toISOString();
    const settings: Settings = {
      enabledAgents: ALL,
      tuning: DEFAULT_TUNING,
      tuningSpec: TUNING_SPEC,
      reason: null,
      language: "auto",
      resolvedLanguage: "ko",
    };
    const recent = { ...EFFORT_APPLIED, at: ago(1) };
    const old = {
      ...EFFORT_APPLIED,
      id: "0",
      at: ago(days + 5),
      target: "claude/check:D-claude-max-thinking-tokens:remove",
      name: "생각 토큰 한도 설정 정리",
    };
    mock({ get_coach_log: () => [recent, old], get_settings: () => settings });
    render(<ChecksTab agents={ALL} focusKey={0} onShowEffect={() => {}} />);
    const optimize = column("Claude", "최적화");
    await within(optimize).findByRole("button", { name: "생각 깊이 옛 설정 정리 되돌리기" });
    await waitFor(() =>
      expect(within(optimize).queryByText("생각 토큰 한도 설정 정리")).toBeNull(),
    );
    expect(within(optimize).getByLabelText("3개 중 1개 고침")).toBeTruthy();
  });

  it("keeps one open row across Optimize and Diagnose so the screen has one primary button", async () => {
    mock();
    render(<ChecksTab agents={ALL} focusKey={0} onShowEffect={() => {}} />);
    const effort = open(
      await within(column("Claude", "최적화")).findByRole("group", { name: EFFORT_TITLE }),
    );
    const effortToggle = effort.querySelector("button[aria-expanded]")!;
    expect(effortToggle.getAttribute("aria-expanded")).toBe("true");
    const diagnoseToggle = within(column("Claude", "진단하기"))
      .getByText("k8s-tools@market")
      .closest("button")!;
    fireEvent.click(diagnoseToggle);
    expect(diagnoseToggle.getAttribute("aria-expanded")).toBe("true");
    expect(effortToggle.getAttribute("aria-expanded")).toBe("false");
    expect(document.querySelectorAll("button.bg-primary")).toHaveLength(1);
    // 같은 줄을 다시 누르면 닫힌다(칸마다의 기존 토글).
    fireEvent.click(diagnoseToggle);
    expect(diagnoseToggle.getAttribute("aria-expanded")).toBe("false");
    expect(document.querySelectorAll("button.bg-primary")).toHaveLength(0);
  });

  it("shows hidden cards without bringing them back, then brings back only the one asked", async () => {
    const calls = mock();
    render(<ChecksTab agents={ALL} focusKey={0} onShowEffect={() => {}} />);
    const toggle = await screen.findByRole("button", { name: "보기" });
    fireEvent.click(toggle);
    // 보기만 해서는 아무것도 쓰지 않는다.
    expect(toggle.getAttribute("aria-expanded")).toBe("true");
    expect(count(calls, "restore_dismissed_check")).toBe(0);
    const back = screen.getAllByRole("button", { name: "다시 알리기" });
    expect(back.length).toBeGreaterThan(0);
    const hiddenId = DASHBOARD_CHECKS.find((c) => c.isDismissed)!.id;
    fireEvent.click(back[0]);
    await waitFor(() => expect(count(calls, "get_checks")).toBe(2));
    expect(calls.filter((c) => c.cmd === "restore_dismissed_check").map((c) => c.args)).toEqual([
      { id: hiddenId },
    ]);
  });

  it("sends the diagnosis effect button to the usage tab", async () => {
    mock();
    const onShowEffect = vi.fn();
    render(<ChecksTab agents={ALL} focusKey={0} onShowEffect={onShowEffect} />);
    const recent = await within(column("Codex", "진단하기")).findByRole("group", {
      name: "최근 바꾼 것",
    });
    fireEvent.click(within(recent).getByRole("button", { name: "효과 보기" }));
    expect(onShowEffect).toHaveBeenCalledOnce();
  });

  it("handoff button reads Claude remaining percent and the minimum from the existing usage and settings", async () => {
    const future = new Date(Date.now() + 3_600_000).toISOString();
    const usage: UsageSnapshot = {
      agents: [
        {
          agent: "claude",
          source: "api",
          observedAt: future,
          checkedAt: future,
          reason: null,
          reasonKind: null,
          windows: [
            { id: "5h", usedPercent: 36, resetsAt: future, resetPassed: false },
            { id: "7d", usedPercent: 10, resetsAt: future, resetPassed: false },
          ],
        },
      ],
    };
    const settings: Settings = {
      enabledAgents: ALL,
      tuning: { ...DEFAULT_TUNING, claudeHandoffMinRemaining: 70 },
      tuningSpec: TUNING_SPEC,
      reason: null,
      language: "auto",
      resolvedLanguage: "ko",
    };
    mock({
      get_checks: () => [LONG_CHECK],
      get_usage: () => usage,
      get_settings: () => settings,
    });
    render(<ChecksTab agents={ALL} focusKey={0} onShowEffect={() => {}} />);
    const long = open(
      await within(column("Claude", "최적화")).findByRole("group", {
        name: /CLAUDE.md 340줄이라/,
      }),
    );
    await within(long).findByText("한도를 써요 · 지금 남은 한도 64%");
    await within(long).findByText("남은 한도가 70%보다 적어요.");
    const button = within(long).getByRole("button", { name: /Claude에게 줄여 달라고 하기/ });
    expect((button as HTMLButtonElement).disabled).toBe(true);
  });

  it("scrolls to the first section with an alert card only when opened from the popover", async () => {
    mock({ get_checks: () => [{ ...THIS_MAC_CHECKS[0], agent: "codex" }] });
    const scrolled: Element[] = [];
    const spy = vi.spyOn(Element.prototype, "scrollIntoView").mockImplementation(function (
      this: Element,
    ) {
      scrolled.push(this);
    });
    const { rerender } = render(<ChecksTab agents={ALL} focusKey={0} onShowEffect={() => {}} />);
    await within(column("Codex", "최적화")).findByRole("group", { name: EFFORT_TITLE });
    expect(scrolled).toHaveLength(0);
    rerender(<ChecksTab agents={ALL} focusKey={1} onShowEffect={() => {}} />);
    await waitFor(() => expect(scrolled).toEqual([section("Codex")]));
    spy.mockRestore();
  });
});
