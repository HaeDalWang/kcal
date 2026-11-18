// 지표 줄(대시보드)·최근 구간 막대·시작 컨텍스트 한 줄(트레이). 설명 문장 없이 값만 둔다.
// 트레이 에이전트 탭은 시작 컨텍스트 한 줄만 둔다(요청 수·캐시·구간 막대·구성 목록은 대시보드, 0023).
// 모르는 값은 줄을 숨긴다. 값은 저장소(`metricsStore`·`contextStore`)와 Rust 갱신 event로 바뀐다.
import { useState, useSyncExternalStore } from "react";

import { formatShare, formatTokens } from "@/components/rate-line";
import { AGENT_COLOR } from "@/lib/agents";
import type { Agent, AgentContext, AgentMetrics, UsageBar } from "@/lib/ipc";
import { contextStore, metricsStore, useSnapshot } from "@/lib/snapshot-store";
import { numberFormat, t } from "@/lib/i18n";

const integer = numberFormat({ maximumFractionDigits: 0 });
const credits = numberFormat({ maximumFractionDigits: 2 });
/** 막대가 0이어도 자리가 보이게 하는 최소 높이(%). */
const MIN_BAR_PERCENT = 4;
/** 지난 구간 막대 불투명도. 면 대비 3:1 이상(0019). 지금 구간은 1 + 위 점. */
const PAST_BAR_OPACITY = 0.85;

type Line = { label: string; value: string; title: string };

function isStartupEstimated(context: AgentContext): boolean {
  return context.items.some(
    (i) => i.loading === "startup" && i.tokens !== null && i.source === "estimated",
  );
}

/** "8.5% (34.2K / 400K 토큰)", 창을 모르면 "34.2K 토큰". 예상이 섞이면 숫자 앞에 `~`. */
export function startupValue(context: AgentContext): string {
  const mark = isStartupEstimated(context) ? "~" : "";
  const tokens = formatTokens(context.totalStartupTokens);
  const window = context.contextWindow;
  if (!window) return withEstimateOnly(context, t.common.tokens(`${mark}${tokens}`));
  const percent = formatShare((context.totalStartupTokens / window) * 100);
  return withEstimateOnly(
    context,
    t.metrics.startupValue(mark, percent, tokens, formatTokens(window)),
  );
}

/** Claude는 시스템 프롬프트·내장 도구가 측정돼야 합계다. 측정된 항목이 하나도 없으면 합계처럼 보이지 않게 표시한다. */
function withEstimateOnly(context: AgentContext, value: string): string {
  const isMeasured = context.items.some((i) => i.tokens !== null && i.source === "measured");
  return context.agent === "claude" && !isMeasured ? `${value} ${t.metrics.estimateOnly}` : value;
}

/** 개요 카드의 짧은 값: 영어는 "8.5% of context window"(토큰은 툴팁), 한국어는 `startupValue`와 같다. */
export function startupShortValue(context: AgentContext): string {
  const window = context.contextWindow;
  if (!window) return startupValue(context);
  const mark = isStartupEstimated(context) ? "~" : "";
  const percent = formatShare((context.totalStartupTokens / window) * 100);
  return withEstimateOnly(
    context,
    t.metrics.startupShort(
      mark,
      percent,
      formatTokens(context.totalStartupTokens),
      formatTokens(window),
    ),
  );
}

/** 값 툴팁: 정확/예상 + 분모 출처. 토큰은 바이트 ÷ 4로 환산한 값이라 그렇게 적는다. */
export function startupTitle(context: AgentContext): string {
  const base = isStartupEstimated(context) ? t.info.estimated : t.info.measured;
  const window =
    context.contextWindow &&
    t.metrics.windowTitle(context.model ?? t.metrics.model, formatTokens(context.contextWindow));
  return [base, t.metrics.bytesNote, window].filter(Boolean).join(" · ");
}

/** 요청 수·캐시 적중률 줄. 모르는 값은 뺀다. Kiro는 5시간 한도가 없어 최근 5시간 질문 수다. */
export function metricLines(metrics: AgentMetrics): Line[] {
  const lines: Line[] = [];
  if (metrics.recentRequests !== null) {
    lines.push({
      label: metrics.agent === "kiro" ? t.metrics.recentQuestions : t.metrics.recentRequests,
      value: t.common.times(integer.format(metrics.recentRequests)),
      title: t.info.measured,
    });
  }
  if (metrics.cacheHitRatio !== null) {
    lines.push({
      label: t.metrics.cacheHit,
      value: `${integer.format(metrics.cacheHitRatio * 100)}%`,
      title: t.info.measuredDetail(t.metrics.cacheHitDetail),
    });
  }
  return lines;
}

/** 지표 2열 카드(작은 라벨 / 큰 값). 대시보드 전용. 조회는 옆의 `UsageBars`가 하고 여기서는 읽기만 한다. */
export function MetricLines({ agent }: { agent: Agent }) {
  const { value: metrics } = useSyncExternalStore(metricsStore.subscribe, metricsStore.getSnapshot);
  const agentMetrics = metrics?.agents.find((a) => a.agent === agent);
  const lines = agentMetrics ? metricLines(agentMetrics) : [];
  if (lines.length === 0) return null;
  return (
    <ul aria-label={t.metrics.list} className="grid grid-cols-2 gap-[6px] px-[20px]">
      {lines.map((line) => (
        <li
          key={line.label}
          title={line.title}
          className="flex flex-col gap-px rounded-lg bg-muted/60 px-3 py-2"
        >
          <span className="text-[10px] text-muted-foreground">{line.label}</span>
          <span className="text-[13px] font-semibold tabular-nums">{line.value}</span>
        </li>
      ))}
    </ul>
  );
}

/** 트레이 에이전트 탭의 시작 컨텍스트 카드: 라벨 → 값 한 줄(폭 310에서 접히지 않게 짧은 값, 토큰 수는 툴팁). 구성 목록·한도 몫은 대시보드 `ContextList`. 항목이 없으면 숨긴다(1절). */
export function StartupSummary({ agent }: { agent: Agent }) {
  const { value: contexts } = useSnapshot(contextStore, agent);
  const context = contexts?.agents.find((a) => a.agent === agent);
  if (!context || context.items.length === 0) return null;
  return (
    <div className="mx-[20px] flex flex-col gap-px rounded-lg bg-muted/60 px-3 py-2">
      <span className="text-[10px] text-muted-foreground">{t.common.startupContext}</span>
      <span
        className="text-[13px] font-semibold tabular-nums"
        title={`${startupValue(context)}\n${startupTitle(context)}`}
      >
        {startupShortValue(context)}
      </span>
    </div>
  );
}

/** "11/05 14:00 시작"(지역 시각). */
export function barStartText(startsAt: string): string {
  return t.metrics.barStart(new Date(startsAt));
}

/** 막대 하나의 툴팁: "11/05 14:00 시작 · 31%" 또는 "· 1.25 크레딧". */
export function barTitle(bar: UsageBar, unit: AgentMetrics["barUnit"]): string {
  const value =
    unit === "percent"
      ? `${integer.format(bar.value)}%`
      : t.common.credits(credits.format(bar.value));
  return `${barStartText(bar.startsAt)} · ${value}`;
}

/** 막대 높이(%). 사용 %는 0~100 그대로, 크레딧은 가장 큰 막대 기준. */
function barHeight(value: number, unit: AgentMetrics["barUnit"], max: number): number {
  const ratio = unit === "percent" ? value / 100 : max > 0 ? value / max : 0;
  return Math.max(MIN_BAR_PERCENT, Math.min(100, ratio * 100));
}

/**
 * 최근 구간 막대. 지금 구간은 진하게 + 위에 점(색만으로 구분하지 않는다), 지난 구간은 조금 옅게. `className`으로 높이를 바꾼다(트레이 58px, 대시보드 더 크게).
 * 커서를 올린 막대의 시각·값을 제목 줄 오른쪽("최고 …" 자리)에 바로 보인다. 지표 조회는 여기서 한다(트레이·대시보드 공통).
 */
export function UsageBars({ agent, className = "h-[58px]" }: { agent: Agent; className?: string }) {
  const { value: metrics } = useSnapshot(metricsStore, agent);
  const [hovered, setHovered] = useState<UsageBar | null>(null);
  const agentMetrics = metrics?.agents.find((a) => a.agent === agent);
  if (!agentMetrics || agentMetrics.bars.length === 0) return null;
  const { bars, barUnit } = agentMetrics;
  const max = Math.max(...bars.map((b) => b.value));
  const isPercent = barUnit === "percent";
  const maxText = t.metrics.max(
    isPercent ? `${integer.format(max)}%` : t.common.credits(credits.format(max)),
  );
  return (
    <div className="flex flex-col gap-[10px] px-[20px]">
      <div className="flex items-baseline justify-between text-[10px] text-muted-foreground tabular-nums">
        <span title={isPercent ? t.metrics.recentWindowsHint : undefined}>
          {isPercent ? t.metrics.recentWindows : t.metrics.dailyCredits}
        </span>
        <span title={t.info.measured}>{hovered ? barTitle(hovered, barUnit) : maxText}</span>
      </div>
      <ul
        aria-label={isPercent ? t.metrics.recentWindowsUsage : t.metrics.dailyCredits}
        className={`flex items-end gap-1 ${className}`}
        onPointerLeave={() => setHovered(null)}
      >
        {bars.map((bar) => {
          const title = barTitle(bar, barUnit);
          return (
            <li
              key={bar.startsAt}
              aria-label={title}
              onPointerEnter={() => setHovered(bar)}
              aria-current={bar.isCurrent || undefined}
              className="flex h-full flex-1 items-end"
            >
              <span
                className="relative block w-full rounded-sm"
                style={{
                  height: `${barHeight(bar.value, barUnit, max)}%`,
                  backgroundColor: AGENT_COLOR[agent],
                  opacity: bar.isCurrent ? 1 : PAST_BAR_OPACITY,
                }}
              >
                {bar.isCurrent && (
                  <span
                    aria-hidden
                    className="absolute -top-[6px] left-1/2 size-[4px] -translate-x-1/2 rounded-full bg-foreground"
                  />
                )}
              </span>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
