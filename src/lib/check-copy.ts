// 최적화 점검 카드 문구(docs/decisions/0017 "화면 문구"). Rust가 준 사실(`CoachCheck.facts`)만으로 만드는 순수 함수다.
// 한국어 문장은 사용자와 읽기 시험한 초안(2026-11-07) 그대로다(`i18n-ko.ts` `checks`). 고치면 0017과 check-copy.test.ts를 같이 고친다.
import { numberFormat, t } from "@/lib/i18n";
import {
  CHECK_TARGET_PREFIX,
  HANDOFF_OPTION,
  PROJECT_SKILL_TARGET_PREFIX,
  CODEX_SCOPE_TARGET_PREFIX,
  SCOPE_TARGET_PREFIX,
  SKILL_TARGET_PREFIX,
  type Agent,
  type AppliedSwitch,
  type CheckFacts,
  type CoachCheck,
  type CoachSwitch,
  type Effort,
  type HandoffError,
  type ModelVerdict,
  type RuleProblem,
  type RuleProposal,
} from "@/lib/ipc";
import { AGENT_DESCRIPTORS, hasFeature } from "@/lib/agents";

/** 버튼 하나. `isWritable`이 거짓이면 kcal이 쓰지 않고 `instruction`만 보인다. */
export type CheckAction = {
  option: string;
  label: string;
  isRecommended: boolean;
  isWritable: boolean;
  /** 버튼 아래 한 줄(한도에 주는 영향 등). */
  note: string | null;
  instruction: string | null;
};

export type CheckCopy = {
  title: string;
  /** 접힌 줄 제목(시안 J). */
  shortTitle: string;
  /** 큰 값 한 줄. `` `이름` ``은 화면이 코드 글꼴로 그린다. */
  headline: string;
  /** 큰 값 아래 설명 한 줄(없으면 null). */
  note: string | null;
  what: string;
  mine: string;
  /** "내 것 중 바뀌는 것"의 목록(C). */
  mineList: string[];
  who: string;
  better: string;
  /** 추천 먼저. `keep`은 들어가지 않는다(바꿀 것이 없다). */
  actions: CheckAction[];
  /** `keep`이 추천이면 그 이유. 화면은 주 버튼 "그대로 두기"(`KEEP_LABEL`) 옆에 보인다. */
  keepReason: string | null;
};

const FAMILY: Record<string, string> = {
  opus: "Opus",
  sonnet: "Sonnet",
  haiku: "Haiku",
  fable: "Fable",
};

/** `claude-opus-5-5` → "Opus 5.5", `claude-3-5-sonnet` → "Sonnet 3.5". 모르는 id는 그대로. */
export function modelLabel(id: string): string {
  const tokens = id.replace(/^claude-/, "").split("-");
  const family = tokens.find((t) => t in FAMILY);
  const numbers = tokens.filter((t) => t !== family && /^\d{1,2}$/.test(t));
  if (!family || numbers.length === 0 || numbers.length + 1 !== tokens.length) return id;
  return `${FAMILY[family]} ${numbers.join(".")}`;
}

const names = (ids: string[]) => ids.map(modelLabel).join("·");

const EFFORT_ORDER: Effort[] = ["low", "medium", "high", "xhigh", "max"];

const integer = numberFormat({ maximumFractionDigits: 0 });
const tokensText = (tokens: number | null) => `~${tokens === null ? "?" : integer.format(tokens)}`;

/** 옵션 id → 버튼. 문구가 없는 옵션(`keep`)은 뺀다. 추천 먼저. */
function actionsOf(
  check: CoachCheck,
  labels: Record<string, { label: string; note?: string | null }>,
): CheckAction[] {
  return check.options
    .filter((o) => o.id in labels)
    .map((o) => ({
      option: o.id,
      label: labels[o.id].label,
      isRecommended: o.isRecommended,
      isWritable: o.isWritable,
      note: labels[o.id].note ?? null,
      instruction: o.instruction,
    }))
    .sort((a, b) => Number(b.isRecommended) - Number(a.isRecommended));
}

const isKeepRecommended = (check: CoachCheck) =>
  check.options.some((o) => o.id === "keep" && o.isRecommended);

type Facts<T extends CheckFacts["type"]> = Extract<CheckFacts, { type: T }>;

function effortCopy(check: CoachCheck, f: Facts<"effortLevel">): CheckCopy {
  const byId = (id: string) => f.models.find((m) => m.model === id);
  // 큰 값의 주인공: 지금 모델이 무시하면 지금 모델, 아니면 무시하는 모델 중 가장 많이 쓴 것.
  const subjectId = f.ignoredBy.includes(f.current) ? f.current : f.ignoredBy[0];
  const subject = byId(subjectId);
  const label = modelLabel(subjectId);
  const copy = t.checks.effort;
  const headline = subject?.ownEffort
    ? copy.ownEffort(label, f.value, subject.ownEffort)
    : copy.defaultEffort(label, f.value, subject?.defaultEffort ?? "?");
  const moved = f.movedTo.length > 0 ? copy.moved(names(f.movedTo), f.value) : "";
  const current = byId(f.current);
  const base = current?.defaultEffort;
  const compare = base ? EFFORT_ORDER.indexOf(f.value) - EFFORT_ORDER.indexOf(base) : 0;
  const saveNote = compare > 0 ? copy.more : compare < 0 ? copy.less : null;
  return {
    title: copy.title,
    shortTitle: copy.short,
    headline,
    note: null,
    what: copy.what(moved),
    mine: copy.mine(f.days, f.models.length),
    mineList: [],
    who: t.checks.whoSetting,
    better: t.checks.betterSetting,
    actions: actionsOf(check, {
      remove: { label: copy.remove },
      "remove-and-save-current": {
        label: copy.saveCurrent(modelLabel(f.current), f.value),
        note: saveNote,
      },
    }),
    keepReason: null,
  };
}

function thinkingCopy(
  check: CoachCheck,
  f: Facts<"alwaysThinking"> | Facts<"maxThinkingTokens">,
): CheckCopy {
  const isBudget = f.type === "maxThinkingTokens" && f.value > 0;
  const ignored = names(f.ignoredBy);
  const affected = names(f.affected);
  const copy = t.checks.thinking;
  const mine =
    f.affected.length === 0
      ? copy.noneAffected(f.days)
      : f.type === "maxThinkingTokens" && isBudget
        ? copy.budget(affected, integer.format(f.value))
        : copy.off(affected);
  return {
    title: f.type === "alwaysThinking" ? copy.offTitle : copy.budgetTitle,
    shortTitle: f.type === "alwaysThinking" ? copy.offShort : copy.budgetShort,
    headline: isBudget ? copy.budgetIgnored(ignored) : copy.offIgnored(ignored),
    note: null,
    what: copy.what,
    mine,
    mineList: [],
    who: t.checks.whoSetting,
    better: t.checks.betterSetting,
    actions: actionsOf(check, { remove: { label: copy.remove } }),
    keepReason: isKeepRecommended(check) ? copy.keep(affected) : null,
  };
}

const fileName = (path: string) => path.split(/[/\\]/).pop() ?? path;

function gitCopy(check: CoachCheck, f: Facts<"gitInstructions">): CheckCopy {
  const files = [...new Set(f.matches.map((m) => fileName(m.file)))].join(", ");
  const copy = t.checks.git;
  const mineList = [
    copy.defaults,
    copy.status,
    ...(f.hasOwnTrailer ? [] : [copy.trailer]),
    ...(f.preCommitSkills.length > 0 ? [copy.preCommit(f.preCommitSkills.join("·"))] : []),
  ];
  const reasons = [
    f.hasOwnTrailer ? null : copy.keepTrailer,
    f.preCommitSkills.length > 0 ? copy.keepPreCommit : null,
  ].filter((r): r is string => r !== null);
  return {
    title: copy.title,
    shortTitle: copy.short,
    headline: copy.headline(files),
    note: copy.note,
    what: copy.what,
    mine: copy.mine(mineList.length),
    mineList,
    who: t.checks.whoValue,
    better: copy.better,
    actions: actionsOf(check, { "disable-git-instructions": { label: copy.action } }),
    keepReason: isKeepRecommended(check) ? reasons.join(" ") : null,
  };
}

function hookCopy(check: CoachCheck, f: Facts<"agentsMdHook">): CheckCopy {
  const copy = t.checks.hook;
  return {
    title: copy.title,
    shortTitle: copy.short,
    headline: copy.headline(tokensText(check.tokens)),
    note: copy.note,
    what: copy.what,
    mine: copy.mine,
    mineList: [],
    who: copy[f.scope],
    better: t.checks.saves(tokensText(check.tokens)),
    actions: actionsOf(check, { "remove-hook": { label: copy.action } }),
    keepReason: null,
  };
}

function skillCopy(check: CoachCheck, f: Facts<"sameSkill">): CheckCopy {
  const plugin = f.pluginId.split("@")[0];
  const copy = t.checks.skill;
  return {
    title: copy.title,
    shortTitle: copy.short(f.name),
    headline: copy.headline(f.name, plugin),
    note: null,
    what: copy.what(f.name),
    mine: f.isSameContent ? copy.same : copy.different,
    mineList: [],
    who: copy.who,
    better: t.checks.saves(tokensText(check.tokens)),
    actions: actionsOf(check, { "hide-personal": { label: copy.action } }),
    keepReason: isKeepRecommended(check) ? copy.keep : null,
  };
}

function longCopy(check: CoachCheck, f: Facts<"longInstructions">): CheckCopy {
  const copy = t.checks.long;
  const lines = integer.format(f.lines);
  const where = f.scope === "user" ? copy.user : copy.project(fileName(f.folder));
  return {
    title: copy.title(where, f.name, lines),
    shortTitle: copy.short(where, f.name, lines),
    headline: copy.headline(where, f.name, lines, f.limit),
    note: copy.note,
    what: copy.what(f.limit),
    mine: copy.mine(f.name),
    mineList: [],
    who: copy.who,
    better: copy.better,
    // 버튼 바로 아래 줄은 반드시 적는다(0017 "적용 방식").
    actions: actionsOf(check, {
      [HANDOFF_OPTION]: { label: t.checks.handoffLabel, note: t.checks.handoffNote },
    }),
    keepReason: null,
  };
}

function rulesCopy(check: CoachCheck, f: Facts<"rulesPaths">): CheckCopy {
  const copy = t.checks.rules;
  const count = integer.format(f.rules.length);
  const chars = integer.format(f.chars);
  return {
    title: copy.title(count),
    shortTitle: copy.title(count),
    headline:
      f.scope === "user" ? copy.user(count, chars) : copy.project(fileName(f.folder), count, chars),
    note: copy.note,
    what: copy.what,
    mine: copy.mine,
    mineList: [],
    who: copy.who,
    better: copy.better,
    actions: actionsOf(check, {
      [HANDOFF_OPTION]: { label: t.checks.rulesLabel, note: t.checks.handoffNote },
    }),
    keepReason: null,
  };
}

/** 배수 글자(2 → "2", 2.5 → "2.5"). 문서 표에 없는 모델이면 null. */
const multiplierText = (multiplier: number | null) =>
  multiplier === null ? null : String(multiplier);

function codexFastCopy(check: CoachCheck, f: Facts<"codexFastMode">): CheckCopy {
  const copy = t.checks.codexFast;
  const times = multiplierText(f.multiplier);
  return {
    title: copy.title(times),
    shortTitle: copy.short(times),
    headline: copy.headline(times),
    note: null,
    what: copy.what,
    mine: copy.mine,
    mineList: [],
    who: copy.who,
    better: copy.better,
    // 이 카드는 안내만 한다: 잃는 것은 버튼 자리의 안내 줄 아래에 보인다.
    actions: actionsOf(check, {
      standard: {
        label: copy.action,
        note: t.checksView.loses(f.model === "gpt-6.2-astra" ? copy.losesAstra : copy.loses),
      },
    }),
    keepReason: null,
  };
}

/** 검사에 떨어진 이유(괄호 안). */
export function ruleProblemText(problem: RuleProblem): string {
  return t.checks.problem[problem];
}

/** 결과 한 줄의 문구. `suggestion`의 `` `이름` ``은 화면이 코드 글꼴로 그린다. */
export type RuleRowCopy = {
  title: string;
  suggestion: string;
  /** 고르면 잃는 것(0018 "잃는 것을 적는다"). 없으면 null. */
  loses: string | null;
  warnings: string[];
  canApply: boolean;
};

function ruleWarnings(rule: RuleProposal, limit: number): string[] {
  const p = rule.proposal;
  const copy = t.checks.rule;
  const isChange = p.choice === "paths" || p.choice === "skill";
  return [
    isChange && rule.referencedBy > 0 ? copy.referenced(integer.format(rule.referencedBy)) : null,
    p.choice === "paths" && rule.chars > limit ? copy.tooLong(integer.format(limit)) : null,
    p.choice === "paths" && rule.hasFrontMatter ? copy.hasFrontMatter : null,
    p.choice === "skill" && rule.hasFrontMatter ? copy.frontMatterSkill : null,
    p.choice === "skill" && rule.skillExists ? copy.skillExists : null,
  ].filter((w): w is string => w !== null);
}

export function ruleRowCopy(rule: RuleProposal, limit: number): RuleRowCopy {
  const p = rule.proposal;
  const copy = t.checks.rule;
  const title = copy.title(rule.name, integer.format(rule.chars));
  const base = { title, warnings: ruleWarnings(rule, limit), canApply: rule.canApply };
  switch (p.choice) {
    case "paths":
      return {
        ...base,
        suggestion: copy.paths(p.paths.map((g) => `\`${g}\``).join(", ")),
        loses: copy.pathsLoses,
      };
    case "skill":
      return {
        ...base,
        suggestion: copy.skill(p.name, p.description),
        loses: copy.skillLoses(p.name),
      };
    case "keep":
      return { ...base, suggestion: copy.keep, loses: null };
    case "unusable":
      return { ...base, suggestion: copy.unusable(ruleProblemText(p.problem)), loses: null };
  }
}

export const unknownKeysText = (count: number) => t.checks.unknownKeys(integer.format(count));

/** 규칙 카드 실행 중 안내. */
export function rulesRunningText(timeoutSecs: number): string {
  return t.checks.rulesRunning(minutesOf(timeoutSecs));
}

/** 버튼을 켤 수 있나(Rust `handoff::check_limit`과 같은 판정). 꺼지면 이유를 준다. */
export function handoffGate(
  remaining: number | null,
  minRemaining: number,
): { isOpen: boolean; reason: string | null } {
  if (remaining === null) return { isOpen: false, reason: t.checks.gateUnknown };
  if (remaining < minRemaining) {
    return { isOpen: false, reason: t.checks.gateLow(minRemaining) };
  }
  return { isOpen: true, reason: null };
}

/** 버튼 아래 지금 남은 한도. */
export function remainingText(remaining: number | null): string {
  return remaining === null
    ? t.checks.remainingUnknown
    : t.checks.remaining(integer.format(Math.floor(remaining)));
}

const minutesOf = (secs: number) => Math.max(1, Math.round(secs / 60));

/** 실행 중 안내. */
export function handoffRunningText(timeoutSecs: number): string {
  return t.checks.handoffRunning(minutesOf(timeoutSecs));
}

/** 맡기기 실패 문구(0018 "결과 판정"). 실패하면 파일은 그대로다. */
export function handoffErrorText(error: HandoffError): string {
  const copy = t.checks.handoffError;
  switch (error.kind) {
    case "busy":
      return copy.busy;
    case "limitUnknown":
      return copy.limitUnknown;
    case "limitTooLow":
      return copy.limitTooLow(error.minRemaining, integer.format(Math.floor(error.remaining)));
    case "cliMissing":
      return copy.cliMissing;
    case "opusUnavailable":
      return copy.opusUnavailable;
    case "timeout":
      return copy.timeout(minutesOf(error.secs));
    case "failed":
      return copy.failed(error.reason);
    case "refused":
      return error.message;
    case "internal":
      return copy.internal;
  }
}

export const linesText = (before: number, after: number) =>
  t.checks.lines(integer.format(before), integer.format(after));

export function checkCopy(check: CoachCheck): CheckCopy {
  const f = check.facts;
  switch (f.type) {
    case "effortLevel":
      return effortCopy(check, f);
    case "alwaysThinking":
    case "maxThinkingTokens":
      return thinkingCopy(check, f);
    case "gitInstructions":
      return gitCopy(check, f);
    case "agentsMdHook":
      return hookCopy(check, f);
    case "sameSkill":
      return skillCopy(check, f);
    case "longInstructions":
      return longCopy(check, f);
    case "rulesPaths":
      return rulesCopy(check, f);
    case "codexFastMode":
      return codexFastCopy(check, f);
  }
}

/** 자세히의 모델 표 한 줄: 모델·요청 수·지금 값·판정. */
export type ModelRow = { label: string; requests: number; current: string; verdict: string };

export function modelRows(f: CheckFacts): ModelRow[] {
  if (!("models" in f)) return [];
  return f.models.map((m: ModelVerdict) => {
    const current =
      f.type === "effortLevel"
        ? (m.ownEffort ?? (m.isIgnored ? m.defaultEffort : f.value) ?? "?")
        : m.isIgnored
          ? t.checks.thinkingOnForced
          : f.type === "maxThinkingTokens" && f.value > 0
            ? t.common.tokens(integer.format(f.value))
            : t.checks.thinkingOff;
    return {
      label: modelLabel(m.model),
      requests: m.requests,
      current,
      verdict: m.isIgnored ? t.checks.ignored : t.checks.applies,
    };
  });
}

/** 이 컴퓨터에서 에이전트를 못 찾아 점검하지 못했을 때 문구(앱 이름·실행 명령은 에이전트 표에서). */
export function notCheckedText(agent: Agent): string {
  const text = t.checks.notChecked(AGENT_DESCRIPTORS[agent].appName, agent);
  return agent === "claude" ? `${text} ${t.checks.claudeSetup}` : text;
}

/** 대시보드 점검 탭에서 그 에이전트의 보이는 카드가 없을 때. `label`은 화면의 에이전트 이름. */
/** 점검이 실제로 돈 에이전트인가(못 찾은 경우 제외). 그때의 빈 목록은 "고칠 것 없음"(보상)이다. */
export const isCheckedAgent = (agent: Agent, isMissing = false) =>
  hasFeature(agent, "checks") && !isMissing;

export function noChecksText(agent: Agent, label: string, isMissing = false): string {
  if (!hasFeature(agent, "checks")) return t.checks.noChecks(label);
  return isMissing ? notCheckedText(agent) : t.checks.empty;
}

/** 알림에 세는 카드: 숨기지 않았고 추천이 `keep`이 아니다(적용을 추천한다). 진단하기(E)는 세지 않는다(0017 "화면 자리"). */
export function isAlert(check: CoachCheck): boolean {
  return !check.isDismissed && check.options.find((o) => o.isRecommended)?.id !== "keep";
}

/** 팝오버 "점검" 알림 개수. 켠 에이전트(`agents`)의 카드만 센다(대시보드에 보이는 것만). */
export function alertCount(checks: CoachCheck[], agents: readonly Agent[]): number {
  return checks.filter((c) => agents.includes(c.agent) && isAlert(c)).length;
}

/** 진단하기가 줄이라고 권하는 스위치: 끄기 제안(`isProposed`) 또는 "프로젝트에서만 켜기" 제안(`scopeMove`). */
export const isSuggested = (item: CoachSwitch) =>
  item.enabled && (item.isProposed || !!item.scopeMove);

/** 팝오버 입구 줄: 이 에이전트에서 줄일 수 있는 것의 개수와 시작 때 실리는 토큰 합(크기를 모르는 것은 0). 없으면 null. */
export function trimSummary(
  switches: CoachSwitch[],
  agent: Agent,
): { count: number; tokens: number } | null {
  const items = switches.filter((s) => s.agent === agent && isSuggested(s));
  if (items.length === 0) return null;
  return { count: items.length, tokens: items.reduce((sum, s) => sum + (s.tokens ?? 0), 0) };
}

/** 입구 줄 문구. 토큰 합이 0이면 토큰 조각을 뺀다. */
export const trimText = (count: number, tokens: number) =>
  t.popover.trim(count, tokens > 0 ? `~${integer.format(tokens)}` : null);

/** "점검" 옆 빨간 점의 읽기 이름. */
export const alertLabel = (count: number) => t.checks.alert(integer.format(count));

/** 목록 끝 한 줄. 숨긴 카드가 없으면 null. */
export function hiddenText(count: number): string | null {
  return count > 0 ? t.checks.hidden(integer.format(count)) : null;
}

/** 적용 기록의 화면 이름. 점검 기록은 이름이 이미 행동이라 그대로, 스위치는 "<이름> 켬|끔". */
export function appliedName({
  name,
  enabled,
  target,
}: Pick<AppliedSwitch, "name" | "enabled" | "target">): string {
  if (
    target.startsWith(CHECK_TARGET_PREFIX) ||
    target.startsWith(SCOPE_TARGET_PREFIX) ||
    target.startsWith(CODEX_SCOPE_TARGET_PREFIX)
  )
    return name;
  if (target.startsWith(SKILL_TARGET_PREFIX) || target.startsWith(PROJECT_SKILL_TARGET_PREFIX))
    return t.coach.appliedSkill(name, enabled);
  return t.coach.appliedName(name, enabled);
}

/** Claude에게 맡기는 카드(한도를 쓴다): 긴 안내문(0018 작업 A)·늘 실리는 규칙(작업 B). */
export const isHandoffCard = (check: CoachCheck) =>
  check.facts.type === "longInstructions" || check.facts.type === "rulesPaths";

/** 접힌 줄 오른쪽 메타. Rust가 준 사실에서만: 한도 씀 > ~토큰 > 효과 없음·일부만 > 기본 안내와 겹침. */
export function rowMeta(check: CoachCheck): { text: string; isEstimate: boolean } | null {
  const copy = t.checksView;
  if (isHandoffCard(check)) return { text: copy.metaLimit, isEstimate: false };
  if (check.tokens !== null)
    return { text: copy.metaTokens(tokensText(check.tokens)), isEstimate: true };
  const f = check.facts;
  switch (f.type) {
    case "effortLevel":
      return {
        text: f.ignoredBy.includes(f.current) ? copy.metaNoEffect : copy.metaPartly,
        isEstimate: false,
      };
    case "alwaysThinking":
    case "maxThinkingTokens":
      return {
        text: f.affected.length === 0 ? copy.metaNoEffect : copy.metaPartly,
        isEstimate: false,
      };
    case "gitInstructions":
      return { text: copy.metaOverlap, isEstimate: false };
    case "codexFastMode":
      return { text: copy.metaFast(multiplierText(f.multiplier)), isEstimate: false };
    default:
      return null;
  }
}

/** 펼친 카드의 근거 칩(사실에서만). 쓸 수 있는 옵션이 있으면 "되돌리기 가능"을 더한다. */
export function checkChips(check: CoachCheck): string[] {
  const chip = t.checksView.chip;
  const f = check.facts;
  const chips: string[] = (() => {
    switch (f.type) {
      case "effortLevel":
      case "alwaysThinking":
      case "maxThinkingTokens":
        return [
          chip.days(f.days),
          ...(f.ignoredBy.length > 0 ? [chip.ignoredBy(names(f.ignoredBy))] : []),
          ...("affected" in f && f.affected.length > 0 ? [chip.affected(names(f.affected))] : []),
        ];
      case "gitInstructions":
        return [chip.rules([...new Set(f.matches.map((m) => fileName(m.file)))].join(", "))];
      case "agentsMdHook":
        return [fileName(f.settingsFile), fileName(f.agentsMd)];
      case "sameSkill":
        return [
          chip.plugin(f.pluginId.split("@")[0]),
          f.isSameContent ? chip.same : chip.different,
        ];
      case "longInstructions":
        return [chip.lines(integer.format(f.lines), f.limit)];
      case "rulesPaths":
        return [chip.ruleCount(integer.format(f.rules.length), integer.format(f.chars))];
      case "codexFastMode":
        return [
          ...(f.model ? [chip.fastModel(f.model)] : []),
          ...(f.multiplier !== null ? [chip.fastMultiplier(String(f.multiplier))] : []),
        ];
    }
  })();
  const canUndo = check.options.some((o) => o.isWritable) || isHandoffCard(check);
  return canUndo ? [...chips, t.coach.chipUndo] : chips;
}

/** 미리보기에 보일 바뀌는 줄(최대 `max`). 추천 쓰기 옵션 먼저, 없으면 첫 쓰기 옵션. */
export function diffLines(
  check: CoachCheck,
  max: number,
): { lines: { kind: "removed" | "added"; text: string }[]; more: number } {
  const writable = check.options.filter((o) => o.isWritable && o.changes.length > 0);
  const option = writable.find((o) => o.isRecommended) ?? writable[0];
  if (!option) return { lines: [], more: 0 };
  const all = option.changes.flatMap((c) => [
    ...(c.before !== null
      ? [{ kind: "removed" as const, text: `${c.location}: ${c.before}` }]
      : []),
    ...(c.after !== null ? [{ kind: "added" as const, text: `${c.location}: ${c.after}` }] : []),
  ]);
  return { lines: all.slice(0, max), more: Math.max(0, all.length - max) };
}

/** 이 칸에서 고쳐서 아직 되돌리지 않은 점검 기록(새것 먼저, `get_coach_log` 순서).
 * `since`(유닉스 ms)가 있으면 그 뒤에 적용한 것만(진단하기 "최근 D일"과 같은 기간). null이면 기간 제한 없음. */
export function fixedEntries(
  log: AppliedSwitch[],
  agent: Agent,
  since: number | null = null,
): AppliedSwitch[] {
  const undone = new Set(log.flatMap((e) => (e.undoOf ? [e.undoOf] : [])));
  return log.filter(
    (e) =>
      e.agent === agent &&
      e.action === "apply" &&
      e.target.startsWith(CHECK_TARGET_PREFIX) &&
      !undone.has(e.id) &&
      (since === null || Date.parse(e.at) >= since),
  );
}

/** 맨 위 회색 한 줄: 남은 카드 수 + Rust가 준 토큰(`check.tokens`)이 있을 때만 그 합. */
export function leftText(left: CoachCheck[]): string {
  const copy = t.checksView;
  if (left.length === 0) return copy.allFixed;
  const known = left.filter((c) => c.tokens !== null);
  const sum = known.reduce((total, c) => total + (c.tokens ?? 0), 0);
  const count = integer.format(left.length);
  if (sum === 0) return copy.left(count);
  const tokens = `~${integer.format(sum)}`;
  return known.length === left.length
    ? copy.leftSaves(count, tokens)
    : copy.leftSomeSave(count, integer.format(known.length), tokens);
}
