import { describe, expect, it } from "vitest";

import {
  alertCount,
  alertLabel,
  appliedName,
  checkCopy,
  handoffErrorText,
  handoffGate,
  hiddenText,
  modelLabel,
  modelRows,
  noChecksText,
  remainingText,
  ruleProblemText,
  ruleRowCopy,
  unknownKeysText,
} from "@/lib/check-copy";
import { setLanguage } from "@/lib/i18n";
import type { CoachCheck } from "@/lib/ipc";
import {
  EFFORT_CHECK,
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
  it("this Mac effort card: Opus 5.5 ignores high and runs its own xhigh, nothing moves", () => {
    const copy = checkCopy(EFFORT_CHECK);
    expect(copy.title).toBe("생각 깊이 설정 한 줄이 지금 모델에 안 먹고 있어요");
    expect(copy.headline).toBe("Opus 5.5는 이 값(high)을 무시하고, 따로 정해 둔 xhigh로 돌아요.");
    expect(copy.what).toBe("안 먹는 옛 설정 한 줄을 지워요.");
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
    expect(copy.headline).toBe("Haiku 5.5는 이 값(low)을 무시하고 기본값 medium로 돌아요.");
    expect(copy.what).toBe(
      "안 먹는 옛 설정 한 줄을 지워요. Haiku 4.5는 지금처럼 low로 돌도록 그 모델 자리에 옮겨 적어요.",
    );
    expect(copy.actions[1]).toMatchObject({
      label: "Haiku 5.5도 low로 돌게 저장하기",
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
    expect(copy.title).toBe("생각 토큰 한도 설정이 지금 모델에 안 먹고 있어요");
    expect(copy.headline).toBe("Opus 5.5는 생각 양을 스스로 정해서 이 숫자를 무시해요.");
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
      { label: "Haiku 4.5", requests: 19, current: "high", verdict: "먹음" },
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
});

describe("noChecksText", () => {
  it("says nothing to fix for Claude and that no checks exist yet for Codex and Kiro", () => {
    expect(noChecksText("claude", "Claude")).toBe("지금 고칠 것이 없어요.");
    expect(noChecksText("codex", "Codex")).toBe(
      "Codex는 아직 최적화 점검이 없어요. 지금은 Claude Code만 점검해요.",
    );
    expect(noChecksText("kiro", "Kiro")).toBe(
      "Kiro는 아직 최적화 점검이 없어요. 지금은 Claude Code만 점검해요.",
    );
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
        note: "한도를 씁니다(남은 한도가 있어야 실행)",
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
        note: "한도를 씁니다(남은 한도가 있어야 실행)",
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
      "스킬 `git-workflow`로 옮기기: Use when writing a commit message or opening a pull request.",
    );
    expect(skill.loses).toContain("/git-workflow로 부를 때만 실려요");
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
    expect(ruleProblemText("pathAbsolute")).toBe("파일 목록이 / 나 ~ 로 시작해요");
    expect(unknownKeysText(2)).toBe("Claude 제안을 쓸 수 없어요(모르는 규칙 2개에 답했어요)");
  });
});
