import { useSyncExternalStore, type ReactNode } from "react";

import { startupTitle, startupValue } from "@/components/agent-metrics";
import { InfoSection } from "@/components/info-section";
import { SnapshotAge } from "@/components/snapshot-age";
import { StateText } from "@/components/state-text";
import { shareOf, tokenRateOf, type Share } from "@/lib/breakdown";
import {
  type Agent,
  type AgentContext,
  type CodexRate,
  type ContextItem,
  type ContextSource,
  type Loading,
} from "@/lib/ipc";
import { contextStore, ratesStore, useSnapshot } from "@/lib/snapshot-store";
import { numberFormat, t } from "@/lib/i18n";

/** 기본으로 이만큼만 줄로 보이고 나머지는 한 줄로 묶는다(`maxItems`로 바꾼다). */
export const TOP_ITEMS = 8;

/** 시작 때 실리지 않는 항목 표시. */
const loadingLabel = (loading: Loading) => (loading === "startup" ? null : t.context.whenNeeded);

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

function ItemRow({ item, share }: { item: ContextItem; share: Share | null }) {
  const loading = loadingLabel(item.loading);
  const isEstimated = item.source === "estimated";
  return (
    <li className="flex items-baseline gap-1.5 text-xs">
      <span className="min-w-0 flex-1 truncate" title={item.path ?? item.name}>
        {item.name}
      </span>
      {loading && <span className="shrink-0 text-muted-foreground">{loading}</span>}
      <span
        className="shrink-0 tabular-nums"
        title={item.tokens === null ? undefined : isEstimated ? t.info.estimated : t.info.measured}
      >
        {item.tokens === null ? "—" : `${isEstimated ? "~" : ""}${integer.format(item.tokens)}`}
      </span>
      {share && (
        <span
          className="shrink-0 whitespace-nowrap text-muted-foreground tabular-nums"
          title={t.info.estimated}
        >
          ~{points.format(share.point)}%
        </span>
      )}
    </li>
  );
}

function ContextBody({
  context,
  charsPerToken,
  share,
  aside,
  maxItems,
}: {
  context: AgentContext;
  charsPerToken: number;
  share: ShareOf;
  aside: ReactNode;
  maxItems: number;
}) {
  const sums = startupTokensBySource(context.items);
  const shown = context.items.slice(0, maxItems);
  const rest = context.items.slice(maxItems);
  const restStartup = startupTokensBySource(rest);
  const totalShare = share(context.totalStartupTokens);
  // 첫 요청 측정값 − 시작 합계: 세지 못한 내장 도구 정의와 첫 사용자 메시지.
  const remainder =
    context.firstRequestTokens === null
      ? 0
      : context.firstRequestTokens - context.totalStartupTokens;
  const remainderShare = remainder > 0 ? share(remainder) : null;
  // 대시보드의 시작 컨텍스트는 이 칸 하나다(팝오버 카드와 같은 값 줄 + 항목별 목록). `MetricLines`는 대시보드에서 이 줄을 빼 두 번 보이지 않는다.
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
        <>
          <span>
            {sums.measured === 0
              ? t.context.estimatedOnly(integer.format(sums.estimated))
              : t.context.sources(integer.format(sums.measured), integer.format(sums.estimated))}
          </span>
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
        </>
      }
    >
      <ul className="flex flex-col gap-0.5">
        {shown.map((item, index) => (
          <ItemRow
            key={`${item.kind}:${item.path ?? item.name}:${index}`}
            item={item}
            share={item.loading === "startup" && item.tokens !== null ? share(item.tokens) : null}
          />
        ))}
      </ul>
      {rest.length > 0 && (
        <p className="text-xs text-muted-foreground tabular-nums">
          {t.context.rest(
            rest.length,
            integer.format(restStartup.measured + restStartup.estimated),
          )}
        </p>
      )}
      <Warnings warnings={context.warnings} />
    </InfoSection>
  );
}

function Warnings({ warnings }: { warnings: string[] }) {
  return warnings.map((warning) => (
    <p key={warning} className="text-xs text-muted-foreground">
      {warning}
    </p>
  ));
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
  maxItems = TOP_ITEMS,
}: {
  agent: Agent;
  charsPerToken: number;
  /** 줄로 보일 항목 수. 나머지는 한 줄로 묶는다. */
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
      <ContextBody
        context={context}
        charsPerToken={charsPerToken}
        share={share}
        aside={aside}
        maxItems={maxItems}
      />
    );
  }
  return (
    <InfoSection title={t.common.startupContext} aside={aside}>
      {!snapshot && error && <StateText kind="error">{error}</StateText>}
      {!snapshot && !error && <StateText kind="loading" />}
      {context === null && <StateText kind="empty" />}
      {context && <p className="text-xs text-muted-foreground">{t.context.nothingFound}</p>}
      {context && <Warnings warnings={context.warnings} />}
    </InfoSection>
  );
}
