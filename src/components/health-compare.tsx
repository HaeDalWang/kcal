import type { ReactNode } from "react";
// 대시보드 "처방 효과"(docs/decisions/0016): 위 주간 요약(최근 N일 vs 그 전 N일), 아래 적용 기록마다 전후.
// 값은 Rust `get_health`와 `health-updated` event로만 바뀐다(타이머 없음). 숫자는 모두 기록에서 잰 값이다.
import { CARD } from "@/components/daily-activity";
import { StateText } from "@/components/state-text";
import { AGENT_COLOR, AGENT_LABEL, hasFeature } from "@/lib/agents";
import { appliedName } from "@/lib/check-copy";
import {
  getHealth,
  onHealthUpdated,
  type Agent,
  type HealthReport,
  type PeriodStats,
  type PeriodRate,
  type Bounds,
  type PrescriptionEffect,
} from "@/lib/ipc";
import { createSnapshotStore, useSnapshot } from "@/lib/snapshot-store";
import { numberFormat, t } from "@/lib/i18n";

export const healthStore = createSnapshotStore<HealthReport>({
  fetch: getHealth,
  listen: onHealthUpdated,
  computedAt: (report) => report.computedAt,
});

// 토큰 수는 언어와 상관없이 K·M으로(다른 화면의 "~34.2K 토큰"과 같게). 한국어 축약("6만")은 섞지 않는다.
const compact = new Intl.NumberFormat("en-US", { notation: "compact", maximumFractionDigits: 1 });
const amount = numberFormat({ maximumFractionDigits: 1 });
/** 이보다 작은 변화(%)는 "비슷"으로 본다. */
const SAME_PERCENT = 1;

type Metric = {
  label: string;
  unit: string;
  pick: (p: PeriodStats) => number | null;
  format: (value: number) => string;
};

const perDay = (count: number | null, p: PeriodStats) =>
  count === null || p.elapsedDays <= 0 ? null : count / p.elapsedDays;

/** 에이전트별 지표. 모두 적을수록 아낀 것이다. */
function metricsOf(agent: Agent): Metric[] {
  if (hasFeature(agent, "creditLimit")) {
    return [
      { label: t.health.perQuestion, unit: "", pick: (p) => p.avgCredits, format: amount.format },
      { label: t.health.dailyCredits, unit: "", pick: (p) => p.dailyUsed, format: amount.format },
    ];
  }
  const common: Metric[] = [
    {
      label: t.health.inputPerRequest,
      unit: t.health.tokensUnit,
      pick: (p) => p.avgInputTokens,
      format: compact.format,
    },
    { label: t.health.dailyWeekly, unit: "%", pick: (p) => p.dailyUsed, format: amount.format },
  ];
  if (!hasFeature(agent, "behaviorDetails")) return common;
  return [
    ...common,
    {
      label: t.health.compactions,
      unit: t.health.timesUnit,
      pick: (p) => perDay(p.compactions, p),
      format: amount.format,
    },
  ];
}

export type Change = { text: string; isUp: boolean | null };

/** 전 → 후 변화. 한쪽이 없거나 전이 0이면 null. 1% 미만은 "비슷"(isUp = null). */
export function changeOf(before: number | null, after: number | null): Change | null {
  if (before === null || after === null || before <= 0) return null;
  const percent = ((after - before) / before) * 100;
  if (Math.abs(percent) < SAME_PERCENT) return { text: t.health.same, isUp: null };
  const isUp = percent > 0;
  return { text: `${isUp ? "↑" : "↓"}${Math.round(Math.abs(percent))}%`, isUp };
}

function timeText(iso: string): string {
  return t.health.time(new Date(iso));
}

/** 후 구간 상태: "되돌림 11월 3일 21:00", 덜 지났으면 "3일째", 다 지났으면 "7일 지남". */
export function afterText(effect: PrescriptionEffect, compareDays: number): string {
  if (effect.undoneAt) return t.health.undone(timeText(effect.undoneAt));
  const days = effect.after.elapsedDays;
  if (days >= compareDays) return t.health.passed(compareDays);
  return t.health.day(Math.max(1, Math.ceil(days)));
}

/** 전·후 중 기록이 구간을 다 못 덮은 쪽이 있는가. */
export function isPartial(before: PeriodStats, after: PeriodStats): boolean {
  return [before, after].some((p) => p.coverage !== null && p.coverage < 1);
}

function MetricValue({
  metric,
  before,
  after,
}: {
  metric: Metric;
  before: PeriodStats;
  after: PeriodStats;
}) {
  const [b, a] = [metric.pick(before), metric.pick(after)];
  const show = (value: number | null) => (value === null ? "—" : metric.format(value));
  const change = changeOf(b, a);
  return (
    <div className="flex min-w-0 flex-col">
      <dt className="text-xs text-muted-foreground">{metric.label}</dt>
      <dd className="text-sm whitespace-nowrap tabular-nums" title={t.info.measured}>
        <span className="text-muted-foreground">{show(b)}</span>
        {" → "}
        <span className="font-semibold">{show(a)}</span>
        {metric.unit && (b !== null || a !== null) && (
          <span className="text-xs text-muted-foreground">{` ${metric.unit}`}</span>
        )}
        {change && (
          <span
            className={`ml-1.5 text-xs ${change.isUp ? "text-warn" : change.isUp === false ? "font-medium" : "text-muted-foreground"}`}
          >
            {change.text}
          </span>
        )}
      </dd>
    </div>
  );
}

function Comparison({
  agent,
  before,
  after,
}: {
  agent: Agent;
  before: PeriodStats;
  after: PeriodStats;
}) {
  const metrics = metricsOf(agent);
  // 전후 모두 값이 없으면 "— → —" 줄을 늘어놓지 않고 "기록 없음" 한 줄만 둔다.
  if (metrics.every((m) => m.pick(before) === null && m.pick(after) === null)) {
    return <p className="text-sm text-muted-foreground">{t.common.noRecord}</p>;
  }
  return (
    <dl className="grid grid-cols-[repeat(auto-fill,minmax(10.5rem,1fr))] gap-x-4 gap-y-1.5">
      {metrics.map((metric) => (
        <MetricValue key={metric.label} metric={metric} before={before} after={after} />
      ))}
    </dl>
  );
}

function SummaryRow({
  label,
  before,
  after,
  estimated = false,
}: {
  label: string;
  before: ReactNode;
  after: ReactNode;
  estimated?: boolean;
}) {
  return (
    <div className="flex min-w-0 flex-col gap-0.5">
      <dt className="text-xs text-muted-foreground">{label}</dt>
      <dd
        className="grid grid-cols-[minmax(0,max-content)_auto_minmax(0,1fr)] items-start gap-2 text-sm tabular-nums"
        title={estimated ? t.info.estimated : t.info.measured}
      >
        <div className="min-w-0 text-muted-foreground">{before}</div>
        <span aria-hidden>→</span>
        <div className="min-w-0 font-medium">{after}</div>
      </dd>
    </div>
  );
}

function modelShares(period: PeriodStats): ReactNode {
  return (
    period.modelInputShares?.map(({ model, share }) => (
      <div key={model ?? "other"} className="break-words">
        {model === null
          ? t.health.otherModels
          : model === "unknown"
            ? t.health.unknownModel
            : model}{" "}
        <span className="whitespace-nowrap">{amount.format(share)}%</span>
      </div>
    )) ?? "—"
  );
}

function rateText(rate: PeriodRate | null): string {
  if (!rate) return "—";
  const tokens = (value: number | null) => (value === null ? "—" : `~${compact.format(value)}`);
  const bounds = ({ min, max }: Bounds) =>
    min === null
      ? "—"
      : max === null
        ? `≥~${compact.format(min)}`
        : `~${compact.format(min)}–${compact.format(max)}`;
  let text = "—";
  if (rate.status === "estimated" && rate.perPercent) {
    const { cachedInput, uncachedInput, output } = rate.perPercent;
    text = t.rate.detail(tokens(cachedInput), tokens(uncachedInput), tokens(output));
  } else if (rate.status === "range_only" && rate.foldRange) {
    const { cachedInput, uncachedInput, output } = rate.foldRange;
    text = t.rate.detail(bounds(cachedInput), bounds(uncachedInput), bounds(output));
  }
  return `${text} · ${t.rate.windows(rate.windows)}`;
}

/** 주간 요약에만 보인다. 비중 변화는 절약/낭비로 판정하지 않는다. */
function WeeklyDetails({
  agent,
  before,
  after,
}: {
  agent: Agent;
  before: PeriodStats;
  after: PeriodStats;
}) {
  if (agent === "kiro") return null;
  const share = (value: number | null) => (value === null ? "—" : `${amount.format(value)}%`);
  return (
    <>
      <dl className="flex min-w-0 flex-col gap-2 border-t pt-2">
        <SummaryRow
          label={`${t.health.automated(agent === "claude" ? "claude -p" : "codex exec")} · ${t.health.inputShare}`}
          before={share(before.automatedInputShare)}
          after={share(after.automatedInputShare)}
        />
        <SummaryRow
          label={t.health.modelShares}
          before={modelShares(before)}
          after={modelShares(after)}
        />
        {agent === "codex" && (
          <SummaryRow
            label={t.rate.perPercent}
            before={rateText(before.codexRate)}
            after={rateText(after.codexRate)}
            estimated
          />
        )}
      </dl>
      {agent === "claude" && (
        <p className="text-xs text-muted-foreground">{t.health.claudeUsage}</p>
      )}
    </>
  );
}

function PartialNote({ before, after }: { before: PeriodStats; after: PeriodStats }) {
  if (!isPartial(before, after)) return null;
  return (
    <span
      className="rounded-md bg-muted px-2 py-0.5 text-xs whitespace-nowrap text-muted-foreground"
      title={t.health.partialTitle}
    >
      {t.health.partial}
    </span>
  );
}

function AgentName({ agent }: { agent: Agent }) {
  return (
    <span className="inline-flex items-center gap-1.5 font-medium text-foreground">
      <span
        aria-hidden
        className="size-2 rounded-full"
        style={{ backgroundColor: AGENT_COLOR[agent] }}
      />
      {AGENT_LABEL[agent]}
    </span>
  );
}

function PrescriptionCard({
  effect,
  compareDays,
}: {
  effect: PrescriptionEffect;
  compareDays: number;
}) {
  return (
    <li className="flex min-w-0 flex-col gap-2 border-t pt-2.5">
      <div className="flex items-start gap-2">
        <div className="flex min-w-0 flex-1 flex-col">
          <span className="truncate font-medium" title={effect.name}>
            {appliedName(effect)}
          </span>
          <span className="text-xs text-muted-foreground tabular-nums">
            <AgentName agent={effect.agent} />
            {t.health.applied(timeText(effect.at), afterText(effect, compareDays))}
          </span>
        </div>
        <PartialNote before={effect.before} after={effect.after} />
      </div>
      <Comparison agent={effect.agent} before={effect.before} after={effect.after} />
    </li>
  );
}

/** 처방 효과 제목 id. 점검 탭 "효과 보기"가 여기로 스크롤한다. */
export const HEALTH_TITLE_ID = "health-title";

/** `refreshKey`가 바뀌면(비교 기간·켠 에이전트) 다시 부른다. */
export function HealthCompare({ refreshKey, only }: { refreshKey: string; only?: Agent | null }) {
  const { value: report, error } = useSnapshot(healthStore, refreshKey);
  const prescriptions = report?.prescriptions.filter((e) => !only || e.agent === only) ?? [];
  return (
    <section aria-labelledby={HEALTH_TITLE_ID} className={`${CARD} flex flex-col gap-4 p-5`}>
      <header className="flex flex-col gap-0.5">
        <h2 id={HEALTH_TITLE_ID} className="scroll-mt-9 font-semibold">
          {t.health.title}
        </h2>
        <p className="text-sm text-muted-foreground">
          {report ? t.health.description(report.compareDays) : t.health.descriptionLoading}
        </p>
      </header>
      {error && <StateText kind="error">{error}</StateText>}
      {!report && !error && <StateText kind="loading" />}
      {report && (
        <>
          <div className="flex flex-col gap-2">
            <h3 className="text-sm font-medium">{t.health.weekly(report.compareDays)}</h3>
            <ul className="flex flex-col gap-2">
              {report.weekly
                .filter((c) => !only || c.agent === only)
                .map((c) => (
                  <li
                    key={c.agent}
                    className="grid grid-cols-1 items-start gap-x-4 gap-y-1.5 border-t pt-2.5 min-[900px]:grid-cols-[6.5rem_1fr]"
                  >
                    <div className="flex flex-wrap items-center gap-1.5">
                      <AgentName agent={c.agent} />
                      <PartialNote before={c.before} after={c.after} />
                    </div>
                    <div className="flex min-w-0 flex-col gap-2">
                      <Comparison agent={c.agent} before={c.before} after={c.after} />
                      <WeeklyDetails agent={c.agent} before={c.before} after={c.after} />
                    </div>
                  </li>
                ))}
            </ul>
          </div>
          <div className="flex flex-col gap-2">
            <h3 className="text-sm font-medium">{t.health.byChange}</h3>
            {prescriptions.length === 0 ? (
              <p className="text-sm text-muted-foreground">{t.health.none}</p>
            ) : (
              <ul className="grid grid-cols-1 gap-2 min-[900px]:grid-cols-2">
                {prescriptions.map((effect) => (
                  <PrescriptionCard
                    key={effect.id}
                    effect={effect}
                    compareDays={report.compareDays}
                  />
                ))}
              </ul>
            )}
          </div>
        </>
      )}
    </section>
  );
}
