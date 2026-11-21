// 최적화 점검·Claude에게 맡기기 IPC(`ipc.ts`가 다시 내보낸다. 컴포넌트는 `@/lib/ipc`에서 가져온다).
// 타입은 src-tauri/src/coach/checks/와 짝을 이룬다. 계약을 바꾸면 양쪽과 테스트를 같이 고친다.
import { invoke } from "@tauri-apps/api/core";

import type { Agent, AppliedSwitch } from "@/lib/ipc";

/** Rust `coach::checks::models::Effort`. 깊이 순. */
export type Effort = "low" | "medium" | "high" | "xhigh" | "max";

/** Rust `coach::checks::CheckKind`(docs/decisions/0017 처방 유형). */
export type CheckKind = "A" | "D" | "C" | "B";

/** Rust `coach::checks::models::ModelCount`. `model`은 정규화한 id(`claude-opus-5-5`). */
export type ModelCount = { model: string; requests: number };

/** Rust `coach::checks::ModelVerdict`. `isIgnored`: 이 카드의 설정을 이 모델이 무시한다. */
export type ModelVerdict = {
  model: string;
  requests: number;
  /** `modelSettings.<model>.effortLevel`(effort 카드만). */
  ownEffort: Effort | null;
  /** 모델 기본 effort. 모르면 null(effort 카드만). */
  defaultEffort: Effort | null;
  isIgnored: boolean;
};

/** Rust `coach::checks::LineMatch`. */
export type LineMatch = { file: string; line: number; text: string };

/** Rust `coach::checks::SettingsScope`. */
export type SettingsScope = "user" | "project" | "local";

/** Rust `coach::checks::rules_paths::RulesScope`. */
export type RulesScope = "user" | "project";

/** Rust `coach::checks::rules_paths::RuleFact`. 늘 실리는 규칙 하나. */
export type RuleFact = {
  /** 범위 rules 폴더 안 상대 이름(`common/testing.md`). */
  name: string;
  /** 머리말을 뗀 본문 글자 수. */
  chars: number;
  hasFrontMatter: boolean;
  /** 같은 rules 폴더의 다른 파일 중 이 파일을 가리키는 파일 수. */
  referencedBy: number;
};

/** Rust `coach::checks::CheckFacts`. 문구는 `src/lib/check-copy.ts`가 이 사실로 만든다. */
export type CheckFacts =
  | {
      type: "effortLevel";
      value: Effort;
      days: number;
      models: ModelVerdict[];
      ignoredBy: string[];
      /** 지우면 값이 풀리는 모델. `remove`가 그 모델 자리에 같은 값을 적는다. */
      movedTo: string[];
      current: string;
      unknownModels: ModelCount[];
    }
  | {
      type: "alwaysThinking";
      days: number;
      models: ModelVerdict[];
      ignoredBy: string[];
      /** 이 설정이 실제로 먹는 모델. */
      affected: string[];
      unknownModels: ModelCount[];
    }
  | {
      type: "maxThinkingTokens";
      value: number;
      days: number;
      models: ModelVerdict[];
      ignoredBy: string[];
      affected: string[];
      unknownModels: ModelCount[];
    }
  | {
      type: "gitInstructions";
      matches: LineMatch[];
      hasOwnTrailer: boolean;
      preCommitSkills: string[];
    }
  | {
      type: "agentsMdHook";
      settingsFile: string;
      scope: SettingsScope;
      command: string;
      agentsMd: string;
      workspace: string;
    }
  | {
      type: "sameSkill";
      name: string;
      personalPath: string;
      pluginId: string;
      pluginPath: string;
      isSameContent: boolean;
    }
  | {
      /** 시작 때 실리는 Claude 안내문 하나가 `limit`줄을 넘는다(0018 작업 A). 경로는 카드 `file`(실제 경로). */
      type: "longInstructions";
      name: string;
      /** 사용자 파일(`~/.claude/CLAUDE.md`)이면 user. */
      scope: RulesScope;
      /** 사용자 범위는 홈, 프로젝트는 파일이 놓인 폴더. */
      folder: string;
      lines: number;
      limit: number;
      /** 그 파일의 `@` 가져오기. */
      imports: string[];
    }
  | {
      /** 한 범위의 rules 폴더에 늘 실리는 규칙이 있다(0018 작업 B). 폴더 경로는 카드 `file`. */
      type: "rulesPaths";
      scope: RulesScope;
      /** `.claude/rules`를 품은 폴더(사용자 범위는 홈). */
      folder: string;
      /** 규칙 본문 글자 합. */
      chars: number;
      /** 필요할 때만 싣게 바꿀 때 파일당 글자 한도. */
      limit: number;
      rules: RuleFact[];
    }
  | {
      /** Codex 빠른 모드가 저장돼 있다(안내만). `multiplier`: 포함 사용량 배수(문서 표의 모델만, 모르면 null). 파일 경로는 카드 `file`. */
      type: "codexFastMode";
      model: string | null;
      multiplier: number | null;
    };

/** Rust `coach::edit::JsonChange`. 새 키면 `before`가, 지우면 `after`가 null(값은 한 줄 JSON). */
export type JsonChange = { location: string; before: string | null; after: string | null };

/** Rust `coach::checks::CheckOption`. `keep`·`isWritable: false`는 적용할 수 없다. */
export type CheckOption = {
  id: string;
  isRecommended: boolean;
  isWritable: boolean;
  /** kcal이 쓰지 않을 때 사용자가 할 일. */
  instruction: string | null;
  /** 지금 파일 기준으로 바뀔 것(자세히용). */
  changes: JsonChange[];
};

/** Rust `coach::checks::CoachCheck`. 최적화 점검 카드 하나(docs/decisions/0017 "첫 점검"). */
export type CoachCheck = {
  id: string;
  kind: CheckKind;
  agent: Agent;
  facts: CheckFacts;
  options: CheckOption[];
  file: string;
  docs: string[];
  /** 세션마다 아끼는 토큰(B만, 예상). 모르면 null. */
  tokens: number | null;
  /** "그대로 두기"로 숨겼고 지금도 추천이 `keep`이다. 화면은 숨긴다. */
  isDismissed: boolean;
};

/** Rust `coach::checks::CheckPreview`. `beforeHash`를 `applyCheck`에 그대로 보낸다. */
export type CheckPreview = {
  id: string;
  option: string;
  file: string;
  changes: JsonChange[];
  beforeHash: string;
};

export function getChecks(): Promise<CoachCheck[]> {
  return invoke<CoachCheck[]>("get_checks");
}

/** 쓰지 않고 바뀔 것을 계산한다. 목록 밖 점검·옵션, `keep`, 안내만 하는 옵션은 `invalid_input`(거부 이유). */
export function previewCheck(id: string, option: string): Promise<CheckPreview> {
  return invoke<CheckPreview>("preview_check", { id, option });
}

/** 백업 뒤 바꾼다. 미리보기 뒤 파일이 바뀌었으면 거부한다. 기록은 `AppliedSwitch`라 `undoSwitch`로 되돌린다. */
export function applyCheck(id: string, option: string, beforeHash: string): Promise<AppliedSwitch> {
  return invoke<AppliedSwitch>("apply_check", { id, option, beforeHash });
}

/** Rust `coach::checks::CHECK_TARGET_PREFIX`. 점검 적용 기록의 `target` 앞부분. */
export const CHECK_TARGET_PREFIX = "claude/check:";

/** "그대로 두기": 추천이 `keep`인 카드를 숨긴다(설정은 그대로). 그 밖의 카드·없는 id는 `invalid_input`. 뒤에 `getChecks`를 다시 읽는다. */
export function dismissCheck(id: string): Promise<void> {
  return invoke("dismiss_check", { id });
}

/** 숨긴 카드 하나를 다시 알린다(점검 목록으로 돌린다). */
export function restoreDismissedCheck(id: string): Promise<void> {
  return invoke("restore_dismissed_check", { id });
}

/** Rust `coach::checks::long_instructions::HANDOFF`. 긴 안내문 카드의 "Claude에게 맡기기" 옵션 id. */
export const HANDOFF_OPTION = "handoff";

/** Rust `coach::checks::diff::DiffKind`. */
export type DiffKind = "same" | "removed" | "added";

/** Rust `coach::checks::diff::DiffLine`. 줄 단위 차이 한 줄. */
export type DiffLine = { kind: DiffKind; text: string };

/** Rust `coach::checks::handoff::HandoffPreview`. `beforeHash`를 `applyClaudeHandoff`에 그대로 보낸다. */
export type HandoffPreview = {
  id: string;
  /** 쓸 파일(링크면 따라간 실제 경로). */
  file: string;
  beforeHash: string;
  beforeLines: number;
  afterLines: number;
  diff: DiffLine[];
  /** Claude가 합치거나 뺀 것. */
  removed: string[];
  /** 원본의 `@` 가져오기 중 고친 글에 없는 것. */
  missingImports: string[];
  /** 고친 글이 비었거나 원본보다 길거나 원본과 같으면 false. */
  canApply: boolean;
};

/** Rust `coach::checks::handoff::HandoffError`. 문구는 `check-copy.ts` `handoffErrorText`. */
export type HandoffError =
  | { kind: "busy" }
  | { kind: "limitUnknown" }
  | { kind: "limitTooLow"; remaining: number; minRemaining: number }
  | { kind: "cliMissing" }
  | { kind: "opusUnavailable" }
  | { kind: "timeout"; secs: number }
  | { kind: "failed"; reason: string }
  | { kind: "refused"; message: string }
  | { kind: "internal" };

const HANDOFF_ERROR_KINDS: readonly HandoffError["kind"][] = [
  "busy",
  "limitUnknown",
  "limitTooLow",
  "cliMissing",
  "opusUnavailable",
  "timeout",
  "failed",
  "refused",
  "internal",
];

export function isHandoffError(value: unknown): value is HandoffError {
  return (
    typeof value === "object" &&
    value !== null &&
    HANDOFF_ERROR_KINDS.includes((value as HandoffError).kind)
  );
}

/** 긴 안내문 카드를 Claude에게 맡긴다(한도를 쓴다). 같은 카드·같은 원본이면 다시 실행하지 않고 보관본을 준다. 실패는 `HandoffError`. */
export function askClaudeHandoff(id: string): Promise<HandoffPreview> {
  return invoke<HandoffPreview>("ask_claude_handoff", { id });
}

/** Rust가 보관한 고친 글을 백업 뒤 쓴다(화면은 글을 보내지 않는다). 기록은 `AppliedSwitch`라 `undoSwitch`로 되돌린다. */
export function applyClaudeHandoff(id: string, beforeHash: string): Promise<AppliedSwitch> {
  return invoke<AppliedSwitch>("apply_claude_handoff", { id, beforeHash });
}

/** Rust `coach::checks::rules_handoff::RuleProblem`. 문구는 `check-copy.ts` `ruleProblemText`. */
export type RuleProblem =
  | "missing"
  | "duplicate"
  | "choice"
  | "skillName"
  | "description"
  | "pathsCount"
  | "pathText"
  | "pathAbsolute";

/** Rust `coach::checks::rules_handoff::Proposal`. `unusable`은 적용 버튼이 없다. */
export type Proposal =
  | { choice: "paths"; paths: string[] }
  | { choice: "skill"; name: string; description: string }
  | { choice: "keep" }
  | { choice: "unusable"; problem: RuleProblem };

/** Rust `coach::checks::rules_handoff::RuleProposal`. `key`·`beforeHash`를 `applyClaudeRule`에 그대로 보낸다. */
export type RuleProposal = {
  key: string;
  name: string;
  chars: number;
  hasFrontMatter: boolean;
  referencedBy: number;
  beforeHash: string;
  proposal: Proposal;
  /** Claude가 고른 이유(한국어 한 문장). 답이 없으면 빈 값. */
  reason: string;
  /** 스킬로 옮길 자리에 같은 이름이 이미 있다. */
  skillExists: boolean;
  canApply: boolean;
};

/** Rust `coach::checks::rules_handoff::RulesPreview`. */
export type RulesPreview = {
  id: string;
  scope: RulesScope;
  folder: string;
  /** 지금 카드에 있는 규칙만, 이름순. */
  rules: RuleProposal[];
  /** Claude 답 중 kcal이 주지 않은 열쇠의 수. */
  unknownKeys: number;
};

/** 규칙 카드를 Claude에게 맡긴다(한도를 쓴다). 같은 카드·같은 파일이면 다시 실행하지 않고 보관본을 준다. 실패는 `HandoffError`. */
export function askClaudeRules(id: string): Promise<RulesPreview> {
  return invoke<RulesPreview>("ask_claude_rules", { id });
}

/** 보관본의 규칙 하나를 적용한다(`paths` 붙이기·스킬로 옮기기). 기록은 `AppliedSwitch`라 `undoSwitch`로 되돌린다. */
export function applyClaudeRule(
  id: string,
  key: string,
  beforeHash: string,
): Promise<AppliedSwitch> {
  return invoke<AppliedSwitch>("apply_claude_rule", { id, key, beforeHash });
}
