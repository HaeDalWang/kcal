import { describe, expect, it } from "vitest";

import {
  alertCount,
  alertLabel,
  appliedName,
  checkChips,
  checkCopy,
  handoffErrorText,
  handoffGate,
  hiddenText,
  modelLabel,
  modelRows,
  noChecksText,
  remainingText,
  rowMeta,
  ruleProblemText,
  ruleRowCopy,
  unknownKeysText,
} from "@/lib/check-copy";
import { setLanguage } from "@/lib/i18n";
import type { CoachCheck } from "@/lib/ipc";
import type { EffortCount } from "@/lib/ipc-checks";
import { fixture, expectFields } from "@/test/ipc-fixture";
import {
  CODEX_FAST_CHECK,
  EFFORT_CHECK,
  SUBAGENT_EFFORT_CHECK,
  GIT_CHECK,
  HOOK_CHECK,
  LONG_CHECK,
  RULES_CHECK,
  RULES_PREVIEW,
  SKILL_CHECK,
} from "@/test/preview/checks-fixtures";

describe("modelLabel", () => {
  it("turns normalized model ids into family and version", () => {
    expect(modelLabel("claude-opus-5-5")).toBe("Opus 5.5");
    expect(modelLabel("claude-haiku-4-5")).toBe("Haiku 4.5");
    expect(modelLabel("claude-fable-5-1")).toBe("Fable 5.1");
    expect(modelLabel("claude-opus-5")).toBe("Opus 5");
    expect(modelLabel("gpt-5")).toBe("gpt-5");
  });
});

describe("checkCopy", () => {
  it("matches the Rust subagent effort fixture fields and effort counts", () => {
    const card = fixture("coach-subagent-effort") as CoachCheck;
    if (card.facts.type !== "subagentEffort") throw new Error("fixture");
    expectFields(
      card.facts,
      {
        type: "required",
        days: "required",
        heavyRequests: "required",
        subagentRequests: "required",
        byEffort: "required",
        agents: "required",
        recommended: "required",
      },
      "CheckFacts.subagentEffort",
    );
    card.facts.byEffort.forEach((count) =>
      expectFields<EffortCount>(count, { effort: "required", requests: "required" }, "EffortCount"),
    );
    expect(card.facts).toEqual(SUBAGENT_EFFORT_CHECK.facts);
    expect(checkCopy(card).headline).toContain("12건 중 3건");
  });

  it("shows measured subagent effort, preserves explicit effort and limits agent names", () => {
    const facts = SUBAGENT_EFFORT_CHECK.facts;
    if (facts.type !== "subagentEffort") throw new Error("fixture");
    const card = {
      ...SUBAGENT_EFFORT_CHECK,
      facts: { ...facts, agents: ["a", "b", "c", "d", "e", "f", "g"] },
    };
    const ko = checkCopy(card);
    expect(ko.headline).toBe("최근 14일 서브에이전트 요청 12건 중 3건이 xhigh 이상이었어요.");
    expect(ko.what).toContain("agent 파일의 effort는 그대로예요");
    expect(ko.note).toBe("새 세션부터 적용돼요.");
    expect(ko.mineList).toContain("xhigh 이상으로 요청한 에이전트: a, b, c, d, e 외 2개");
    expect(ko.actions.map((a) => [a.option, a.isRecommended])).toEqual([["set-high", true]]);
    setLanguage("en");
    const en = checkCopy(card);
    expect(en.headline).toBe(
      "In the last 14 days, 3 of 12 subagent requests ran at xhigh or above.",
    );
    expect(en.mineList.at(-1)).toContain("a, b, c, d, e and 2 more");
    expect(en.what).toContain("agent files stays unchanged");
  });
  it("this Mac effort card: Opus 5.5 ignores high and runs its own xhigh, nothing moves", () => {
    const copy = checkCopy(EFFORT_CHECK);
    expect(copy.title).toBe("생각 깊이 설정 한 줄이 지금 모델에서 무시돼요");
    expect(copy.headline).toBe(
      "Opus 5.5에서는 이 값(high)을 무시하고, 따로 정해 둔 값(xhigh)으로 돌아요.",
    );
    expect(copy.what).toBe("무시되는 옛 설정 한 줄을 지워요.");
    expect(copy.mine).toBe("없어요. 최근 30일에 쓴 모델 4개는 모두 지금과 같은 깊이로 돌아요.");
    expect(copy.actions.map((a) => [a.label, a.isRecommended])).toEqual([["옛 설정 지우기", true]]);
    expect(copy.keepReason).toBeNull();
  });

  it("effort card names moved models and offers saving the current model with its cost", () => {
    const facts = EFFORT_CHECK.facts as Extract<CoachCheck["facts"], { type: "effortLevel" }>;
    const check: CoachCheck = {
      ...EFFORT_CHECK,
      facts: {
        ...facts,
        value: "low",
        current: "claude-haiku-5-5",
        movedTo: ["claude-haiku-4-5"],
        models: facts.models.map((m) =>
          m.model === "claude-haiku-5-5" ? { ...m, requests: 99999 } : m,
        ),
      },
      options: [
        ...EFFORT_CHECK.options,
        {
          id: "remove-and-save-current",
          isRecommended: false,
          isWritable: true,
          instruction: null,
          changes: [],
        },
      ],
    };
    const copy = checkCopy(check);
    expect(copy.headline).toBe("Haiku 5.5에서는 이 값(low)을 무시하고 기본값(medium)으로 돌아요.");
    expect(copy.what).toBe(
      "무시되는 옛 설정 한 줄을 지워요. Haiku 4.5에는 지금처럼 돌도록 그 모델 자리에 low 값을 옮겨 적어요.",
    );
    expect(copy.actions[1]).toMatchObject({
      label: "Haiku 5.5도 low 값으로 저장하기",
      note: "덜 생각하는 만큼 한도를 덜 써요.",
    });
  });

  it("this Mac git card: names git-workflow.md, lists the lost trailer and recommends keep", () => {
    const copy = checkCopy(GIT_CHECK);
    expect(copy.headline).toBe("내 규칙(git-workflow.md)과 Claude Code 기본 안내가 같이 실려요.");
    expect(copy.mine).toBe("내 규칙은 그대로예요. 빠지는 건 Claude Code 쪽 3가지예요.");
    expect(copy.mineList).toEqual([
      "기본 커밋·PR 안내",
      "세션 시작 때 붙던 git 상태 요약(브랜치·바뀐 파일·최근 커밋)",
      "커밋 끝 서명 줄(Co-Authored-By). 내 규칙에 서명 줄이 직접 적혀 있지 않아서, 끄면 서명이 안 붙어요.",
    ]);
    expect(copy.keepReason).toBe("끄면 커밋 서명이 빠져서요.");
    expect(copy.actions.map((a) => [a.label, a.isRecommended])).toEqual([
      ["내 규칙만 남기기", false],
    ]);
  });

  it("git card with own trailer and a verify skill lists the pre-commit hint", () => {
    const facts = GIT_CHECK.facts as Extract<CoachCheck["facts"], { type: "gitInstructions" }>;
    const copy = checkCopy({
      ...GIT_CHECK,
      facts: { ...facts, hasOwnTrailer: true, preCommitSkills: ["verify"] },
    });
    expect(copy.mineList.at(-1)).toBe("커밋 전에 verify 스킬을 돌리라는 안내");
    expect(copy.mineList).toHaveLength(3);
    expect(copy.keepReason).toBe("커밋 전 검사 안내가 빠져서요.");
  });

  it("thinking cards say what still obeys the setting", () => {
    const check: CoachCheck = {
      ...EFFORT_CHECK,
      id: "D-claude-max-thinking-tokens",
      facts: {
        type: "maxThinkingTokens",
        value: 8000,
        days: 30,
        models: [],
        ignoredBy: ["claude-opus-5-5"],
        affected: ["claude-haiku-4-5"],
        unknownModels: [],
      },
      options: [
        { ...EFFORT_CHECK.options[0], isRecommended: false },
        { id: "keep", isRecommended: true, isWritable: false, instruction: null, changes: [] },
      ],
    };
    const copy = checkCopy(check);
    expect(copy.title).toBe("생각 토큰 한도 설정이 지금 모델에서 무시돼요");
    expect(copy.headline).toBe("Opus 5.5에서는 생각 양을 스스로 정해서 이 숫자를 무시해요.");
    expect(copy.mine).toBe(
      "Haiku 4.5에서는 이 설정이 생각 양을 8,000 토큰으로 묶고 있어요. 지우면 그 묶음이 풀려요.",
    );
    expect(copy.keepReason).not.toBeNull();
  });

  it("B cards: project hook is instruction only, same skill hides the personal copy", () => {
    const hook = checkCopy(HOOK_CHECK);
    expect(hook.headline).toBe("AGENTS.md ~1,240 토큰이 세션마다 두 번 실려요.");
    expect(hook.who).toContain("kcal이 고치지 않아요");
    expect(hook.actions[0]).toMatchObject({ isWritable: false });
    const skill = checkCopy(SKILL_CHECK);
    expect(skill.headline).toBe(
      "`playwright-cli` 스킬이 내 스킬 폴더와 플러그인 `playwright`에 둘 다 있어서, 설명이 매번 두 번 실려요.",
    );
    expect(skill.mine).toBe("없어요. 두 스킬 내용이 같아요.");
    expect(skill.better).toBe("세션마다 ~85 토큰을 아껴요.");
  });
});

describe("modelRows", () => {
  it("shows the value each recent model runs at and whether it ignores the setting", () => {
    expect(modelRows(EFFORT_CHECK.facts)).toEqual([
      { label: "Opus 5.5", requests: 9774, current: "xhigh", verdict: "무시" },
      { label: "Sonnet 5.5", requests: 38, current: "medium", verdict: "무시" },
      { label: "Haiku 4.5", requests: 19, current: "high", verdict: "적용" },
      { label: "Haiku 5.5", requests: 13, current: "medium", verdict: "무시" },
    ]);
    expect(modelRows(GIT_CHECK.facts)).toEqual([]);
  });
});

describe("appliedName", () => {
  it("shows check records by name only and switches with on/off", () => {
    const check = { name: "생각 깊이 옛 설정 정리", enabled: false };
    expect(appliedName({ ...check, target: "claude/check:D-claude-effort-level:remove" })).toBe(
      "생각 깊이 옛 설정 정리",
    );
    expect(appliedName({ name: "p@m", enabled: false, target: "codex/plugin:p@m" })).toBe("p@m 끔");
    expect(appliedName({ name: "p@m", enabled: true, target: "codex/plugin:p@m" })).toBe("p@m 켬");
    // 프로젝트에서만 켜기 기록은 이름이 이미 행동이라 "켬/끔"을 붙이지 않는다(Claude 플러그인·Codex MCP).
    const moved = "x: 프로젝트에서만 켬(a)";
    for (const target of ["claude/plugin-scope:x", "codex/mcp-scope:x", "codex/plugin-scope:x"])
      expect(appliedName({ name: moved, enabled: false, target })).toBe(moved);
  });
});

describe("hiddenText", () => {
  it("counts hidden cards and says nothing when none are hidden", () => {
    expect(hiddenText(2)).toBe("숨긴 카드 2개");
    expect(hiddenText(0)).toBeNull();
  });
});

describe("alertCount", () => {
  it("counts cards that recommend a change, not keep-recommended or hidden ones", () => {
    // 이 Mac: D(지우기 추천) 1장, C(그대로 두기 추천) 0장.
    expect(alertCount([EFFORT_CHECK, GIT_CHECK], ["claude"])).toBe(1);
    expect(alertCount([EFFORT_CHECK, GIT_CHECK, HOOK_CHECK, SKILL_CHECK], ["claude"])).toBe(3);
    expect(alertCount([{ ...EFFORT_CHECK, isDismissed: true }, GIT_CHECK], ["claude"])).toBe(0);
    expect(alertCount([], ["claude"])).toBe(0);
    expect(alertLabel(3)).toBe("점검할 것 3개");
  });

  it("counts only agents that are turned on", () => {
    expect(alertCount([EFFORT_CHECK], ["codex", "kiro"])).toBe(0);
  });

  it("counts the Codex fast-mode card in the Codex tab only", () => {
    expect(alertCount([EFFORT_CHECK, CODEX_FAST_CHECK], ["codex"])).toBe(1);
    expect(alertCount([EFFORT_CHECK, CODEX_FAST_CHECK], ["claude", "codex"])).toBe(2);
    expect(alertCount([CODEX_FAST_CHECK], ["claude", "kiro"])).toBe(0);
  });
});

describe("noChecksText", () => {
  it("says nothing to fix when supported agents have no matching cards", () => {
    expect(noChecksText("claude", "Claude")).toBe("지금 고칠 것이 없어요.");
    expect(noChecksText("codex", "Codex")).toBe("지금 고칠 것이 없어요.");
    expect(noChecksText("kiro", "Kiro")).toBe("지금 고칠 것이 없어요.");
  });

  it("says Codex was not found by its own name and command, without the Claude setup link", () => {
    expect(noChecksText("codex", "Codex", true)).toBe(
      "점검하지 못했어요. 이 컴퓨터에서 Codex를 찾지 못했어요. codex를 한 번 실행해 로그인한 뒤 새로고침하세요.",
    );
  });
});

describe("Codex fast-mode card", () => {
  const withFacts = (model: string | null, multiplier: number | null): CoachCheck => ({
    ...CODEX_FAST_CHECK,
    facts: { type: "codexFastMode", model, multiplier },
  });

  it("names the multiplier, answers the three questions and the loss with the Astra speed", () => {
    const copy = checkCopy(CODEX_FAST_CHECK);
    expect(copy.title).toBe("빠른 모드라 한도가 2배로 닳아요");
    expect(copy.headline).toBe("Codex가 빠른 모드로 저장돼 있어서, 같은 일에 한도를 2배 써요.");
    expect(copy.what).toBe("Codex에서 /fast를 한 번 입력해요.");
    expect(copy.mine).toContain("Standard로 돌아가요");
    expect(copy.who).toBe("당신이 Codex에서 직접 해요. kcal은 파일을 고치지 않아요.");
    expect(copy.actions).toEqual([
      expect.objectContaining({
        option: "standard",
        isRecommended: true,
        isWritable: false,
        instruction:
          "Codex에서 `/fast`를 한 번 더 입력하세요. Standard로 돌아가고, 그 선택은 Codex가 저장해요.",
        note: "잃는 것: 응답 생성 속도예요. GPT-6.2 Astra는 Standard보다 약 1.5배 빨라요(문서).",
      }),
    ]);
    expect(rowMeta(CODEX_FAST_CHECK)?.text).toBe("한도 2배");
    expect(checkChips(CODEX_FAST_CHECK)).toEqual(["모델 gpt-6.2-astra", "포함 사용량 2배(문서)"]);
  });

  it("keeps 2.5 and drops the speed number for other models", () => {
    const copy = checkCopy(withFacts("gpt-6.1-sol", 2.5));
    expect(copy.title).toBe("빠른 모드라 한도가 2.5배로 닳아요");
    expect(copy.actions[0].note).toBe("잃는 것: 응답 생성 속도예요.");
  });

  it("says only 'more' without a number when the model is not in the table", () => {
    const copy = checkCopy(withFacts(null, null));
    expect(copy.title).toBe("빠른 모드라 한도가 더 빨리 닳아요");
    expect(copy.headline).toBe("Codex가 빠른 모드로 저장돼 있어서, 같은 일에 한도를 더 써요.");
    expect(copy.actions[0].note).toBe("잃는 것: 응답 생성 속도예요.");
    expect(rowMeta(withFacts("gpt-9", null))?.text).toBe("한도 더 씀");
    expect(checkChips(withFacts(null, null))).toEqual([]);
  });

  it("reads the same in English", () => {
    setLanguage("en");
    try {
      expect(checkCopy(CODEX_FAST_CHECK).title).toBe("Fast mode uses your limit 2x as fast");
      expect(checkCopy(withFacts(null, null)).title).toBe("Fast mode uses your limit faster");
      expect(checkCopy(CODEX_FAST_CHECK).actions[0].instruction).toBe(
        "Enter `/fast` once more in Codex. It returns to Standard, and Codex saves that choice.",
      );
      expect(checkCopy(CODEX_FAST_CHECK).actions[0].note).toBe(
        "You lose: Generation speed. GPT-6.2 Astra is about 1.5x faster than Standard (docs).",
      );
      expect(noChecksText("kiro", "Kiro")).toBe("Nothing to fix right now.");
    } finally {
      setLanguage("ko");
    }
  });
});

describe("long instructions card (Claude에게 맡기기)", () => {
  it("gives a user card and a project card different short titles in ko and en", () => {
    const user: CoachCheck = {
      ...LONG_CHECK,
      facts: { ...LONG_CHECK.facts, scope: "user", folder: "/Users/me", lines: 206 } as never,
    };
    const project: CoachCheck = {
      ...LONG_CHECK,
      facts: { ...LONG_CHECK.facts, lines: 214 } as never,
    };
    expect([checkCopy(user).shortTitle, checkCopy(project).shortTitle]).toEqual([
      "전역 CLAUDE.md 206줄로 김",
      "kcal 프로젝트 CLAUDE.md 214줄로 김",
    ]);
    setLanguage("en");
    try {
      expect([checkCopy(user).shortTitle, checkCopy(project).shortTitle]).toEqual([
        "Global CLAUDE.md is 206 lines",
        "kcal project CLAUDE.md is 214 lines",
      ]);
    } finally {
      setLanguage("ko");
    }
  });

  it("names the line count in the title and puts the limit note under the recommended button", () => {
    const copy = checkCopy(LONG_CHECK);
    expect(copy.title).toBe("kcal 프로젝트 CLAUDE.md 340줄이라 Claude가 다 지키기 어려워요");
    expect(copy.headline).toBe(
      "kcal 프로젝트 `CLAUDE.md` 파일이 340줄이에요. 공식 문서는 파일당 200줄 아래를 권해요.",
    );
    expect(copy.actions).toEqual([
      expect.objectContaining({
        option: "handoff",
        label: "Claude에게 줄여 달라고 하기",
        isRecommended: true,
        note: "한도를 써요 (남은 한도가 있어야 실행돼요)",
      }),
    ]);
  });

  it("closes the button below the minimum remaining or when the limit is unknown", () => {
    expect(handoffGate(64, 20)).toEqual({ isOpen: true, reason: null });
    expect(handoffGate(20, 20)).toEqual({ isOpen: true, reason: null });
    expect(handoffGate(12, 20)).toEqual({ isOpen: false, reason: "남은 한도가 20%보다 적어요." });
    expect(handoffGate(null, 20)).toEqual({ isOpen: false, reason: "한도를 아직 몰라요." });
    expect(remainingText(64.7)).toBe("지금 남은 한도 64%");
    expect(remainingText(null)).toBe("지금 남은 한도: 모름");
  });

  it("explains each handoff failure and says the file is unchanged", () => {
    expect(handoffErrorText({ kind: "opusUnavailable" })).toBe(
      "이 계정에서는 Opus 5.5를 쓸 수 없어 맡길 수 없어요. 파일은 그대로예요.",
    );
    expect(handoffErrorText({ kind: "busy" })).toBe("이미 맡긴 일이 끝나기를 기다리는 중이에요.");
    expect(handoffErrorText({ kind: "timeout", secs: 300 })).toBe(
      "Claude가 5분 안에 끝내지 못했어요. 파일은 그대로예요.",
    );
    expect(handoffErrorText({ kind: "failed", reason: "exit 1, api_error" })).toBe(
      "Claude에게 맡기지 못했어요(exit 1, api_error). 파일은 그대로예요.",
    );
    expect(handoffErrorText({ kind: "refused", message: "다시 맡겨 주세요." })).toBe(
      "다시 맡겨 주세요.",
    );
  });

  it("counts as an alert because the recommendation is not keep", () => {
    expect(alertCount([LONG_CHECK], ["claude"])).toBe(1);
  });
});

describe("rules card copy (0018 작업 B)", () => {
  it("user card names the count, total chars and the Claude button with the limit note", () => {
    const copy = checkCopy(RULES_CHECK);
    expect(copy.title).toBe("늘 실리는 규칙 9개");
    expect(copy.headline).toBe(
      "`~/.claude/rules`의 규칙 9개(20,631자)가 모든 세션 시작 때 실려요.",
    );
    expect(copy.who).toContain("Claude(Opus 5.5)가 규칙마다 제안하고");
    expect(copy.actions).toEqual([
      {
        option: "handoff",
        label: "Claude에게 규칙마다 물어보기",
        isRecommended: true,
        isWritable: false,
        note: "한도를 써요 (남은 한도가 있어야 실행돼요)",
        instruction: "Claude에게 맡긴 뒤 규칙마다 골라 바꿔요.",
      },
    ]);
  });

  it("project card names the project folder", () => {
    const project: CoachCheck = {
      ...RULES_CHECK,
      facts: { ...RULES_CHECK.facts, scope: "project", folder: "/Users/me/work/app" } as never,
    };
    expect(checkCopy(project).headline).toContain("이 프로젝트(`app`)의 규칙 9개");
  });

  it("result rows say the suggestion, what is lost and warnings for paths, skill, keep and unusable", () => {
    const [paths, skill, keep, unusable] = RULES_PREVIEW.rules.map((r) => ruleRowCopy(r, 40000));
    expect(paths.title).toBe("`common/coding-style.md` · 2,618자");
    expect(paths.suggestion).toBe(
      "이 파일을 다룰 때만 싣기: `src/**/*.{ts,tsx}`, `src-tauri/src/**/*.rs`",
    );
    expect(paths.loses).toContain("대화를 압축하면 빠졌다가, 그 파일을 다시 열면 돌아와요");
    expect(skill.suggestion).toBe(
      "스킬로 옮기기(`git-workflow`): Use when writing a commit message or opening a pull request.",
    );
    expect(skill.loses).toContain("/git-workflow 명령으로 부를 때만 실려요");
    expect(skill.warnings).toEqual([
      "다른 규칙 1곳이 이 파일을 가리켜요. 그 링크는 고치지 않아요.",
    ]);
    expect([keep.suggestion, keep.loses, keep.canApply]).toEqual(["그대로 두기", null, false]);
    expect(unusable.suggestion).toBe(
      "Claude 제안을 쓸 수 없어요(스킬 이름이 영문 소문자·숫자·- 64자 안이 아니에요)",
    );
    expect(unusable.canApply).toBe(false);
  });

  it("paths row warns over 40,000 chars and inside an existing front matter, skill row on name taken", () => {
    const [paths, skill] = RULES_PREVIEW.rules;
    const long = ruleRowCopy(
      { ...paths, chars: 40001, hasFrontMatter: true, canApply: false },
      40000,
    );
    expect(long.warnings).toEqual([
      "본문이 40,000자를 넘어, 필요할 때만 실리면 뒷부분이 잘려요. 바꾸지 않아요.",
      "이미 머리말이 있어서 그 안에 넣어요.",
    ]);
    const taken = ruleRowCopy({ ...skill, referencedBy: 0, skillExists: true }, 40000);
    expect(taken.warnings).toEqual(["같은 이름의 스킬이 이미 있어서 옮기지 않아요."]);
    // 그대로 두기·검사 탈락 줄에는 "다른 규칙이 가리킴" 경고를 붙이지 않는다(바꾸지 않으니까).
    const keep = ruleRowCopy({ ...RULES_PREVIEW.rules[2], referencedBy: 3 }, 40000);
    expect(keep.warnings).toEqual([]);
  });

  it("every check problem has its own reason and unknown keys get one line", () => {
    const problems = [
      "missing",
      "duplicate",
      "choice",
      "skillName",
      "description",
      "pathsCount",
      "pathText",
      "pathAbsolute",
    ] as const;
    expect(new Set(problems.map(ruleProblemText)).size).toBe(problems.length);
    expect(ruleProblemText("pathAbsolute")).toBe("파일 목록이 ‘/’나 ‘~’로 시작해요");
    expect(unknownKeysText(2)).toBe("Claude 제안을 쓸 수 없어요(모르는 규칙 2개에 답했어요)");
  });
});

// Advice copy must retain the file, evidence, actor and loss in both languages.
describe("saved-setting advice", () => {
  it.each(["ko", "en"] as const)(
    "answers the three questions in %s without an apply action",
    async (lang) => {
      const { SAVED_ADVICE_CHECKS } = await import("@/test/preview/checks-fixtures");
      setLanguage(lang);
      for (const card of SAVED_ADVICE_CHECKS) {
        const copy = checkCopy(card);
        expect(copy.title.length).toBeGreaterThan(0);
        expect(copy.what.length).toBeGreaterThan(0);
        expect(copy.mine).toContain("kcal");
        expect(copy.mineList).toContain(card.file);
        expect(copy.who.length).toBeGreaterThan(0);
        expect(copy.actions.find((a) => a.option === "guide")?.instruction).toBeTruthy();
        expect(copy.actions.find((a) => a.option === "guide")?.note).toBeTruthy();
        expect(card.options.every((o) => !o.isWritable && o.changes.length === 0)).toBe(true);
        if (lang === "en") expect(JSON.stringify(copy)).not.toMatch(/[가-힣]/);
      }
      setLanguage("ko");
    },
  );
});
