import { ChevronDown } from "lucide-react";
import { useState, useSyncExternalStore, type ReactNode } from "react";

import { AGENT_COLOR, startupTitle, startupValue } from "@/components/agent-metrics";
import { InfoSection, SUMMARY_CLASS } from "@/components/info-section";
import { formatShare } from "@/components/rate-line";
import { SnapshotAge } from "@/components/snapshot-age";
import { StateText } from "@/components/state-text";
import { shareOf, tokenRateOf, type Share } from "@/lib/breakdown";
import {
  type Agent,
  type AgentContext,
  type CodexRate,
  type ContextItem,
  type ContextKind,
  type ContextSource,
} from "@/lib/ipc";
import { contextStore, ratesStore, useSnapshot } from "@/lib/snapshot-store";
import { numberFormat, t } from "@/lib/i18n";

/** 접힌 상태에서 묶음마다 보이는 줄 수(큰 순). 나머지는 "N개 더 보기"로 편다. */
export const TOP_PER_GROUP = 3;

const integer = numberFormat({ maximumFractionDigits: 0 });
const points = numberFormat({ minimumFractionDigits: 2, maximumFractionDigits: 2 });

/** 토큰 → 5시간 한도 몫(%). 1%당 토큰(Codex·Claude)이 있고 사용 패턴이 있을 때만 값이 있다. */
type ShareOf = (tokens: number) => Share | null;

/** 자세히 안의 범위 설명(예상). */
function shareRange(share: Share): string {
  return t.context.range(
    points.format(share.point),
    points.format(share.low),
    points.format(share.high),
  );
}

/** 시작 시점 항목의 토큰을 출처별로 더한다. */
export function startupTokensBySource(items: ContextItem[]): Record<ContextSource, number> {
  const sums: Record<ContextSource, number> = { measured: 0, estimated: 0 };
  for (const item of items) {
    if (item.loading === "startup" && item.tokens !== null) sums[item.source] += item.tokens;
  }
  return sums;
}

type GroupId = keyof typeof t.context.groups;

/** `ContextKind` → 묶음. 데이터에 있는 종류만 쓴다. */
const GROUP_OF: Record<ContextKind, GroupId> = {
  system: "tools",
  tools: "tools",
  instructions: "files",
  rules: "files",
  skills: "skills",
  mcp: "mcp",
  mcpInstructions: "mcp",
  agents: "other",
  plugins: "other",
  hooks: "other",
};
const GROUP_ORDER: GroupId[] = ["tools", "files", "skills", "mcp", "other"];
/** 묶음 색: 에이전트 색의 불투명도 단계(새 색 토큰 없음). 막대와 표시 점이 같은 값을 쓴다. */
const GROUP_OPACITY = [1, 0.75, 0.55, 0.4, 0.3];

type Group = { id: GroupId; items: ContextItem[]; tokens: number; isEstimated: boolean };

/** 시작 항목을 묶음별로 모은다. 묶음 안은 토큰 큰 순, 모르는 값(null)은 끝. 빈 묶음은 뺀다. */
function groupItems(items: ContextItem[]): Group[] {
  return GROUP_ORDER.map((id) => {
    const own = items
      .filter((i) => GROUP_OF[i.kind] === id)
      .sort((a, b) => (b.tokens ?? -1) - (a.tokens ?? -1));
    return {
      id,
      items: own,
      tokens: own.reduce((sum, i) => sum + (i.tokens ?? 0), 0),
      isEstimated: own.some((i) => i.source === "estimated"),
    };
  }).filter((g) => g.items.length > 0);
}

const MIN_PERCENT = 0.01;
const mark = (isEstimated: boolean) => (isEstimated ? "~" : "");

/** 값 줄: 이름 · 토큰 · (컨텍스트 창을 알면) 창 대비 %. 에이전트와 상관없이 같은 칸 모양이다. */
const rowClass = (hasWindow: boolean) =>
  `grid items-baseline gap-x-2 text-xs ${
    hasWindow ? "grid-cols-[minmax(0,1fr)_auto_3.5rem]" : "grid-cols-[minmax(0,1fr)_auto]"
  }`;

/** 창 대비 %. 0.01% 미만은 "<0.01%"로 줄여 칸 너비를 지킨다. */
function windowPercent(tokens: number, window: number, isEstimated: boolean): string {
  const value = (tokens / window) * 100;
  return value < MIN_PERCENT ? `<${MIN_PERCENT}%` : `${mark(isEstimated)}${formatShare(value)}%`;
}

function ValueCells({
  tokens,
  isEstimated,
  window,
  className = "",
}: {
  tokens: number | null;
  isEstimated: boolean;
  window: number | null;
  className?: string;
}) {
  const percent = tokens === null || !window ? null : windowPercent(tokens, window, isEstimated);
  return (
    <>
      <span
        className={`text-right tabular-nums ${className}`}
        title={tokens === null ? undefined : isEstimated ? t.info.estimated : t.info.measured}
      >
        {tokens === null ? "—" : `${mark(isEstimated)}${integer.format(tokens)}`}
      </span>
      {window !== null && (
        <span
          className="text-right whitespace-nowrap text-muted-foreground tabular-nums"
          title={percent === null ? undefined : t.context.windowShare}
        >
          {percent}
        </span>
      )}
    </>
  );
}

function ItemRow({ item, window }: { item: ContextItem; window: number | null }) {
  return (
    <li className={rowClass(window !== null)}>
      <span className="min-w-0 truncate" title={item.path ?? item.name}>
        {item.name}
      </span>
      <ValueCells
        tokens={item.tokens}
        isEstimated={item.source === "estimated"}
        window={window}
        className="text-muted-foreground"
      />
    </li>
  );
}

function ItemList({ items, window }: { items: ContextItem[]; window: number | null }) {
  return (
    <ul className="flex flex-col gap-0.5 pl-3">
      {items.map((item, index) => (
        <ItemRow
          key={`${item.kind}:${item.path ?? item.name}:${index}`}
          item={item}
          window={window}
        />
      ))}
    </ul>
  );
}

const groupColor = (agent: Agent, id: GroupId) => ({
  backgroundColor: AGENT_COLOR[agent],
  opacity: GROUP_OPACITY[GROUP_ORDER.indexOf(id)],
});

/** 시작 토큰의 묶음별 비율. 장식이라 글자 값은 아래 묶음 줄에 있다. */
function CompositionBar({ agent, groups }: { agent: Agent; groups: Group[] }) {
  const total = groups.reduce((sum, g) => sum + g.tokens, 0);
  if (total === 0) return null;
  return (
    <div role="img" aria-label={t.context.barLabel} className="flex h-1.5 gap-px">
      {groups.map((g) => (
        <span
          key={g.id}
          title={`${t.context.groups[g.id]} ${integer.format(g.tokens)}`}
          className="h-full first:rounded-l-full last:rounded-r-full"
          style={{ width: `${(g.tokens / total) * 100}%`, ...groupColor(agent, g.id) }}
        />
      ))}
    </div>
  );
}

function ContextBody({
  context,
  charsPerToken,
  share,
  aside,
}: {
  context: AgentContext;
  charsPerToken: number;
  share: ShareOf;
  aside: ReactNode;
}) {
  const [isOpen, setIsOpen] = useState(false);
  const sums = startupTokensBySource(context.items);
  const groups = groupItems(context.items.filter((i) => i.loading === "startup"));
  const onDemand = context.items.filter((i) => i.loading !== "startup");
  const window = context.contextWindow;
  const hiddenCount =
    groups.reduce((sum, g) => sum + Math.max(0, g.items.length - TOP_PER_GROUP), 0) +
    onDemand.length;
  const onDemandTokens = onDemand.reduce((sum, i) => sum + (i.tokens ?? 0), 0);
  const totalShare = share(context.totalStartupTokens);
  // 첫 요청 측정값 − 시작 합계: 세지 못한 내장 도구 정의와 첫 사용자 메시지.
  const remainder =
    context.firstRequestTokens === null
      ? 0
      : context.firstRequestTokens - context.totalStartupTokens;
  const remainderShare = remainder > 0 ? share(remainder) : null;
  // 접을 것이 없으면 펼친 것과 같다. 근거·주의 줄은 펼친 아래에만 둔다.
  const isExpanded = isOpen || hiddenCount === 0;
  // 대시보드의 시작 컨텍스트는 이 칸 하나다(트레이 `StartupSummary`와 같은 값 줄 + 묶음별 목록·한도 몫).
  return (
    <InfoSection
      title={t.common.startupContext}
      value={startupValue(context)}
      valueTitle={startupTitle(context)}
      description={
        <>
          <p>{t.context.description}</p>
          {totalShare && (
            <p title={t.info.estimated}>{t.metrics.limitShare(points.format(totalShare.point))}</p>
          )}
        </>
      }
      aside={aside}
      details={
        <span>
          {sums.measured === 0
            ? t.context.estimatedOnly(integer.format(sums.estimated))
            : t.context.sources(integer.format(sums.measured), integer.format(sums.estimated))}
        </span>
      }
    >
      <CompositionBar agent={context.agent} groups={groups} />
      <div className="flex flex-col gap-2">
        {groups.map((g) => (
          <div key={g.id} className="flex flex-col gap-0.5">
            <div className={`${rowClass(window !== null)} font-medium`}>
              <span className="flex min-w-0 items-center gap-1.5">
                <span
                  aria-hidden
                  className="size-1.5 shrink-0 rounded-full"
                  style={groupColor(context.agent, g.id)}
                />
                <span className="truncate">{t.context.groups[g.id]}</span>
              </span>
              <ValueCells tokens={g.tokens} isEstimated={g.isEstimated} window={window} />
            </div>
            <ItemList
              items={isExpanded ? g.items : g.items.slice(0, TOP_PER_GROUP)}
              window={window}
            />
          </div>
        ))}
        {onDemand.length > 0 && (
          <div className="flex flex-col gap-0.5">
            <p className="text-xs text-muted-foreground tabular-nums">
              {t.context.onDemand(
                onDemand.length,
                onDemandTokens > 0 ? integer.format(onDemandTokens) : null,
              )}
            </p>
            {isExpanded && <ItemList items={onDemand} window={window} />}
          </div>
        )}
      </div>
      {hiddenCount > 0 && (
        <button
          type="button"
          aria-expanded={isOpen}
          onClick={() => setIsOpen(!isOpen)}
          className={`${SUMMARY_CLASS} flex items-center gap-0.5 text-xs text-muted-foreground`}
        >
          <ChevronDown className={`size-3 ${isOpen ? "rotate-180" : ""}`} aria-hidden />
          {isOpen ? t.context.less : t.context.more(hiddenCount)}
        </button>
      )}
      {isExpanded && (
        <div className="flex flex-col gap-0.5 text-xs text-muted-foreground tabular-nums">
          {totalShare && (
            <span title={t.info.estimated}>{t.context.limitRange(shareRange(totalShare))}</span>
          )}
          {context.firstRequestTokens !== null && (
            <span>{t.context.firstRequest(integer.format(context.firstRequestTokens))}</span>
          )}
          {remainderShare && (
            <span title={t.info.estimated}>
              {t.context.remainder(integer.format(remainder), shareRange(remainderShare))}
            </span>
          )}
          {context.firstRequestChars !== null && (
            <span>
              {t.context.assembled(
                integer.format(context.firstRequestChars),
                integer.format(Math.round(context.firstRequestChars / charsPerToken)),
              )}
            </span>
          )}
          <Warnings warnings={context.warnings} />
        </div>
      )}
    </InfoSection>
  );
}

function Warnings({ warnings }: { warnings: string[] }) {
  return warnings.map((warning) => <p key={warning}>{warning}</p>);
}

const NO_SHARE: ShareOf = () => null;

/** Codex·Claude만 한도 몫을 붙인다. 환율이 `estimated`가 아니면 `shareOf`가 null이라 붙지 않는다. */
function shareFor(rate: CodexRate | null): ShareOf {
  if (!rate) return NO_SHARE;
  return (tokens) => shareOf(tokens, rate);
}

/**
 * 에이전트 하나의 세션 시작 컨텍스트. 마지막 값은 메모리에 남고(`contextStore`), Rust가 갱신하면 event로 바뀐다.
 * `charsPerToken`: 문자 수 → 토큰 추정 비율(settings `tuning.bytesPerToken`).
 * 한도 몫(%)은 `ratesStore`의 1%당 토큰·사용 패턴(Codex·Claude)으로 계산한다. 조회는 옆의 `RateLine`이 하고 여기서는 읽기만 한다.
 */
export function ContextList({
  agent,
  charsPerToken,
}: {
  agent: Agent;
  charsPerToken: number;
  /** @deprecated 쓰지 않는다. 묶음마다 `TOP_PER_GROUP`줄만 보이고 나머지는 펼친다. 호출하는 쪽에서 지운다. */
  maxItems?: number;
}) {
  const { value: snapshot, error } = useSnapshot(contextStore, agent);
  const { value: rates } = useSyncExternalStore(ratesStore.subscribe, ratesStore.getSnapshot);
  const share = shareFor(tokenRateOf(rates, agent));
  // null: 응답에 이 에이전트가 없음.
  const context = snapshot && (snapshot.agents.find((a) => a.agent === agent) ?? null);

  const aside = snapshot && <SnapshotAge computedAt={snapshot.computedAt} />;

  if (context && context.items.length > 0) {
    return (
      <ContextBody context={context} charsPerToken={charsPerToken} share={share} aside={aside} />
    );
  }
  return (
    <InfoSection title={t.common.startupContext} aside={aside}>
      {!snapshot && error && <StateText kind="error">{error}</StateText>}
      {!snapshot && !error && <StateText kind="loading" />}
      {context === null && <StateText kind="empty" />}
      {context && <p className="text-xs text-muted-foreground">{t.context.nothingFound}</p>}
      {context && (
        <div className="text-xs text-muted-foreground">
          <Warnings warnings={context.warnings} />
        </div>
      )}
    </InfoSection>
  );
}
