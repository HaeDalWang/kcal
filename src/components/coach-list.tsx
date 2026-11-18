// 진단하기 목록(시안 E, 2026-11-14): 맨 위 큰 숫자 한 줄(제안 합계) → 묶음 3개("끄기 제안" / "<프로젝트>에서만 켜기 제안" /
// "그대로 둠", 마지막은 접힘). 제안 줄은 한 줄이고, 누른 줄 하나만 카드로 펼쳐 근거 칩과 "끄면 함께 꺼지는 것"을 보인다.
// 펼친 줄의 버튼만 강조 버튼이다(design.md 6절 3). 묶음은 점 + 글자로 알린다(색만으로 알리지 않는다).
import { ChevronRight } from "lucide-react";
import { cn } from "cn";
import { useState } from "react";

import { AGENT_COLOR } from "@/components/agent-metrics";
import { SUMMARY_CLASS } from "@/components/info-section";
import { formatShare } from "@/components/rate-line";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { shareOf } from "@/lib/breakdown";
import { CLAUDE_CODE, isSuggested, trimSummary } from "@/lib/check-copy";
import { numberFormat, t } from "@/lib/i18n";
import { switchRowKey, useOpenRow, type OpenRow } from "@/lib/open-row";
import type { Agent, CoachSwitch, CodexRate, NameList, SwitchUsage } from "@/lib/ipc";

const integer = numberFormat({ maximumFractionDigits: 0 });

/** Codex 스위치의 5시간 한도 몫(%, 추론). 환율·창 패턴이 없으면 null. */
function codexShare(item: CoachSwitch, rate: CodexRate | undefined): number | null {
  if (item.agent !== "codex" || item.tokens === null || !rate) return null;
  return shareOf(item.tokens, rate)?.point ?? null;
}

/** "1 use in 227 Claude sessions over 30 days · last used Nov 5 in kcal" 또는 "최근 30일 안 씀". */
function usageText(usage: SwitchUsage): string {
  if (usage.total === 0) {
    if (usage.sessions < usage.minSessions) return t.coach.fewSessions(usage.days, usage.sessions);
    const unused = t.coach.unused(usage.days, usage.sessions);
    return usage.isPartial ? `${unused} (${t.coach.partial})` : unused;
  }
  const last =
    usage.lastUsed !== null && usage.lastProject
      ? { at: usage.lastUsed, project: usage.lastProject }
      : null;
  return t.coach.used(usage.days, usage.sessions, usage.total, last);
}

/** 사용자 범위로 켠 플러그인인데 쓴 횟수가 적어 "프로젝트에서만 켜기"를 권하지 않는 것. */
const hasTooFewUses = (item: CoachSwitch) =>
  item.kind === "plugin" &&
  item.enabled &&
  item.scope?.user === true &&
  !item.scopeMove &&
  item.usage !== null &&
  item.usage.total > 0 &&
  item.usage.total < item.usage.scopeMinUses;

/** "범위: 전체" 또는 "범위: 프로젝트 kcal, foo". 꺼져 있고 어느 프로젝트에서도 안 켰으면 null. */
function scopeText(item: CoachSwitch): string | null {
  if (!item.scope) return null;
  if (item.scope.user) return t.coach.scopeAll;
  const on = item.scope.projects.filter((p) => p.enabled).map((p) => p.name);
  return on.length > 0 ? t.coach.scopeProjects(on.join(", ")) : null;
}

/** 켜져 있고 최근에 쓴 것: 끄기를 권하지 않는다. */
const isKept = (item: CoachSwitch) =>
  item.enabled && !item.scopeMove && (item.usage?.total ?? 0) > 0;

/** 행의 상태 하나. 제안 > 사용 적음 > 쓰는 중 > 꺼짐 > 알 수 없음(근거가 없다). */
export type SwitchBadge = keyof typeof t.coach.badge;
export function badgeOf(item: CoachSwitch): SwitchBadge {
  if (isSuggested(item)) return "suggested";
  if (hasTooFewUses(item)) return "fewUses";
  if (isKept(item)) return "inUse";
  return item.enabled ? "unknown" : "off";
}

/** 묶음: 제안(배지 "suggested")은 끄기(`isProposed`)와 프로젝트에서만 켜기(`scopeMove`)로 나누고, 나머지는 그대로 둠. */
export type SwitchGroup = "off" | "move" | "keep";
export function groupOf(item: CoachSwitch): SwitchGroup {
  if (badgeOf(item) !== "suggested") return "keep";
  return item.isProposed ? "off" : "move";
}

/** "그대로 둠" 줄의 짧은 사유: 배지 글자 + 왜(횟수·기록 적음·크기 모름 등). */
function keepReason(item: CoachSwitch): string {
  const badge = badgeOf(item);
  const usage = item.usage;
  const why = (() => {
    if (badge === "off") return null;
    if ((badge === "inUse" || badge === "fewUses") && usage) return t.coach.reasonUses(usage.total);
    if (!usage) return item.agent === "claude" ? null : t.coach.reasonNoLog;
    if (usage.total === 0 && usage.sessions < usage.minSessions)
      return t.coach.reasonFewSessions(usage.sessions);
    if (usage.isPartial) return t.coach.partial;
    if (item.tokens === null) return t.coach.sizeUnknown;
    return item.loading === "startup" ? null : t.coach.reasonLater;
  })();
  return why ? `${t.coach.badge[badge]} · ${why}` : t.coach.badge[badge];
}

/** "스킬 4개(label, review, summarize 외 1개)" / "4 skills (label, …)". 없으면 null. */
function nameListText(list: NameList, kind: (count: number) => string): string | null {
  if (list.count === 0) return null;
  const rest = list.count - list.names.length;
  const names = list.names.join(", ") + (rest > 0 ? ` ${t.coach.moreNames(rest)}` : "");
  return t.coach.namesIn(kind(list.count), names);
}

/** "끄면 함께 꺼지는 것" 한 줄. 플러그인 폴더를 못 읽었거나(Rust `None`) MCP면 null. */
function contentsText(item: CoachSwitch): string | null {
  const contents = item.kind === "plugin" ? item.contents : null;
  if (!contents) return null;
  const kinds = t.coach.contentKind;
  const parts = [
    nameListText(contents.skills, kinds.skills),
    nameListText(contents.agents, kinds.agents),
    nameListText(contents.commands, kinds.commands),
    nameListText(contents.mcpServers, kinds.mcpServers),
  ].filter((p): p is string => p !== null);
  if (parts.length === 0) return null;
  const label = item.scopeMove ? t.coach.losesMove : t.coach.losesOff;
  return `${label}: ${parts.join(" · ")}`;
}

/** 펼친 카드의 근거 칩(84b317f의 사용 기록 근거). */
function evidenceChips(item: CoachSwitch, share: number | null): string[] {
  const usage = item.usage;
  const chips: (string | null)[] = [];
  if (usage) {
    chips.push(t.coach.chipUses(usage.days, usage.total));
    chips.push(usage.isPartial ? t.coach.partial : t.coach.chipAllSessions(usage.sessions));
    if (item.scopeMove)
      chips.push(...usage.projects.map((p) => t.coach.chipUsedIn(p.name, p.count)));
    if (usage.lastUsed !== null) chips.push(t.coach.chipLast(usage.lastUsed));
  }
  chips.push(scopeText(item));
  if (share !== null) chips.push(t.coach.limitShare(formatShare(share)));
  // 직접 끄는 항목(format "manual")은 kcal이 쓰지 않아 되돌리기가 없다.
  if (item.format !== "manual") chips.push(t.coach.chipUndo);
  return chips.filter((c): c is string => c !== null);
}

function actionLabel(item: CoachSwitch): string {
  if (item.scopeMove) return t.coach.moveToProjects;
  if (item.format === "manual") return t.coach.howToOff;
  return item.enabled ? t.coach.turnOff : t.coach.turnOn;
}

/** 줄의 버튼. `isStrong`이면 강조(펼친 줄 하나만). `relative z-10`: 제안 줄 전체를 덮는 펼치기 자리 위에 놓는다. */
function ActionButton({
  item,
  isStrong,
  onPick,
  onMove,
}: {
  item: CoachSwitch;
  isStrong: boolean;
  onPick: (item: CoachSwitch) => void;
  onMove: (item: CoachSwitch) => void;
}) {
  const action = actionLabel(item);
  return (
    <Button
      size="xs"
      variant={isStrong ? "default" : "outline"}
      className="relative z-10"
      aria-label={`${item.name} ${action}`}
      onClick={() => (item.scopeMove ? onMove(item) : onPick(item))}
    >
      {action}
    </Button>
  );
}

const KindChip = ({ item }: { item: CoachSwitch }) => (
  <Badge
    variant="outline"
    className="h-4 shrink-0 px-1.5 py-0 text-[10px] leading-none text-muted-foreground"
  >
    {item.kind === "mcp" ? "MCP" : t.coach.plugin}
  </Badge>
);

/** 제안 줄. 접히면 한 줄(▸ 이름 · 종류 · ~토큰 · 버튼), 펼치면 테두리 카드에 근거 칩과 함께 꺼지는 것. */
function SuggestRow({
  item,
  rate,
  isOpen,
  onToggle,
  onPick,
  onMove,
}: {
  item: CoachSwitch;
  rate: CodexRate | undefined;
  isOpen: boolean;
  onToggle: () => void;
  onPick: (item: CoachSwitch) => void;
  onMove: (item: CoachSwitch) => void;
}) {
  const detailsId = `${item.target}-details`;
  const share = codexShare(item, rate);
  const loses = isOpen ? contentsText(item) : null;
  const noLog = !item.usage && item.agent !== "claude";
  return (
    <li
      className={`flex flex-col gap-1.5 rounded-lg border text-xs ${isOpen ? "-mx-2 px-2 py-1.5" : "border-transparent"}`}
      style={isOpen ? { borderColor: AGENT_COLOR[item.agent] } : undefined}
    >
      {/* 줄 전체가 펼치기 버튼의 누르는 자리다: 버튼의 ::after가 줄을 덮고, 끄기 버튼만 그 위(z-10)에 둔다(중첩 button 없음). */}
      <div className="relative flex items-center gap-2 rounded-md hover:bg-accent">
        <button
          type="button"
          aria-expanded={isOpen}
          aria-controls={detailsId}
          onClick={onToggle}
          className="flex min-w-0 flex-1 cursor-pointer items-center gap-1.5 text-left after:absolute after:inset-0 after:rounded-md after:content-[''] focus-visible:outline-none focus-visible:after:ring-3 focus-visible:after:ring-ring active:translate-y-px"
        >
          <ChevronRight
            aria-hidden
            className={`size-[12px] shrink-0 text-muted-foreground transition-transform motion-reduce:transition-none ${isOpen ? "rotate-90" : ""}`}
          />
          <span className="truncate font-medium" title={item.name}>
            {item.name}
          </span>
          <KindChip item={item} />
          <span
            className="ml-auto shrink-0 pl-2 font-semibold whitespace-nowrap tabular-nums"
            title={t.info.estimated}
          >
            {item.tokens === null ? t.coach.sizeUnknown : `~${integer.format(item.tokens)}`}
          </span>
        </button>
        <ActionButton item={item} isStrong={isOpen} onPick={onPick} onMove={onMove} />
      </div>
      {isOpen && (
        <div id={detailsId} className="flex flex-col gap-1 pl-[18px] text-[10px] tabular-nums">
          <ul className="flex flex-wrap gap-1" aria-label={t.coach.badge.suggested}>
            {evidenceChips(item, share).map((chip) => (
              <li key={chip}>
                <Badge variant="outline" className="h-4 px-1.5 py-0 text-[10px] leading-none">
                  {chip}
                </Badge>
              </li>
            ))}
          </ul>
          {noLog && (
            <p className="text-muted-foreground">
              {t.coach.noUsageLog(item.agent === "codex" ? "Codex" : "Kiro")}
            </p>
          )}
          {loses && <p className="text-muted-foreground">{loses}</p>}
        </div>
      )}
    </li>
  );
}

/** "그대로 둠" 줄: 이름 + 짧은 사유 한 줄. 끄는 버튼은 kcal이 권하지 않을 뿐 둔다(0022). */
function KeepRow({
  item,
  onPick,
  onMove,
}: {
  item: CoachSwitch;
  onPick: (item: CoachSwitch) => void;
  onMove: (item: CoachSwitch) => void;
}) {
  const title = [
    item.usage ? usageText(item.usage) : null,
    hasTooFewUses(item) ? t.coach.tooFewUses : null,
    ...(item.usage?.projects.map((p) => `${p.path} ${integer.format(p.count)}×`) ?? []),
  ]
    .filter(Boolean)
    .join("\n");
  return (
    <li className="flex items-center gap-2 text-xs">
      <span className="min-w-0 truncate" title={item.name}>
        {item.name}
      </span>
      <span
        className="min-w-0 flex-1 truncate text-[10px] text-muted-foreground tabular-nums"
        title={title || undefined}
      >
        {keepReason(item)}
      </span>
      {item.tokens !== null && (
        <span className="shrink-0 text-[10px] text-muted-foreground tabular-nums">
          {`~${integer.format(item.tokens)}`}
        </span>
      )}
      {!isKept(item) && (
        <ActionButton item={item} isStrong={false} onPick={onPick} onMove={onMove} />
      )}
    </li>
  );
}

/** 묶음 제목 앞 작은 점. 끄기는 채운 점, 프로젝트에서만은 테두리 점, 그대로 둠은 회색 점(글자가 뜻을 말한다). */
function GroupDot({ group, agent }: { group: SwitchGroup; agent: Agent }) {
  const color = AGENT_COLOR[agent];
  const style =
    group === "off"
      ? { backgroundColor: color }
      : group === "move"
        ? { borderColor: color }
        : undefined;
  return (
    <span
      aria-hidden
      style={style}
      className={`size-[7px] shrink-0 rounded-full ${group === "move" ? "border-[1.5px]" : ""} ${group === "keep" ? "bg-muted-foreground" : ""}`}
    />
  );
}

export const GROUP_TITLE =
  "flex items-center gap-1.5 text-[11px] font-medium text-muted-foreground";

/** 묶음 제목 "<프로젝트>에서만 켜기 제안": 제안이 모두 같은 한 프로젝트면 그 이름, 아니면 일반 문구. */
function moveProject(items: CoachSwitch[]): string | null {
  const names = new Set(items.map((s) => s.scopeMove?.projects.map((p) => p.name).join(", ")));
  const [only] = [...names];
  return names.size === 1 && only ? only : null;
}

/** 맨 위 요약: 큰 숫자(제안 합계) + 회색 한 줄 + 오른쪽 "읽는 법". 제안이 없으면 빈 상태 문장. */
function Summary({
  agent,
  isMissing,
  switches,
}: {
  agent: Agent;
  isMissing: boolean;
  switches: CoachSwitch[];
}) {
  const [isLegendOpen, setIsLegendOpen] = useState(false);
  const trim = trimSummary(switches, agent);
  const basis = switches.find((s) => s.usage)?.usage ?? null;
  const legendId = `${agent}-diagnose-legend`;
  const legend =
    agent === "claude"
      ? t.coach.legendTokens + t.coach.legendClaude + t.coach.evidenceSource
      : t.coach.legendTokens;
  return (
    <div className="flex flex-col gap-1 px-5 pt-2.5 pb-2">
      <div className="flex items-start gap-2">
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          {trim ? (
            <>
              <p className="text-2xl leading-tight font-semibold tabular-nums">
                <span title={t.info.estimated}>{`~${integer.format(trim.tokens)}`}</span>
                <span className="ml-1 text-xs font-medium text-muted-foreground">
                  {t.coach.tokensUnit}
                </span>
              </p>
              {/* 두 문장은 줄을 나눠 둔다(한 줄로 이으면 좁을 때 "·"가 줄 끝에 홀로 남는다). */}
              <p className="text-[10px] text-muted-foreground tabular-nums">
                {t.coach.summary(trim.count)}
              </p>
              {basis && (
                <p className="text-[10px] text-muted-foreground tabular-nums">
                  {t.coach.basis(basis.days, basis.sessions)}
                </p>
              )}
            </>
          ) : (
            <p className="text-xs text-muted-foreground wrap-break-word select-text">
              {isMissing
                ? t.checks.notChecked(CLAUDE_CODE)
                : switches.length === 0
                  ? t.coach.noneOn
                  : t.coach.noSwitches}
            </p>
          )}
        </div>
        <Button
          size="xs"
          variant="ghost"
          aria-expanded={isLegendOpen}
          aria-controls={legendId}
          onClick={() => setIsLegendOpen((open) => !open)}
          className="text-muted-foreground"
        >
          {t.coach.howToRead}
        </Button>
      </div>
      {isLegendOpen && (
        <p id={legendId} className="text-[10px] text-muted-foreground">
          {legend}
        </p>
      )}
    </div>
  );
}

export function SwitchLists({
  agent,
  isMissing,
  switches,
  rate,
  onPick,
  onMove,
  openRow,
}: {
  agent: Agent;
  /** 에이전트를 이 컴퓨터에서 못 찾았다: 점검을 못 한 것이지 끌 것이 없는 게 아니다. */
  isMissing: boolean;
  switches: CoachSwitch[];
  rate: CodexRate | undefined;
  onPick: (item: CoachSwitch) => void;
  onMove: (item: CoachSwitch) => void;
  /** 점검 탭 전체의 펼친 줄(키 "switch:<target>"). 없으면 이 칸 안에서만 하나. */
  openRow?: OpenRow;
}) {
  // 펼친 줄은 하나만(강조 버튼 하나, design.md 6절 3). 기본은 모두 접힘.
  const open = useOpenRow(openRow);
  const off = switches.filter((s) => groupOf(s) === "off");
  const move = switches.filter((s) => groupOf(s) === "move");
  const keep = switches.filter((s) => groupOf(s) === "keep");
  const offDays = off.find((s) => s.usage)?.usage?.days ?? null;
  const row = (s: CoachSwitch) => (
    <SuggestRow
      key={s.target}
      item={s}
      rate={rate}
      isOpen={open.key === switchRowKey(s.target)}
      onToggle={() => open.toggle(switchRowKey(s.target))}
      onPick={onPick}
      onMove={onMove}
    />
  );
  const offTitle =
    offDays === null ? t.coach.groupOff : `${t.coach.groupOff} · ${t.coach.groupOffWhy(offDays)}`;
  const moveTitle = t.coach.groupMove(moveProject(move));
  return (
    <>
      <Summary agent={agent} isMissing={isMissing} switches={switches} />
      {off.length > 0 && (
        <section
          role="group"
          aria-label={offTitle}
          className="flex flex-col gap-1.5 border-t px-5 py-2.5"
        >
          <h2 className={GROUP_TITLE}>
            <GroupDot group="off" agent={agent} />
            {offTitle}
          </h2>
          <ul className="flex flex-col gap-1.5">{off.map(row)}</ul>
        </section>
      )}
      {move.length > 0 && (
        <section
          role="group"
          aria-label={moveTitle}
          className="flex flex-col gap-1.5 border-t px-5 py-2.5"
        >
          <h2 className={GROUP_TITLE}>
            <GroupDot group="move" agent={agent} />
            {moveTitle}
          </h2>
          <ul className="flex flex-col gap-1.5">{move.map(row)}</ul>
        </section>
      )}
      {keep.length > 0 && (
        <details className="group border-t px-5 py-2.5">
          {/* 제목 줄 전체가 누르는 자리(제안 줄과 같은 pointer). */}
          <summary className={cn(SUMMARY_CLASS, GROUP_TITLE, "w-auto cursor-pointer")}>
            <GroupDot group="keep" agent={agent} />
            {t.coach.groupKeep(keep.length)}
            <ChevronRight
              aria-hidden
              className="size-[12px] shrink-0 transition-transform group-open:rotate-90 motion-reduce:transition-none"
            />
          </summary>
          {agent !== "claude" && (
            <p className="pt-1 text-[10px] text-muted-foreground">
              {t.coach.noUsageLog(agent === "codex" ? "Codex" : "Kiro")}
            </p>
          )}
          <ul className="flex flex-col gap-1.5 pt-1.5">
            {keep.map((s) => (
              <KeepRow key={s.target} item={s} onPick={onPick} onMove={onMove} />
            ))}
          </ul>
        </details>
      )}
    </>
  );
}
