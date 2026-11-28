import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { mockIPC } from "@tauri-apps/api/mocks";
import { describe, expect, it } from "vitest";

import { AgentChecks, type HandoffContext } from "@/components/checks-view";
import { setLanguage } from "@/lib/i18n";
import { checkCopy } from "@/lib/check-copy";
import { ko } from "@/lib/i18n-ko";
import type { AppliedSwitch, CoachCheck } from "@/lib/ipc";
import {
  ALL_CHECKS,
  ALL_HIDDEN,
  CODEX_APPLIED,
  EFFORT_APPLIED,
  EFFORT_CHECK,
  SUBAGENT_EFFORT_CHECK,
  EFFORT_PREVIEW,
  GIT_CHECK,
  LONG_APPLIED,
  LONG_CHECK,
  LONG_PREVIEW,
  RULES_APPLIED,
  RULES_CHECK,
  RULES_PREVIEW,
  THIS_MAC_CHECKS,
} from "@/test/preview/checks-fixtures";

type Calls = { cmd: string; args: unknown }[];

function mock(overrides: Record<string, (args: unknown) => unknown> = {}): Calls {
  const calls: Calls = [];
  mockIPC((cmd, args) => {
    calls.push({ cmd, args });
    if (overrides[cmd]) return overrides[cmd](args);
    if (cmd === "preview_check") return EFFORT_PREVIEW;
    if (cmd === "apply_check") return EFFORT_APPLIED;
    return undefined;
  });
  return calls;
}

const EFFORT_TITLE = "생각 깊이 설정 한 줄이 지금 모델에서 무시돼요";
const GIT_TITLE = "커밋 안내가 두 벌이라 Claude가 헷갈릴 수 있어요";
const card = (title: string) => screen.getByRole("group", { name: title });
/** 줄의 펼치기 버튼(줄 전체가 누르는 자리). */
const toggleOf = (title: string) =>
  card(title).querySelector<HTMLButtonElement>("button[aria-expanded]")!;
/** 줄을 눌러 펼치고 그 카드를 돌려준다. */
const openCard = (title: string) => {
  fireEvent.click(toggleOf(title));
  return card(title);
};
const isPrimary = (button: HTMLElement) => button.className.includes("bg-primary");

function list(checks: CoachCheck[], onChanged: () => void = () => {}, log: AppliedSwitch[] = []) {
  return (
    <AgentChecks
      checks={checks}
      log={log}
      emptyText="지금 고칠 것이 없어요."
      onChanged={onChanged}
    />
  );
}

const LONG_TITLE = "kcal 프로젝트 CLAUDE.md 340줄이라 Claude가 다 지키기 어려워요";
const OPEN: HandoffContext = { remaining: 64, minRemaining: 20, timeoutSecs: 300 };

function handoffList(handoff: HandoffContext, onChanged: () => void = () => {}) {
  return (
    <AgentChecks
      checks={[LONG_CHECK, EFFORT_CHECK]}
      emptyText="지금 고칠 것이 없어요."
      onChanged={onChanged}
      handoff={handoff}
    />
  );
}

describe("AgentChecks", () => {
  it.each(["ko", "en"] as const)(
    "renders the subagent effort facts and action in %s",
    (language) => {
      setLanguage(language);
      mock();
      render(list([SUBAGENT_EFFORT_CHECK]));
      const copy = checkCopy(SUBAGENT_EFFORT_CHECK);
      const group = openCard(copy.title);
      expect(within(group).getByText(copy.headline)).toBeTruthy();
      expect(within(group).getByText(copy.what)).toBeTruthy();
      expect(within(group).getByText(copy.note!)).toBeTruthy();
      expect(within(group).getByText(/Explore, researcher/)).toBeTruthy();
      expect(
        within(group).getByRole("button", { name: new RegExp(copy.actions[0].label) }),
      ).toBeTruthy();
      expect(group.textContent).toContain('env.CLAUDE_CODE_SUBAGENT_EFFORT: "high"');
    },
  );
  it("starts with every row folded and opens only the clicked row with headline, chips, diff and actions", () => {
    mock();
    render(list(THIS_MAC_CHECKS));
    // 기본은 모두 접힘: 짧은 제목·메타·작은 버튼만.
    expect(toggleOf(EFFORT_TITLE).getAttribute("aria-expanded")).toBe("false");
    expect(toggleOf(GIT_TITLE).getAttribute("aria-expanded")).toBe("false");
    expect(within(card(EFFORT_TITLE)).getByText("무시되는 생각 깊이 설정")).toBeTruthy();
    expect(within(card(EFFORT_TITLE)).getByText("효과 없음")).toBeTruthy();
    expect(within(card(GIT_TITLE)).getByText("기본 안내와 겹침")).toBeTruthy();
    expect(screen.queryByText("무엇을 하나")).toBeNull();
    expect(screen.queryByText(/Opus 5\.5에서는 이 값/)).toBeNull();
    // 접힌 화면에는 강조 버튼이 없다.
    expect(screen.getAllByRole("button").some(isPrimary)).toBe(false);

    const effort = openCard(EFFORT_TITLE);
    expect(toggleOf(EFFORT_TITLE).getAttribute("aria-expanded")).toBe("true");
    expect(within(effort).getByText("최근 30일 기록")).toBeTruthy();
    expect(within(effort).getByText("되돌리기 가능")).toBeTruthy();
    expect(within(effort).getByLabelText("바뀌는 줄").textContent).toContain(
      '- effortLevel: "high"',
    );
    expect(within(effort).getByText(/Opus 5\.5에서는 이 값\(high\)을 무시하고/)).toBeTruthy();
    expect(within(effort).getByText("무엇을 하나")).toBeTruthy();
    const remove = within(effort).getByRole("button", { name: /옛 설정 지우기\s*추천/ });
    expect(isPrimary(remove)).toBe(true);
    expect(screen.getAllByRole("button").filter(isPrimary)).toEqual([remove]);
    // 키 경로·문서 주소는 접힌 "파일 위치와 출처" 안에만 있다.
    const details = effort.querySelector("details")!;
    expect(details.open).toBe(false);
    expect(within(details).getByText("파일 위치와 출처")).toBeTruthy();
    expect(within(details).getByText(/effortLevel: "high" → \(지움\)/)).toBeTruthy();
    expect(within(details).getByText(/2026-11-07 Claude Code 2\.1\.319에서 확인/)).toBeTruthy();

    // 다른 줄을 펼치면 앞 줄은 접힌다(펼친 줄은 하나).
    const git = openCard(GIT_TITLE);
    expect(toggleOf(EFFORT_TITLE).getAttribute("aria-expanded")).toBe("false");
    expect(within(git).getByRole("button", { name: /그대로 두기\s*추천/ })).toBeTruthy();
    expect(within(git).getByText(/끄면 커밋 서명이 빠져서요/)).toBeTruthy();
    expect(within(git).getByText(ko.checks.keepNote)).toBeTruthy();
    expect(within(git).getByRole("button", { name: "내 규칙만 남기기" })).toBeTruthy();
    // 추천이 keep이 아닌 카드에는 숨기기 버튼이 없다(Rust가 숨기지 않는다).
    expect(
      within(openCard(EFFORT_TITLE)).queryByRole("button", { name: /그대로 두기/ }),
    ).toBeNull();
    // 펼친 줄을 다시 누르면 접힌다.
    fireEvent.click(toggleOf(EFFORT_TITLE));
    expect(toggleOf(EFFORT_TITLE).getAttribute("aria-expanded")).toBe("false");
  });

  it("a button inside a folded row acts without opening it", async () => {
    let changed = 0;
    const calls = mock({ dismiss_check: () => null });
    render(list(THIS_MAC_CHECKS, () => (changed += 1)));
    fireEvent.click(screen.getByRole("button", { name: "커밋 안내가 두 벌 그대로 두기" }));
    await waitFor(() => expect(changed).toBe(1));
    expect(toggleOf(GIT_TITLE).getAttribute("aria-expanded")).toBe("false");
    expect(calls.find((c) => c.cmd === "dismiss_check")?.args).toEqual({
      id: "C-claude-git-instructions",
    });
    // "고치기"는 펼치지 않고 그 자리에서 미리보기로 간다.
    fireEvent.click(screen.getByRole("button", { name: "무시되는 생각 깊이 설정 고치기" }));
    await screen.findByRole("group", { name: "바꿀 내용 확인" });
    expect(calls.find((c) => c.cmd === "preview_check")?.args).toEqual({
      id: "D-claude-effort-level",
      option: "remove",
    });
  });

  it("shows fixed lines first with progress numbers and undoes them through the existing flow", async () => {
    let changed = 0;
    const undone: AppliedSwitch = {
      ...EFFORT_APPLIED,
      id: "9",
      action: "undo",
      undoOf: "1",
      enabled: true,
    };
    const calls = mock({ undo_switch: () => undone });
    const older: AppliedSwitch = { ...EFFORT_APPLIED, id: "0", name: "되돌린 옛 기록" };
    const olderUndo: AppliedSwitch = { ...undone, id: "01", undoOf: "0" };
    // 다른 에이전트 기록·되돌린 기록은 고친 줄이 아니다.
    render(
      list([GIT_CHECK], () => (changed += 1), [EFFORT_APPLIED, CODEX_APPLIED, olderUndo, older]),
    );
    expect(screen.getByLabelText("2개 중 1개 고침").textContent).toBe("1 / 2고침");
    expect(screen.getByText("남은 1개")).toBeTruthy();
    const fixedName = screen.getByText("생각 깊이 옛 설정 정리");
    expect(fixedName.tagName).toBe("S");
    expect(screen.getByRole("img", { name: "고침" })).toBeTruthy();
    expect(screen.queryByText("되돌린 옛 기록")).toBeNull();
    // 고친 줄이 남은 카드보다 위에 있다.
    expect(
      fixedName.compareDocumentPosition(card(GIT_TITLE)) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "생각 깊이 옛 설정 정리 되돌리기" }));
    const done = await screen.findByRole("group", { name: "적용 결과" });
    expect(within(done).getByText("생각 깊이 옛 설정 정리 되돌림")).toBeTruthy();
    expect(calls.find((c) => c.cmd === "undo_switch")?.args).toEqual({ id: "1" });
    expect(changed).toBe(1);
  });

  it("says nothing is left when every card is fixed, and adds known tokens to the gray line", () => {
    mock();
    const { unmount } = render(list([], () => {}, [EFFORT_APPLIED]));
    expect(screen.getByLabelText("1개 중 1개 고침")).toBeTruthy();
    expect(screen.getByText("남은 것이 없어요")).toBeTruthy();
    unmount();
    render(list(ALL_CHECKS));
    expect(screen.getByLabelText("4개 중 0개 고침")).toBeTruthy();
    // 토큰은 Rust가 준 값(hook 1,240 + skill 85)만 더한다.
    expect(
      screen.getByText("남은 4개 · 그중 2개를 고치면 세션마다 ~1,325 토큰 덜 실려요"),
    ).toBeTruthy();
  });

  it("shows instruction instead of a button for a hook kcal does not write", () => {
    mock();
    render(list(ALL_CHECKS));
    // 안내만 하는 카드는 접힌 줄에 버튼이 없다.
    expect(within(card("같은 안내문이 두 번 실리고 있어요")).getAllByRole("button")).toHaveLength(
      1,
    );
    const hooks = openCard("같은 안내문이 두 번 실리고 있어요");
    expect(within(hooks).queryByRole("button", { name: "hook 빼기" })).toBeNull();
    expect(within(hooks).getByText("cat AGENTS.md")).toBeTruthy();
    expect(within(hooks).getByText(/직접 지우세요/)).toBeTruthy();
  });

  it("runs preview, confirm and done in the card's place while other cards stay visible", async () => {
    const calls = mock();
    let changed = 0;
    const { rerender } = render(list(THIS_MAC_CHECKS, () => (changed += 1)));
    fireEvent.click(within(openCard(EFFORT_TITLE)).getByRole("button", { name: /옛 설정 지우기/ }));
    const panel = screen.getByRole("group", { name: "바꿀 내용 확인" });
    await within(panel).findByText('- "high"');
    // 지우는 줄은 `-`만 보인다(가짜 "+ (지움)" 줄 없음).
    expect(within(panel).queryByText("+ (지움)")).toBeNull();
    // 미리보기는 그 카드 자리(첫 자리)의 테두리 카드 안에 있고, 다른 카드는 그대로다.
    const frame = panel.parentElement;
    expect(frame?.previousElementSibling).toBeNull();
    expect(frame?.nextElementSibling).toBe(card(GIT_TITLE));
    expect(screen.queryByRole("group", { name: EFFORT_TITLE })).toBeNull();
    expect(calls.some((c) => c.cmd === "apply_check")).toBe(false);
    expect(calls.find((c) => c.cmd === "preview_check")?.args).toEqual({
      id: "D-claude-effort-level",
      option: "remove",
    });

    fireEvent.click(within(panel).getByRole("button", { name: "확인하고 바꾸기" }));
    const done = await screen.findByRole("group", { name: "적용 결과" });
    expect(within(done).getByText("생각 깊이 옛 설정 정리 했어요")).toBeTruthy();
    expect(within(done).getByText(ko.coach.restart)).toBeTruthy();
    expect(within(done).getByText(EFFORT_APPLIED.undoCommand)).toBeTruthy();
    expect(calls.find((c) => c.cmd === "apply_check")?.args).toEqual({
      id: "D-claude-effort-level",
      option: "remove",
      beforeHash: EFFORT_PREVIEW.beforeHash,
    });
    expect(changed).toBe(1);

    // 다시 읽은 목록에서 고친 카드가 빠져도 완료 패널은 그 자리에 남는다.
    rerender(list([GIT_CHECK], () => (changed += 1)));
    expect(screen.getByRole("group", { name: "적용 결과" })).toBeTruthy();
    expect(card(GIT_TITLE)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "목록으로" }));
    expect(screen.queryByRole("group", { name: "적용 결과" })).toBeNull();
  });

  it("shows the refusal reason in plain words and the empty state", async () => {
    const message =
      "미리보기 뒤에 설정 파일이 바뀌었어요. 아무것도 바꾸지 않았어요. 다시 미리 보세요.";
    mock({ apply_check: () => Promise.reject({ kind: "invalid_input", message }) });
    const { unmount } = render(list(THIS_MAC_CHECKS));
    fireEvent.click(within(openCard(EFFORT_TITLE)).getByRole("button", { name: /옛 설정 지우기/ }));
    const panel = screen.getByRole("group", { name: "바꿀 내용 확인" });
    await within(panel).findByText('- "high"');
    fireEvent.click(within(panel).getByRole("button", { name: "확인하고 바꾸기" }));
    expect((await screen.findByRole("alert")).textContent).toBe(message);
    unmount();

    render(list(ALL_HIDDEN));
    expect(screen.getByText("지금 고칠 것이 없어요.")).toBeTruthy();
    expect(screen.queryByRole("group", { name: GIT_TITLE })).toBeNull();
  });

  it("handoff button is off below the minimum remaining and shows why and the remaining percent", () => {
    mock();
    render(handoffList({ remaining: 12, minRemaining: 20, timeoutSecs: 300 }));
    expect(within(card(LONG_TITLE)).getByText("한도 씀")).toBeTruthy();
    // 접힌 줄의 "Claude에게"도 같은 문으로 꺼진다.
    const row = screen.getByRole("button", {
      name: "kcal 프로젝트 CLAUDE.md 340줄로 김 Claude에게",
    });
    expect((row as HTMLButtonElement).disabled).toBe(true);
    const long = openCard(LONG_TITLE);
    const button = within(long).getByRole("button", { name: /Claude에게 줄여 달라고 하기/ });
    expect((button as HTMLButtonElement).disabled).toBe(true);
    expect(within(long).getByText("한도를 써요 · 지금 남은 한도 12%")).toBeTruthy();
    expect(within(long).getByText("남은 한도가 20%보다 적어요.")).toBeTruthy();
  });

  it("handoff button is off when the limit is unknown", () => {
    mock();
    render(handoffList({ remaining: null, minRemaining: 20, timeoutSecs: 300 }));
    const long = openCard(LONG_TITLE);
    const button = within(long).getByRole("button", { name: /Claude에게 줄여 달라고 하기/ });
    expect((button as HTMLButtonElement).disabled).toBe(true);
    expect(within(long).getByText("한도를 아직 몰라요.")).toBeTruthy();
  });

  it("handoff shows the diff, what Claude merged and the missing import, then applies by id and hash only", async () => {
    let changed = 0;
    const calls = mock({
      ask_claude_handoff: () => LONG_PREVIEW,
      apply_claude_handoff: () => LONG_APPLIED,
    });
    render(handoffList(OPEN, () => (changed += 1)));
    fireEvent.click(
      within(openCard(LONG_TITLE)).getByRole("button", { name: /Claude에게 줄여 달라고 하기/ }),
    );
    const panel = await screen.findByRole("group", { name: "Claude가 고쳐 쓴 글" });
    await within(panel).findByText("340줄 → 182줄");
    // 줄 앞 표시(- 뺌, + 넣음) 뒤에 원래 줄(마크다운 목록 "- …")이 온다.
    expect(within(panel).getByText("- - Run the test suite before you push.")).toBeTruthy();
    expect(within(panel).getByText("+ - Run the test suite before you push.")).toBeTruthy();
    expect(within(panel).getByText("Claude가 합치거나 뺀 것")).toBeTruthy();
    expect(within(panel).getByText("테스트 실행 규칙 두 줄을 합쳤어요.")).toBeTruthy();
    expect(within(panel).getByRole("alert").textContent).toContain("@docs/style.md");
    fireEvent.click(within(panel).getByRole("button", { name: "확인하고 바꾸기" }));
    await screen.findByRole("group", { name: "적용 결과" });
    expect(calls.find((c) => c.cmd === "apply_claude_handoff")?.args).toEqual({
      id: LONG_CHECK.id,
      beforeHash: LONG_PREVIEW.beforeHash,
    });
    expect(changed).toBe(1);
  });

  it("handoff apply button is off when the result cannot be applied", async () => {
    mock({ ask_claude_handoff: () => ({ ...LONG_PREVIEW, canApply: false }) });
    render(handoffList(OPEN));
    fireEvent.click(
      within(openCard(LONG_TITLE)).getByRole("button", { name: /Claude에게 줄여 달라고 하기/ }),
    );
    const panel = await screen.findByRole("group", { name: "Claude가 고쳐 쓴 글" });
    const apply = await within(panel).findByRole("button", { name: "확인하고 바꾸기" });
    expect((apply as HTMLButtonElement).disabled).toBe(true);
  });

  it("handoff shows the Opus unavailable message and goes back to the card", async () => {
    mock({ ask_claude_handoff: () => Promise.reject({ kind: "opusUnavailable" }) });
    render(handoffList(OPEN));
    fireEvent.click(
      within(openCard(LONG_TITLE)).getByRole("button", { name: /Claude에게 줄여 달라고 하기/ }),
    );
    expect((await screen.findByRole("alert")).textContent).toBe(
      "이 계정에서는 Opus 5.5를 쓸 수 없어 맡길 수 없어요. 파일은 그대로예요.",
    );
    expect(card(LONG_TITLE)).toBeTruthy();
  });

  it("keep hides the card through dismissCheck by id, then asks the tab to read again", async () => {
    let changed = 0;
    const calls = mock({ dismiss_check: () => null });
    render(list(THIS_MAC_CHECKS, () => (changed += 1)));
    fireEvent.click(within(card(GIT_TITLE)).getByRole("button", { name: /그대로 두기/ }));
    await waitFor(() => expect(changed).toBe(1));
    expect(calls.find((c) => c.cmd === "dismiss_check")?.args).toEqual({
      id: "C-claude-git-instructions",
    });
  });

  describe("rules card (0018 작업 B)", () => {
    const RULES_TITLE = "늘 실리는 규칙 9개";
    const ASK = /Claude에게 규칙마다 물어보기/;
    const rulesList = (handoff: HandoffContext, onChanged: () => void = () => {}) => (
      <AgentChecks
        checks={[RULES_CHECK, EFFORT_CHECK]}
        emptyText="지금 고칠 것이 없어요."
        onChanged={onChanged}
        handoff={handoff}
      />
    );

    it("card shows the count, what changes, who does it and the limit note under the button", () => {
      mock();
      render(rulesList(OPEN));
      const rules = openCard(RULES_TITLE);
      expect(
        within(rules).getByText(/규칙 9개\(20,631자\)가 모든 세션 시작 때 실려요/),
      ).toBeTruthy();
      expect(within(rules).getByText("내 것 중 바뀌는 것")).toBeTruthy();
      expect(within(rules).getByText(/Claude\(Opus 5\.5\)가 규칙마다 제안하고/)).toBeTruthy();
      const button = within(rules).getByRole("button", { name: ASK });
      expect((button as HTMLButtonElement).disabled).toBe(false);
      expect(within(rules).getByText("한도를 써요 · 지금 남은 한도 64%")).toBeTruthy();
      expect(within(rules).getByRole("button", { name: "그대로 두기" })).toBeTruthy();
    });

    it("results show one line per rule with paths, skill, keep, a warning and a failed check", async () => {
      mock({ ask_claude_rules: () => RULES_PREVIEW });
      render(rulesList(OPEN));
      fireEvent.click(within(openCard(RULES_TITLE)).getByRole("button", { name: ASK }));
      const panel = await screen.findByRole("group", { name: "Claude가 규칙마다 제안한 것" });
      const row = (name: string) => within(panel).getByRole("listitem", { name });
      const paths = await within(panel).findByRole("listitem", { name: "common/coding-style.md" });
      expect(paths.textContent).toContain("이 파일을 다룰 때만 싣기");
      expect(paths.textContent).toContain("잃는 것:");
      expect(within(paths).getByRole("button", { name: /이대로 바꾸기/ })).toBeTruthy();
      const skill = row("common/git-workflow.md");
      expect(skill.textContent).toContain("git-workflow");
      expect(within(skill).getByText(/다른 규칙 1곳이 이 파일을 가리켜요/)).toBeTruthy();
      const keep = row("common/working-with-me.md");
      expect(keep.textContent).toContain("그대로 두기");
      expect(within(keep).queryByRole("button")).toBeNull();
      const failed = row("common/kiro-delegation.md");
      expect(failed.textContent).toContain("Claude 제안을 쓸 수 없어요(");
      expect(within(failed).queryByRole("button")).toBeNull();
    });

    it("applying one rule sends id, key and hash only and marks that line done", async () => {
      let changed = 0;
      const calls = mock({
        ask_claude_rules: () => RULES_PREVIEW,
        apply_claude_rule: () => RULES_APPLIED,
      });
      render(rulesList(OPEN, () => (changed += 1)));
      fireEvent.click(within(openCard(RULES_TITLE)).getByRole("button", { name: ASK }));
      const paths = await screen.findByRole("listitem", { name: "common/coding-style.md" });
      fireEvent.click(within(paths).getByRole("button", { name: /이대로 바꾸기/ }));
      await within(paths).findByRole("status");
      expect(within(paths).queryByRole("button")).toBeNull();
      expect(calls.find((c) => c.cmd === "apply_claude_rule")?.args).toEqual({
        id: RULES_CHECK.id,
        key: "r1",
        beforeHash: RULES_PREVIEW.rules[0].beforeHash,
      });
      expect(changed).toBe(1);
      // 다른 줄의 버튼은 그대로 남는다.
      const skill = screen.getByRole("listitem", { name: "common/git-workflow.md" });
      expect(within(skill).getByRole("button", { name: /이대로 바꾸기/ })).toBeTruthy();
    });

    it("unknown keys get one line and Opus unavailable goes back to the card", async () => {
      mock({ ask_claude_rules: () => ({ ...RULES_PREVIEW, unknownKeys: 1 }) });
      const { unmount } = render(rulesList(OPEN));
      fireEvent.click(within(openCard(RULES_TITLE)).getByRole("button", { name: ASK }));
      expect(
        await screen.findByText("Claude 제안을 쓸 수 없어요(모르는 규칙 1개에 답했어요)"),
      ).toBeTruthy();
      unmount();
      mock({ ask_claude_rules: () => Promise.reject({ kind: "opusUnavailable" }) });
      render(rulesList(OPEN));
      fireEvent.click(within(openCard(RULES_TITLE)).getByRole("button", { name: ASK }));
      expect((await screen.findByRole("alert")).textContent).toBe(
        "이 계정에서는 Opus 5.5를 쓸 수 없어 맡길 수 없어요. 파일은 그대로예요.",
      );
      expect(card(RULES_TITLE)).toBeTruthy();
    });
  });
  it("returns focus to the row's action button when a panel closes with Escape", async () => {
    mock();
    render(list(THIS_MAC_CHECKS));
    const opener = screen.getByRole("button", { name: "무시되는 생각 깊이 설정 고치기" });
    fireEvent.click(opener);
    const panel = await screen.findByRole("group", { name: "바꿀 내용 확인" });
    await waitFor(() => expect(document.activeElement).toBe(panel));
    fireEvent.keyDown(panel, { key: "Escape" });
    await waitFor(() =>
      expect(document.activeElement).toBe(
        screen.getByRole("button", { name: "무시되는 생각 깊이 설정 고치기" }),
      ),
    );
  });
});
