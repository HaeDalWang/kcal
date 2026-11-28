import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { mockIPC } from "@tauri-apps/api/mocks";
import { describe, expect, it, vi } from "vitest";

import { SwitchLists } from "@/components/coach-list";
import { CoachPanel } from "@/components/coach-view";
import { ko } from "@/lib/i18n-ko";
import { en } from "@/lib/i18n-en";
import type {
  Agent,
  AppliedSwitch,
  CoachSwitch,
  RateSnapshot,
  ScopeMovePreview,
  SwitchPreview,
  SwitchUsage,
} from "@/lib/ipc";

const sw = (target: string, extra: Partial<CoachSwitch> = {}): CoachSwitch => {
  const [agent, rest] = target.split("/");
  return {
    target,
    agent: agent as CoachSwitch["agent"],
    kind: rest.startsWith("mcp") ? "mcp" : "plugin",
    name: rest.split(":")[1],
    enabled: true,
    tokens: 900,
    loading: "startup",
    isProposed: true,
    format: "toml",
    usage: null,
    scope: null,
    scopeMove: null,
    contents: null,
    skill: null,
    file: "/h/.codex/config.toml",
    instruction: null,
    ...extra,
  };
};

const SWITCHES: CoachSwitch[] = [
  sw("codex/plugin:big@m"),
  sw("codex/mcp:small", { tokens: 300 }),
  sw("claude/mcp:docs", {
    tokens: 400,
    format: "manual",
    file: null,
    instruction: "Claude Code에서 /mcp를 열고 이 서버를 끄세요.",
  }),
  sw("kiro/mcp:k", { tokens: null, loading: "deferred", isProposed: false, format: "json" }),
];

const LAST_USED = Date.UTC(2026, 10, 5, 12) / 1000;
const usage = (projects: [string, number][], projectCount = projects.length): SwitchUsage => ({
  days: 30,
  total: projects.reduce((sum, [, n]) => sum + n, 0),
  projects: projects.map(([name, count]) => ({
    name,
    path: `/h/work/${name}`,
    count,
    exists: name !== "gone",
  })),
  projectCount,
  isPartial: false,
  sessions: 24,
  minSessions: 10,
  scopeMinUses: 3,
  // 11월 5일 정오 UTC: 어느 시간대에서도 같은 날짜.
  lastUsed: projects.length > 0 ? LAST_USED : null,
  lastProject: projects[0]?.[0] ?? null,
});

const claudePlugin = (name: string, extra: Partial<CoachSwitch>) =>
  sw(`claude/plugin:${name}`, {
    format: "json",
    file: "/h/.claude/settings.json",
    isProposed: false,
    scope: { user: true, projects: [] },
    ...extra,
  });

/** Claude: 안 쓴 것(끄기 제안), 여러 곳에서 쓴 것(그대로), 두 곳에서만 쓴 것(옮기기), 프로젝트에서만 켠 것. */
const CLAUDE_SWITCHES: CoachSwitch[] = [
  claudePlugin("idle@m", { isProposed: true, usage: usage([]) }),
  claudePlugin("busy@m", {
    usage: usage(
      [
        ["kcal", 12],
        ["foo", 3],
        ["bar", 2],
      ],
      5,
    ),
  }),
  claudePlugin("niche@m", {
    tokens: 700,
    usage: usage([
      ["kcal", 4],
      ["gone", 1],
    ]),
    scopeMove: { projects: [{ name: "kcal", path: "/h/work/kcal" }], skipped: ["gone"] },
  }),
  claudePlugin("moved@m", {
    enabled: false,
    tokens: null,
    usage: usage([["kcal", 2]]),
    scope: {
      user: false,
      projects: [{ name: "kcal", path: "/h/work/kcal", file: "local", enabled: true }],
    },
  }),
];

const MOVE_PREVIEW: ScopeMovePreview = {
  target: "claude/plugin:niche@m",
  changes: [
    {
      file: "/h/work/kcal/.claude/settings.local.json",
      location: 'enabledPlugins."niche@m"',
      before: null,
      after: "true",
      isNewFile: true,
    },
    {
      file: "/h/.claude/settings.json",
      location: 'enabledPlugins."niche@m"',
      before: '"niche@m": true',
      after: '"niche@m": false',
      isNewFile: false,
    },
  ],
  skipped: ["gone"],
  beforeHash: "0123456789abcdef",
};

const MOVED: AppliedSwitch = {
  id: "9",
  at: "2026-11-13T12:00:00Z",
  action: "apply",
  target: "claude/plugin-scope:niche@m",
  agent: "claude",
  name: "niche@m: 프로젝트에서만 켬(kcal)",
  file: "/h/.claude/settings.json",
  backup: "/h/.claude/settings.json.kcal-20261113T120000000Z.bak",
  enabled: false,
  tokens: 700,
  afterHash: "fedcba9876543210",
  undoCommand: "cp 'b' '/h/.claude/settings.json' && rm '/h/work/kcal/.claude/settings.local.json'",
  undoOf: null,
  files: [
    {
      file: "/h/work/kcal/.claude/settings.local.json",
      backup: null,
      afterHash: "0000000000000000",
      createdDir: true,
    },
  ],
};

const PREVIEW: SwitchPreview = {
  target: "codex/plugin:big@m",
  enabled: false,
  file: "/h/.codex/config.toml",
  location: '[plugins."big@m"] 안 2번째 줄',
  before: "enabled = true",
  after: "enabled = false",
  beforeHash: "0123456789abcdef",
  changed: true,
};

const APPLIED: AppliedSwitch = {
  id: "1",
  at: "2026-11-05T12:00:00Z",
  action: "apply",
  target: "codex/plugin:big@m",
  agent: "codex",
  name: "big@m",
  file: "/h/.codex/config.toml",
  backup: "/h/.codex/config.toml.kcal-20261105T120000000Z.bak",
  enabled: false,
  tokens: 900,
  afterHash: "fedcba9876543210",
  undoCommand: "cp '/h/.codex/config.toml.kcal-20261105T120000000Z.bak' '/h/.codex/config.toml'",
  undoOf: null,
};

const RATES = {
  computedAt: new Date().toISOString(),
  codex: {
    status: "estimated",
    r2: 0.9,
    windows: 10,
    excludedWindows: 0,
    perPercent: { cachedInput: 1000, uncachedInput: null, output: null },
    foldRange: {
      cachedInput: { min: 900, max: 1100 },
      uncachedInput: { min: null, max: null },
      output: { min: null, max: null },
    },
    pattern: {
      days: 14,
      windows: 10,
      requests: { median: 100, p90: 150 },
      misses: { median: 0, p90: 0 },
    },
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
} satisfies RateSnapshot;

type Calls = { cmd: string; args: unknown }[];

function mock(overrides: Record<string, (args: unknown) => unknown> = {}): Calls {
  const calls: Calls = [];
  mockIPC((cmd, args) => {
    calls.push({ cmd, args });
    if (overrides[cmd]) return overrides[cmd](args);
    if (cmd === "get_rates") return RATES;
    if (cmd === "preview_switch") return PREVIEW;
    if (cmd === "apply_switch") return APPLIED;
    return undefined;
  });
  return calls;
}

const proposals = () => screen.getByRole("group", { name: /^끄기 제안/ });
/** 줄의 펼치기 버튼(이름 · 종류 · ~토큰을 담는다). 끄기 버튼("<이름> 끄기")과 겹치지 않게 aria-expanded로 고른다. */
const toggleOf = (root: HTMLElement, name: string) =>
  [...root.querySelectorAll<HTMLElement>("button[aria-expanded]")].find((b) =>
    b.textContent?.startsWith(name),
  ) as HTMLElement;
const rowsOf = (group: HTMLElement) =>
  within(group)
    .getAllByRole("listitem")
    .filter((li) => li.parentElement?.closest("li") === null);

function renderPanel(
  agent: Agent,
  {
    log = [],
    onChanged = () => {},
    switches = SWITCHES,
  }: { log?: AppliedSwitch[]; onChanged?: () => void; switches?: CoachSwitch[] } = {},
) {
  return render(
    <CoachPanel
      agent={agent}
      switches={switches}
      log={log}
      onChanged={onChanged}
      onShowEffect={() => {}}
    />,
  );
}

describe("CoachPanel", () => {
  it("lists only that agent's proposals in order with the Codex 5-hour share", async () => {
    mock();
    renderPanel("codex");
    const rows = rowsOf(proposals());
    expect(rows.map((r) => r.textContent)).toEqual([
      expect.stringContaining("big@m"),
      expect.stringContaining("small"),
    ]);
    // 900 × 100 / 1000 = 90%: 펼쳐야 보인다.
    expect(rows[0].textContent).not.toContain("5시간 한도");
    fireEvent.click(toggleOf(rows[0], "big@m"));
    await within(proposals()).findByText(/5시간 한도 ~90%/);
    expect(screen.queryByText("docs")).toBeNull();
    // 에이전트 칸 안이라 줄마다 에이전트 이름 표지를 붙이지 않는다(사용 기록 없음 안내 문장 안의 이름은 별개).
    expect(within(rows[0]).queryByText("Codex")).toBeNull();
    expect(within(rows[0]).getByText(/Codex는 쓴 횟수를 kcal이 볼 수 없어서/)).toBeTruthy();
  });

  it("shows the empty state and folds the rest under leave-as-is for an agent with nothing proposed", () => {
    mock();
    renderPanel("kiro");
    expect(screen.getByText(/지금 끄면 아낄 수 있는 스위치가 없어요/)).toBeTruthy();
    expect(screen.queryByRole("group", { name: /^끄기 제안/ })).toBeNull();
    const keep = screen.getByText("그대로 둠 · 1개").closest("details") as HTMLDetailsElement;
    expect(keep.open).toBe(false);
    expect(within(keep).getByText("알 수 없음 · 쓴 횟수 모름")).toBeTruthy();
    expect(screen.queryByText("big@m")).toBeNull();
  });

  it("previews the line, applies only after confirm, then shows backup, undo and restart note", async () => {
    const calls = mock();
    renderPanel("codex");
    fireEvent.click(await screen.findByRole("button", { name: "big@m 끄기" }));
    const panel = await screen.findByRole("group", { name: "바꿀 내용 확인" });
    await within(panel).findByText("- enabled = true");
    expect(within(panel).getByText("+ enabled = false")).toBeTruthy();
    expect(calls.some((c) => c.cmd === "apply_switch")).toBe(false);

    fireEvent.click(within(panel).getByRole("button", { name: "확인하고 끄기" }));
    const done = await screen.findByRole("group", { name: "적용 결과" });
    expect(within(done).getByText(ko.coach.restart)).toBeTruthy();
    expect(within(done).getByText(/^백업: /)).toBeTruthy();
    expect(within(done).getByText(APPLIED.undoCommand)).toBeTruthy();
    expect(within(done).getByRole("button", { name: "되돌리기" })).toBeTruthy();
    expect(calls.find((c) => c.cmd === "apply_switch")?.args).toEqual({
      target: "codex/plugin:big@m",
      enabled: false,
      beforeHash: PREVIEW.beforeHash,
    });
  });

  it("returns focus to the row's action button when a Diagnose panel closes with Escape", async () => {
    mock();
    renderPanel("codex");
    fireEvent.click(await screen.findByRole("button", { name: "big@m 끄기" }));
    const panel = await screen.findByRole("group", { name: "바꿀 내용 확인" });
    await waitFor(() => expect(document.activeElement).toBe(panel));
    fireEvent.keyDown(panel, { key: "Escape" });
    await waitFor(() =>
      expect(document.activeElement).toBe(screen.getByRole("button", { name: "big@m 끄기" })),
    );
  });

  it("shows instruction instead of preview for Claude MCP", async () => {
    const calls = mock();
    renderPanel("claude");
    fireEvent.click(await screen.findByRole("button", { name: "docs 끄는 법" }));
    expect(screen.getByText(/\/mcp를 열고/)).toBeTruthy();
    expect(calls.some((c) => c.cmd === "preview_switch")).toBe(false);
  });

  it("shows the refusal reason when apply is rejected and keeps the preview", async () => {
    const message =
      "미리보기 뒤에 설정 파일이 바뀌었어요. 아무것도 바꾸지 않았어요. 다시 미리 보세요.";
    mock({ apply_switch: () => Promise.reject({ kind: "invalid_input", message }) });
    renderPanel("codex");
    fireEvent.click(await screen.findByRole("button", { name: "big@m 끄기" }));
    const panel = await screen.findByRole("group", { name: "바꿀 내용 확인" });
    await within(panel).findByText("- enabled = true");
    fireEvent.click(within(panel).getByRole("button", { name: "확인하고 끄기" }));
    expect((await screen.findByRole("alert")).textContent).toBe(message);
    expect(screen.getByRole("group", { name: "바꿀 내용 확인" })).toBeTruthy();
  });

  it("shows the reason and returns to the proposal list when the preview is rejected", async () => {
    const message = "설정 파일을 읽지 못했어요.";
    mock({ preview_switch: () => Promise.reject({ kind: "invalid_input", message }) });
    renderPanel("codex");
    fireEvent.click(await screen.findByRole("button", { name: "big@m 끄기" }));
    expect((await screen.findByRole("alert")).textContent).toBe(message);
    expect(screen.queryByRole("group", { name: "바꿀 내용 확인" })).toBeNull();
    expect(screen.getByRole("button", { name: "big@m 끄기" })).toBeTruthy();
  });

  it("shows the reason and returns to the proposal list when the scope-move preview is rejected", async () => {
    const message = "프로젝트 설정을 읽지 못했어요.";
    mock({ preview_scope_move: () => Promise.reject({ kind: "invalid_input", message }) });
    renderPanel("claude", { switches: CLAUDE_SWITCHES });
    fireEvent.click(screen.getByRole("button", { name: "niche@m 프로젝트에서만 켜기" }));
    expect((await screen.findByRole("alert")).textContent).toBe(message);
    expect(screen.queryByRole("group", { name: "바꿀 내용 확인" })).toBeNull();
    expect(screen.getByRole("button", { name: "niche@m 프로젝트에서만 켜기" })).toBeTruthy();
  });

  it("shows the reason, keeps the list and does not ask the tab to read again when undo is rejected", async () => {
    const message = "되돌릴 백업을 찾지 못했어요.";
    mock({ undo_switch: () => Promise.reject({ kind: "not_found", message }) });
    const onChanged = vi.fn();
    renderPanel("codex", { log: [APPLIED], onChanged });
    const recent = screen.getByRole("group", { name: "최근 바꾼 것" });
    fireEvent.click(within(recent).getByRole("button", { name: "되돌리기" }));
    expect((await screen.findByRole("alert")).textContent).toBe(message);
    expect(onChanged).not.toHaveBeenCalled();
    expect(screen.getByRole("group", { name: "최근 바꾼 것" })).toBeTruthy();
  });

  it("offers undo for recent applies of that agent, then asks the tab to read again", async () => {
    const calls = mock({
      undo_switch: () => ({ ...APPLIED, id: "2", action: "undo", enabled: true, undoOf: "1" }),
    });
    let changed = 0;
    renderPanel("codex", { log: [APPLIED], onChanged: () => (changed += 1) });
    const recent = screen.getByRole("group", { name: "최근 바꾼 것" });
    fireEvent.click(within(recent).getByRole("button", { name: "되돌리기" }));
    await screen.findByRole("heading", { name: "big@m 되돌림" });
    expect(screen.getByRole("status").textContent).toBe("big@m 되돌림");
    expect(calls.find((c) => c.cmd === "undo_switch")?.args).toEqual({ id: "1" });
    expect(changed).toBe(1);
  });

  it("tops the list with the suggested total, its basis, and a how-to-read toggle", () => {
    mock();
    renderPanel("claude", { switches: CLAUDE_SWITCHES });
    // 끄기 제안 idle@m 900 + 프로젝트에서만 켜기 niche@m 700(팝오버 입구 줄과 같은 합).
    expect(screen.getByText("~1,600")).toBeTruthy();
    expect(screen.getByText("2개를 바꾸면 세션마다 이만큼 덜 실려요")).toBeTruthy();
    expect(screen.getByText("최근 30일 세션 24개 기준")).toBeTruthy();
    const legend = ko.coach.legendTokens + ko.coach.legendClaude + ko.coach.evidenceSource;
    expect(screen.queryByText(legend)).toBeNull();
    const howTo = screen.getByRole("button", { name: ko.coach.howToRead });
    expect(howTo.getAttribute("aria-expanded")).toBe("false");
    fireEvent.click(howTo);
    expect(screen.getByText(legend)).toBeTruthy();
  });

  it("sorts switches into turn-off, project-only and a folded leave-as-is group with dots and words", () => {
    mock();
    const once = claudePlugin("once@m", { usage: usage([["side-app", 1]]) });
    renderPanel("claude", { switches: [...CLAUDE_SWITCHES, once] });
    const names = (group: HTMLElement) =>
      rowsOf(group).map((li) => li.textContent?.match(/^[a-z]+@m/)?.[0]);
    const off = screen.getByRole("group", { name: "끄기 제안 · 30일 동안 안 씀" });
    expect(names(off)).toEqual(["idle@m"]);
    const move = screen.getByRole("group", { name: "kcal 프로젝트에서만 켜기 제안" });
    expect(names(move)).toEqual(["niche@m"]);
    const keep = screen.getByText("그대로 둠 · 3개").closest("details") as HTMLDetailsElement;
    expect(keep.open).toBe(false);
    // 펼치면 이름 + 짧은 사유(배지 글자 재사용) 한 줄씩.
    expect(rowsOf(keep).map((li) => li.textContent)).toEqual([
      "busy@m쓰는 중 · 17번~900",
      "moved@m꺼짐켜기",
      "once@m사용 적음 · 1번~900",
    ]);
    // 쓰는 중이면 끄기 버튼이 없고, 사유 툴팁에 근거 문장·프로젝트별 횟수가 있다.
    const busy = rowsOf(keep)[0];
    expect(within(busy).queryByRole("button")).toBeNull();
    expect(within(busy).getByText("쓰는 중 · 17번").getAttribute("title")).toBe(
      "30일 · Claude 세션 24개 중 17번 · 마지막 11월 5일 kcal\n/h/work/kcal 12×\n/h/work/foo 3×\n/h/work/bar 2×",
    );
    expect(within(rowsOf(keep)[2]).getByText("사용 적음 · 1번").getAttribute("title")).toContain(
      ko.coach.tooFewUses,
    );
    // 묶음 제목 앞 점은 장식(글자가 뜻을 말한다).
    expect(off.querySelector("h2 > span[aria-hidden]")).toBeTruthy();
  });

  it("keeps suggestion rows on one line, folded by default, and opens only the pressed row as a card", () => {
    mock();
    const contents = {
      skills: { names: ["label", "review", "summarize"], count: 4 },
      agents: { names: ["checker"], count: 1 },
      commands: { names: [], count: 0 },
      mcpServers: { names: ["tracker"], count: 1 },
    };
    const switches = CLAUDE_SWITCHES.map((s) => (s.name === "idle@m" ? { ...s, contents } : s));
    renderPanel("claude", { switches });
    const idle = rowsOf(proposals())[0];
    const niche = rowsOf(screen.getByRole("group", { name: /프로젝트에서만 켜기 제안$/ }))[0];
    const toggle = toggleOf(idle, "idle@m");
    expect(toggle.textContent).toBe("idle@m플러그인~900");
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
    expect(idle.textContent).not.toContain(ko.coach.chipUndo);
    expect(idle.textContent).not.toContain(ko.coach.losesOff);
    // 접힌 줄의 버튼은 모두 보조 모양.
    const off = within(idle).getByRole("button", { name: "idle@m 끄기" });
    expect(off.className).toContain("border-border");

    fireEvent.click(toggle);
    expect(toggle.getAttribute("aria-expanded")).toBe("true");
    for (const chip of ["30일 0번", "세션 24개 전부 확인", "범위: 전체", ko.coach.chipUndo]) {
      expect(within(idle).getByText(chip)).toBeTruthy();
    }
    expect(
      within(idle).getByText(
        "끄면 함께 꺼지는 것: 스킬 4개(label, review, summarize 외 1개) · 에이전트 1개(checker) · MCP 서버 1개(tracker)",
      ),
    ).toBeTruthy();
    // 펼친 줄의 버튼만 강조.
    expect(off.className).toContain("bg-primary");
    expect(
      within(niche).getByRole("button", { name: "niche@m 프로젝트에서만 켜기" }).className,
    ).not.toContain("bg-primary");

    // 다른 줄을 펼치면 앞 줄은 접힌다(한 번에 한 줄).
    fireEvent.click(toggleOf(niche, "niche@m"));
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
    expect(within(niche).getByText("kcal에서 4번")).toBeTruthy();
    expect(within(niche).getByText("마지막 11월 5일")).toBeTruthy();
    // 폴더를 못 읽은 플러그인(contents null)은 함께 꺼지는 줄을 숨긴다.
    expect(niche.textContent).not.toContain(ko.coach.losesMove);
  });

  it("hides the turns-off-with-it line for MCP switches and marks partial history on the card", () => {
    mock();
    const contents = {
      skills: { names: ["x"], count: 1 },
      agents: { names: [], count: 0 },
      commands: { names: [], count: 0 },
      mcpServers: { names: [], count: 0 },
    };
    const mcp = sw("claude/mcp:docs", {
      format: "manual",
      file: null,
      usage: { ...usage([]), isPartial: true },
      // Rust는 MCP에 넣지 않지만, 들어와도 화면은 플러그인만 보인다.
      contents,
    });
    renderPanel("claude", { switches: [mcp] });
    const row = rowsOf(proposals())[0];
    fireEvent.click(toggleOf(row, "docs"));
    expect(within(row).getByText(ko.coach.partial)).toBeTruthy();
    expect(row.textContent).not.toContain(ko.coach.losesOff);
    // 직접 끄는 항목은 되돌리기 칩이 없다.
    expect(row.textContent).not.toContain(ko.coach.chipUndo);
  });

  it("opens a row by clicking its name, and its action button leaves the row as it is", () => {
    const onPick = vi.fn();
    render(
      <SwitchLists
        agent="claude"
        isMissing={false}
        switches={CLAUDE_SWITCHES}
        rate={undefined}
        onPick={onPick}
        onMove={() => {}}
      />,
    );
    const idle = rowsOf(proposals())[0];
    const toggle = toggleOf(idle, "idle@m");
    const action = within(idle).getByRole("button", { name: "idle@m 끄기" });
    fireEvent.click(action);
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
    fireEvent.click(within(idle).getByText("idle@m"));
    expect(toggle.getAttribute("aria-expanded")).toBe("true");
    fireEvent.click(action);
    expect(toggle.getAttribute("aria-expanded")).toBe("true");
    expect(onPick).toHaveBeenCalledTimes(2);
    fireEvent.click(within(idle).getByText("idle@m"));
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
  });

  it("opens rows from the keyboard", () => {
    mock();
    renderPanel("claude", { switches: CLAUDE_SWITCHES });
    const toggle = toggleOf(rowsOf(proposals())[0], "idle@m");
    expect(toggle.tagName).toBe("BUTTON");
    toggle.focus();
    expect(document.activeElement).toBe(toggle);
    // 네이티브 button이라 Enter·Space는 click으로 들어온다.
    fireEvent.click(toggle);
    expect(toggle.getAttribute("aria-expanded")).toBe("true");
    expect(toggle.getAttribute("aria-controls")).toBe("claude/plugin:idle@m-details");
    expect(document.getElementById("claude/plugin:idle@m-details")).toBeTruthy();
  });

  it("says kcal can't tell, instead of unused, when there are too few Claude sessions", () => {
    mock();
    const few = (sessions: number) => claudePlugin("new@m", { usage: { ...usage([]), sessions } });
    const { unmount } = renderPanel("claude", { switches: [few(3)] });
    const reason = screen.getByText("알 수 없음 · 기록 적음(세션 3개)");
    expect(reason.getAttribute("title")).toBe(
      "Claude 기록이 적어서 쓰는지 알 수 없어요 (30일 동안 세션 3개)",
    );
    expect(screen.queryByText(/안 씀/)).toBeNull();
    unmount();
    renderPanel("claude", { switches: [few(0)] });
    expect(screen.getByText("알 수 없음 · 기록 적음(세션 0개)").getAttribute("title")).toBe(
      "최근 30일에 Claude 세션이 없어서 쓰는지 알 수 없어요",
    );
  });

  it("shows Codex session evidence and keeps plugins unmeasured", () => {
    mock();
    renderPanel("codex", {
      switches: [
        sw("codex/mcp:few", { isProposed: false, usage: { ...usage([]), sessions: 3 } }),
        sw("codex/mcp:idle", { usage: usage([]) }),
        sw("codex/plugin:unknown", { isProposed: false }),
      ],
    });
    expect(screen.getByText("알 수 없음 · 기록 적음(세션 3개)").getAttribute("title")).toBe(
      "Codex 기록이 적어서 쓰는지 알 수 없어요 (30일 동안 세션 3개)",
    );
    expect(within(proposals()).getByText("idle")).toBeTruthy();
    expect(en.coach.fewSessions(30, 3, "Codex")).toBe(
      "Not enough Codex history to tell if you use this (3 sessions in 30 days)",
    );
    expect(en.coach.fewSessions(30, 0, "Codex")).toContain("No Codex sessions found");
    expect(en.coach.unused(30, 24, "Codex")).toContain("24 Codex sessions");
    const unknown = screen.getByText("unknown").closest("li")!;
    expect(within(unknown).getByText("알 수 없음 · 쓴 횟수 모름").getAttribute("title")).toBe(
      ko.coach.noUsageLog("Codex"),
    );
  });

  it("moves a plugin to the projects that used it in one preview, one confirm and one undo", async () => {
    const calls = mock({
      preview_scope_move: () => MOVE_PREVIEW,
      apply_scope_move: () => MOVED,
    });
    renderPanel("claude", { switches: CLAUDE_SWITCHES });
    const movable = screen.getByRole("group", { name: "kcal 프로젝트에서만 켜기 제안" });
    expect(rowsOf(movable)).toHaveLength(1);
    fireEvent.click(within(movable).getByRole("button", { name: "niche@m 프로젝트에서만 켜기" }));
    const panel = await screen.findByRole("group", { name: "바꿀 내용 확인" });
    await within(panel).findByText(/폴더가 없어져 건너뛰어요: gone/);
    expect(within(panel).getByText(/settings\.local\.json \(새 파일\)/)).toBeTruthy();
    expect(within(panel).getByText('+ "niche@m": false')).toBeTruthy();
    expect(calls.find((c) => c.cmd === "preview_scope_move")?.args).toEqual({
      target: "claude/plugin:niche@m",
    });
    expect(calls.some((c) => c.cmd === "apply_scope_move")).toBe(false);

    fireEvent.click(within(panel).getByRole("button", { name: "확인하고 옮기기" }));
    const done = await screen.findByRole("group", { name: "적용 결과" });
    // 기록 이름 그대로(뒤에 "켬/끔"을 붙이지 않는다).
    expect(within(done).getByText(MOVED.name)).toBeTruthy();
    expect(within(done).getByText(MOVED.undoCommand)).toBeTruthy();
    expect(calls.find((c) => c.cmd === "apply_scope_move")?.args).toEqual({
      target: "claude/plugin:niche@m",
      beforeHash: MOVE_PREVIEW.beforeHash,
    });
  });

  it("moves a Codex MCP server to the projects that used it, with the committed-file note and Codex wording", async () => {
    const calls = mock({ preview_scope_move: () => MOVE_PREVIEW });
    const server = sw("codex/mcp:crg", {
      tokens: null,
      isProposed: false,
      usage: usage([["kcal", 5]]),
      scopeMove: { projects: [{ name: "kcal", path: "/h/work/kcal" }], skipped: [] },
    });
    renderPanel("codex", { switches: [server] });
    const movable = screen.getByRole("group", { name: "kcal 프로젝트에서만 켜기 제안" });
    fireEvent.click(within(movable).getByRole("button", { name: "crg 프로젝트에서만 켜기" }));
    const panel = await screen.findByRole("group", { name: "바꿀 내용 확인" });
    expect(within(panel).getByText(/커밋될 수 있어요/)).toBeTruthy();
    expect(calls.find((c) => c.cmd === "preview_scope_move")?.args).toEqual({
      target: "codex/mcp:crg",
    });
  });

  it("renders a Codex plugin scope row and calls preview, apply and undo wrappers", async () => {
    const target = "codex/plugin:browser@openai-bundled";
    const moved = {
      ...MOVED,
      target: "codex/plugin-scope:browser@openai-bundled",
      agent: "codex" as const,
    };
    const calls = mock({
      preview_scope_move: () => ({ ...MOVE_PREVIEW, target }),
      apply_scope_move: () => moved,
      undo_switch: () => ({ ...moved, action: "undo", undoOf: moved.id }),
    });
    renderPanel("codex", {
      switches: [
        sw(target, {
          isProposed: false,
          usage: usage([["kcal", 5]]),
          scopeMove: { projects: [{ name: "kcal", path: "/h/work/kcal" }], skipped: [] },
        }),
      ],
    });
    fireEvent.click(
      screen.getByRole("button", { name: "browser@openai-bundled 프로젝트에서만 켜기" }),
    );
    const panel = await screen.findByRole("group", { name: "바꿀 내용 확인" });
    expect(within(panel).getByText(/커밋될 수 있어요/)).toBeTruthy();
    expect(calls.find((c) => c.cmd === "preview_scope_move")?.args).toEqual({ target });
    fireEvent.click(within(panel).getByRole("button", { name: "확인하고 옮기기" }));
    const done = await screen.findByRole("group", { name: "적용 결과" });
    expect(within(done).getByText(moved.name)).toBeTruthy();
    expect(calls.find((c) => c.cmd === "apply_scope_move")?.args).toEqual({
      target,
      beforeHash: MOVE_PREVIEW.beforeHash,
    });
    fireEvent.click(within(done).getByRole("button", { name: "되돌리기" }));
    await waitFor(() =>
      expect(calls.find((c) => c.cmd === "undo_switch")?.args).toEqual({ id: moved.id }),
    );
  });

  it("hides a skill's description instead of turning it off, with skill wording and no backup line for a new file", async () => {
    const calls = mock({
      preview_switch: () => ({
        ...PREVIEW,
        target: "claude/project-skill:old",
        file: "/h/work/kcal/.claude/settings.local.json",
        location: "skillOverrides.old",
        before: null,
        after: '"old": "user-invocable-only"',
      }),
      apply_switch: () => ({
        ...APPLIED,
        target: "claude/project-skill:old",
        agent: "claude",
        name: "old",
        backup: "",
      }),
    });
    const skill = (name: string, extra: Partial<CoachSwitch>) =>
      sw(`claude/project-skill:${name}`, {
        kind: "projectSkill",
        tokens: null,
        loading: null,
        format: "json",
        file: "/h/work/kcal/.claude/settings.local.json",
        skill: { state: null, project: "kcal" },
        usage: usage([]),
        ...extra,
      });
    renderPanel("claude", {
      switches: [
        skill("old", { isProposed: true }),
        skill("mine", {
          enabled: false,
          isProposed: false,
          skill: { state: "name-only", project: "kcal" },
          format: "manual",
          file: null,
          instruction: "직접 정한 값이에요.",
        }),
      ],
    });
    const hide = screen.getByRole("group", { name: /^설명 숨기기 제안/ });
    expect(screen.queryByText(/끄기 제안/)).toBeNull();
    // 크기를 모르면 큰 토큰 숫자를 만들지 않는다.
    expect(screen.queryByText("토큰")).toBeNull();
    // 직접 정한 값은 현재 상태만 보이고 버튼이 없다.
    expect(screen.queryByRole("button", { name: /mine/ })).toBeNull();
    fireEvent.click(within(hide).getByRole("button", { name: "old 설명 숨기기" }));
    const panel = await screen.findByRole("group", { name: "바꿀 내용 확인" });
    expect(within(panel).getByText("old 설명 숨기기")).toBeTruthy();
    expect(within(panel).getByText('+ "old": "user-invocable-only"')).toBeTruthy();
    fireEvent.click(within(panel).getByRole("button", { name: "확인하고 숨기기" }));
    const done = await screen.findByRole("group", { name: "적용 결과" });
    expect(within(done).getByText("old 설명 숨김")).toBeTruthy();
    expect(within(done).queryByText(/백업:/)).toBeNull();
    expect(calls.find((c) => c.cmd === "apply_switch")?.args).toMatchObject({
      target: "claude/project-skill:old",
      enabled: false,
    });
  });

  it("lists at most three open applies, skipping undone ones and optimization-check fixes", () => {
    mock();
    const applied = (id: string, extra: Partial<AppliedSwitch> = {}): AppliedSwitch => ({
      ...APPLIED,
      id,
      name: `p${id}@m`,
      ...extra,
    });
    const log = [
      applied("1"),
      applied("2", { action: "undo", undoOf: "1" }),
      applied("3", { target: "claude/check:x", agent: "codex" }),
      applied("4"),
      applied("5"),
      applied("6"),
      applied("7"),
    ];
    const onShowEffect = vi.fn();
    render(
      <CoachPanel
        agent="codex"
        switches={SWITCHES}
        log={log}
        onChanged={() => {}}
        onShowEffect={onShowEffect}
      />,
    );
    const recent = screen.getByRole("group", { name: "최근 바꾼 것" });
    expect(
      within(recent)
        .getAllByRole("listitem")
        .map((li) => li.textContent),
    ).toEqual([
      expect.stringContaining("p4@m"),
      expect.stringContaining("p5@m"),
      expect.stringContaining("p6@m"),
    ]);
    fireEvent.click(within(recent).getAllByRole("button", { name: ko.coach.showEffect })[0]);
    expect(onShowEffect).toHaveBeenCalledTimes(1);
  });

  it("does not show another agent's recent applies", () => {
    mock();
    renderPanel("kiro", { log: [APPLIED] });
    expect(screen.queryByRole("group", { name: "최근 바꾼 것" })).toBeNull();
  });
});

it("renders MCP guide commands and close-first line without invoking preview or apply", async () => {
  const invoke = vi.fn();
  mockIPC(invoke);
  const commands = [
    "cd '/work/kcal' && claude mcp add-json -s local 'docs' '{}'",
    "claude mcp remove -s user 'docs'",
  ];
  const undoCommands = [
    "claude mcp add-json -s user 'docs' '{}'",
    "cd '/work/kcal' && claude mcp remove -s local 'docs'",
  ];
  render(
    <CoachPanel
      agent="claude"
      switches={[
        sw("claude/mcp:docs", {
          format: "manual",
          isProposed: false,
          scopeMove: { projects: [{ name: "kcal", path: "/work/kcal" }], skipped: [] },
          scopeGuide: { commands, undoCommands },
        }),
      ]}
      log={[]}
      onChanged={vi.fn()}
      onShowEffect={vi.fn()}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: `docs ${ko.coach.moveToProjects}` }));
  const panel = screen.getByRole("group", { name: ko.coach.moveTitle("docs") });
  expect(within(panel).getByText(ko.coach.mcpCloseFirst)).toBeTruthy();
  expect(panel.children[1]?.textContent).toBe(ko.coach.mcpCloseFirst);
  for (const command of [...commands, ...undoCommands])
    expect(within(panel).getByText(command)).toBeTruthy();
  expect(within(panel).getAllByRole("button", { name: ko.coach.copyCommand })).toHaveLength(4);
  expect(invoke.mock.calls.some(([cmd]) => /preview|apply/.test(String(cmd)))).toBe(false);
});

it("renders Kiro JSON guide and session count without invoking preview or apply", () => {
  const invoke = vi.fn();
  mockIPC(invoke);
  const commands = [
    "Paste into .kiro/settings/mcp.json",
    '{"mcpServers":{"docs":{"command":"node","disabled":false}}}',
    'Set user "disabled": true',
  ];
  render(
    <CoachPanel
      agent="kiro"
      switches={[
        sw("kiro/mcp:docs", {
          format: "json",
          isProposed: false,
          usage: usage([["kcal", 3]]),
          scopeMove: { projects: [{ name: "kcal", path: "/work/kcal" }], skipped: [] },
          scopeGuide: { commands, undoCommands: ["Enable user then remove project entry"] },
        }),
      ]}
      log={[]}
      onChanged={vi.fn()}
      onShowEffect={vi.fn()}
    />,
  );
  expect(screen.getByText(ko.coach.basis(30, 24))).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: `docs ${ko.coach.moveToProjects}` }));
  const panel = screen.getByRole("group", { name: ko.coach.moveTitle("docs") });
  expect(within(panel).getByText(ko.coach.kiroMcpCloseFirst)).toBeTruthy();
  expect(within(panel).getByText(ko.coach.kiroMcpGuideNote)).toBeTruthy();
  for (const command of commands) expect(within(panel).getByText(command)).toBeTruthy();
  expect(within(panel).getAllByRole("button", { name: ko.coach.copyCommand })).toHaveLength(1);
  expect(invoke.mock.calls.some(([cmd]) => /preview|apply/.test(String(cmd)))).toBe(false);
});

it("shows Kiro session count when zero-use evidence is insufficient", () => {
  mockIPC(vi.fn());
  render(
    <CoachPanel
      agent="kiro"
      switches={[
        sw("kiro/mcp:docs", {
          format: "json",
          isProposed: false,
          usage: { ...usage([]), sessions: 2 },
        }),
      ]}
      log={[]}
      onChanged={vi.fn()}
      onShowEffect={vi.fn()}
    />,
  );
  expect(screen.getByTitle(ko.coach.fewSessions(30, 2, "Kiro"))).toBeTruthy();
});
