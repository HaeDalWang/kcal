import { fireEvent, render, screen } from "@testing-library/react";
import { mockIPC } from "@tauri-apps/api/mocks";
import { expect, it, vi } from "vitest";

import { CoachPanel } from "@/components/coach-view";
import { ko } from "@/lib/i18n-ko";
import { DIAG46_SWITCHES } from "@/test/preview/checks-fixtures";

it("labels both project folders and opens connector off guidance without a write", () => {
  const invoke = vi.fn();
  mockIPC(invoke);
  render(
    <CoachPanel
      agent="claude"
      switches={DIAG46_SWITCHES.map((s) => ({ ...s, isProposed: !!s.projectPath }))}
      log={[]}
      onChanged={vi.fn()}
      onShowEffect={vi.fn()}
    />,
  );
  expect(screen.getByText("project-docs · /Users/me/work/kcal")).toBeTruthy();
  expect(screen.getByText("project-docs · /Users/me/work/side-app")).toBeTruthy();
  fireEvent.click(screen.getByText(ko.coach.groupKeep(1)));
  fireEvent.click(screen.getByRole("button", { name: `claude.ai Figma ${ko.coach.howToOff}` }));
  expect(screen.getByText(/disableClaudeAiConnectors: true/)).toBeTruthy();
  expect(screen.getByText(/ENABLE_CLAUDEAI_MCP_SERVERS=false/)).toBeTruthy();
  expect(screen.getByText(/https:\/\/claude.ai\/customize\/connectors/)).toBeTruthy();
  expect(invoke.mock.calls.some(([cmd]) => /preview|apply/.test(String(cmd)))).toBe(false);
});
