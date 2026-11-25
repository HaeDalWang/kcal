import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { mockIPC } from "@tauri-apps/api/mocks";
import { describe, expect, it, vi } from "vitest";
import { ClaudeSessionsSection } from "@/components/claude-sessions";
import { type ClaudeSession, type ClaudeSessions } from "@/lib/ipc";

const row = (status: string, stopEligible = false): ClaudeSession => ({
  pid: 42,
  sessionId: "abc",
  cwd: "/work/a-very-long-project-name/with/nested/folders",
  status,
  startedAt: null,
  kind: null,
  idleSeconds: 90000,
  stopEligible,
  exclusion: null,
});
const inventory: ClaudeSessions = {
  canStop: true,
  idleHours: 24,
  sessions: [row("waiting"), row("busy"), row("idle", true)],
};

describe("Claude sessions", () => {
  it("counts statuses, reveals folders, and reads only on mount/focus/user action", async () => {
    const read = vi.fn(() => inventory);
    mockIPC(read);
    render(<ClaudeSessionsSection />);
    const summary = await screen.findByText("입력 대기 1 · 작업 중 1 · 대기 1");
    expect(read).toHaveBeenCalledTimes(1);
    fireEvent.click(summary);
    expect(screen.getAllByText(inventory.sessions[0].cwd!)).toHaveLength(3);
    fireEvent.click(screen.getByRole("button", { name: "세션 새로고침" }));
    await waitFor(() => expect(read).toHaveBeenCalledTimes(2));
    await act(() => window.dispatchEvent(new Event("focus")));
    expect(read).toHaveBeenCalledTimes(3);
  });

  it("requires one confirmation, lists only eligible targets, and sends those exact identities", async () => {
    const stop = vi.fn<(...args: unknown[]) => { stopped: number; skipped: string[] }>(() => ({
      stopped: 1,
      skipped: [],
    }));
    mockIPC((cmd, args) => (cmd === "stop_idle_claude_sessions" ? stop(args) : inventory));
    render(<ClaudeSessionsSection />);
    fireEvent.click(await screen.findByRole("button", { name: "24h 넘은 1개 끄기" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(/보내지 않은 입력/).textContent).toContain("claude --resume");
    expect(within(dialog).getAllByRole("listitem")).toHaveLength(1);
    expect(stop).not.toHaveBeenCalled();
    fireEvent.click(within(dialog).getByRole("button", { name: "취소" }));
    expect(stop).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "24h 넘은 1개 끄기" }));
    fireEvent.click(
      within(await screen.findByRole("dialog")).getByRole("button", { name: "확인하고 끄기" }),
    );
    await screen.findByText("1개 종료 · 0개 건너뜀");
    expect(stop).toHaveBeenCalledWith({ targets: [{ pid: 42, sessionId: "abc" }] });
  });

  it("hides stop on Windows and shows empty and read error states", async () => {
    mockIPC(() => ({ ...inventory, canStop: false, sessions: [] }));
    const view = render(<ClaudeSessionsSection />);
    await screen.findByText("실행 중인 세션이 없어요.");
    expect(screen.queryByText(/개 끄기/)).toBeNull();
    view.unmount();
    mockIPC(() => {
      throw { kind: "invalid_input", message: "읽지 못함" };
    });
    render(<ClaudeSessionsSection />);
    await screen.findByText("읽지 못함");
    expect(screen.queryByText(/개 끄기/)).toBeNull();
  });

  it("shows stop failures, skipped counts and reasons without claiming all sessions stopped", async () => {
    let failure = true;
    mockIPC((cmd) => {
      if (cmd !== "stop_idle_claude_sessions") return inventory;
      if (failure) throw { kind: "internal", message: "종료 실패" };
      return { stopped: 0, skipped: ["42: 그사이 세션이 바뀜"] };
    });
    render(<ClaudeSessionsSection />);
    for (const expected of ["종료 실패", "0개 종료 · 1개 건너뜀 42: 그사이 세션이 바뀜"]) {
      fireEvent.click(await screen.findByRole("button", { name: "24h 넘은 1개 끄기" }));
      fireEvent.click(
        within(await screen.findByRole("dialog")).getByRole("button", { name: "확인하고 끄기" }),
      );
      await screen.findByText(expected);
      failure = false;
    }
  });
});
