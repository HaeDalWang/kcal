// 대시보드 위쪽(docs/decisions/0015): 요일 줄, 큰 링 + 숫자 칸 + 점선 줄, 시간대 막대.
// 값은 `metricsStore`의 시간 버킷을 지역 날짜로 묶은 것(src/lib/daily.ts). 타이머 없음.
import { ChevronLeft, ChevronRight } from "lucide-react";

import { ActivityRings, type Ring } from "@/components/activity-rings";
import { AGENT_COLOR } from "@/components/agent-metrics";
import { HourlyChart } from "@/components/hourly-chart";
import { measuredTitle } from "@/components/info-section";
import { AGENT_LABEL } from "@/components/usage-popover";
import { Button } from "@/components/ui/button";
import {
  busiestHour,
  dailyGoal,
  dayActivity,
  hourRangeText,
  ringFraction,
  type DayActivity,
} from "@/lib/daily";
import type { Agent, AgentUsage, MetricsSnapshot, Settings } from "@/lib/ipc";
import { numberFormat, t } from "@/lib/i18n";
import { metricsStore, useSnapshot } from "@/lib/snapshot-store";

const amount = numberFormat({ maximumFractionDigits: 1 });
const integer = numberFormat({ maximumFractionDigits: 0 });

export const CARD = "rounded-2xl bg-card text-card-foreground ring-1 ring-foreground/10";

/** "11월 6일 (금)", 영어 "Fri, Nov 6". */
export function dateText(day: Date): string {
  return t.daily.date(day);
}

/** 에이전트 하나의 그날 값. `goal`이 null이면 몫을 모른다(Kiro 월 한도 없음). */
type AgentDay = {
  agent: Agent;
  activity: DayActivity;
  goal: number | null;
  /** Kiro는 크레딧, 나머지는 %. */
  isCredits: boolean;
  /** % 에이전트인데 한도 표본이 하나도 없다(로그인 못 읽음 등). 0 %가 아니라 모르는 값이다. */
  isUnread: boolean;
};

function agentDays(
  agents: readonly Agent[],
  metrics: MetricsSnapshot | undefined,
  settings: Settings,
  kiroUsage: AgentUsage | undefined,
  day: Date,
): AgentDay[] {
  return agents.map((agent) => {
    const m = metrics?.agents.find((a) => a.agent === agent);
    const activity = dayActivity(m?.hourly ?? [], day);
    const isCredits = agent === "kiro";
    return {
      agent,
      activity,
      goal: dailyGoal(agent, settings.tuning, kiroUsage, day),
      isCredits,
      // 5시간 막대(`bars`)는 한도 표본으로만 생긴다. 비어 있으면 그날 %도 읽지 못한 것이다.
      isUnread: !isCredits && m !== undefined && m.bars.length === 0 && activity.used === 0,
    };
  });
}

function rings(values: readonly AgentDay[]): Ring[] {
  return values.flatMap((v) =>
    v.goal === null || v.isUnread
      ? []
      : [{ agent: v.agent, fraction: ringFraction(v.activity.used, v.goal) }],
  );
}

/** 그날 어떤 에이전트에도 쓴 양도 요청도 없다. 0 %를 내지 않고 "기록 없음"을 보인다. */
function isEmptyDay(values: readonly AgentDay[]): boolean {
  return values.every((v) => v.activity.used === 0 && v.activity.requests.every((n) => n === 0));
}

/** "Claude 3.5% / 하루 몫 14%, Codex …". 링·요일 버튼의 aria-label. 기록이 없는 날은 "Claude No data". */
function ringsLabel(values: readonly AgentDay[], isEmpty = isEmptyDay(values)): string {
  return values
    .map((v) => {
      if (isEmpty) return `${AGENT_LABEL[v.agent]} ${t.common.noRecord}`;
      if (v.isUnread) return `${AGENT_LABEL[v.agent]} ${t.daily.limitUnread}`;
      const used = t.daily.amount(amount.format(v.activity.used), v.isCredits);
      const goal =
        v.goal === null ? "" : t.daily.goal(t.daily.amount(amount.format(v.goal), v.isCredits));
      return `${AGENT_LABEL[v.agent]} ${used}${goal}`;
    })
    .join(", ");
}

const sumRequests = (values: readonly AgentDay[]) =>
  values.flatMap((v) => v.activity.requests).reduce((a, b) => a + b, 0);

/** 에이전트를 합친 시각별 요청 수. */
function combined(values: readonly AgentDay[]): number[] {
  return Array.from({ length: 24 }, (_, hour) =>
    values.reduce((sum, v) => sum + v.activity.requests[hour], 0),
  );
}

type DayPickerProps = {
  days: readonly Date[];
  selected: number;
  onSelect: (index: number) => void;
};

/** 헤더 가운데 알약: < 오늘 >. 최근 7일 안에서만 움직인다. */
export function DayPicker({ days, selected, onSelect }: DayPickerProps) {
  const last = days.length - 1;
  const text =
    selected === last
      ? t.daily.today
      : selected === last - 1
        ? t.daily.yesterday
        : dateText(days[selected]);
  return (
    <div className="flex h-8 items-center gap-0.5 rounded-lg bg-muted p-[3px]">
      <Button
        variant="ghost"
        size="icon-xs"
        className="rounded-md"
        aria-label={t.daily.previous}
        disabled={selected === 0}
        onClick={() => onSelect(selected - 1)}
      >
        <ChevronLeft className="size-3.5" />
      </Button>
      <span
        aria-live="polite"
        title={dateText(days[selected])}
        className="min-w-28 text-center text-sm font-medium tabular-nums"
      >
        {text}
      </span>
      <Button
        variant="ghost"
        size="icon-xs"
        className="rounded-md"
        aria-label={t.daily.next}
        disabled={selected === last}
        onClick={() => onSelect(selected + 1)}
      >
        <ChevronRight className="size-3.5" />
      </Button>
    </div>
  );
}

type Props = DayPickerProps & {
  settings: Settings;
  kiroUsage: AgentUsage | undefined;
};

export function DailyActivity({ days, selected, onSelect, settings, kiroUsage }: Props) {
  const { value: metrics, error } = useSnapshot(metricsStore, "daily");
  const agents = settings.enabledAgents;
  const week = days.map((day) => agentDays(agents, metrics, settings, kiroUsage, day));
  const today = week[selected];
  return (
    <>
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}
      <ul aria-label={t.daily.week} className="grid grid-cols-7 gap-1">
        {days.map((day, i) => (
          <li key={day.getTime()}>
            <button
              type="button"
              aria-pressed={i === selected}
              aria-label={`${dateText(day)}: ${ringsLabel(week[i])}`}
              onClick={() => onSelect(i)}
              className="group flex w-full flex-col items-center gap-1.5 rounded-2xl py-2 outline-none hover:bg-muted/60 focus-visible:ring-3 focus-visible:ring-ring aria-pressed:bg-muted aria-pressed:ring-1 aria-pressed:ring-foreground/15"
            >
              <span className="text-xs font-medium text-muted-foreground group-aria-pressed:font-semibold group-aria-pressed:text-foreground">
                {t.daily.weekday(day)}
              </span>
              <ActivityRings
                rings={rings(week[i])}
                size={40}
                stroke={4}
                gap={1}
                label={ringsLabel(week[i])}
              />
            </button>
          </li>
        ))}
      </ul>
      <div className="grid grid-cols-1 gap-3 min-[900px]:grid-cols-2">
        <TodayCard values={today} />
        <HourlyCard values={today} />
      </div>
    </>
  );
}

function TodayCard({ values }: { values: readonly AgentDay[] }) {
  const isEmpty = isEmptyDay(values);
  const all = combined(values);
  const busiest = busiestHour(all);
  const activeHours = all.filter((count) => count > 0).length;
  const lines = [
    {
      label: t.daily.totalRequests,
      value: t.common.times(integer.format(sumRequests(values))),
      title: measuredTitle(t.daily.kiroNote),
    },
    {
      label: t.daily.busiest,
      value: busiest === null ? t.common.noRecord : hourRangeText(busiest),
    },
    { label: t.daily.activeHours, value: t.daily.hours(activeHours) },
  ];
  return (
    <section aria-label={t.daily.usedToday} className={`${CARD} flex flex-col gap-5 p-5`}>
      <ActivityRings
        rings={rings(values)}
        size={208}
        stroke={16}
        gap={4}
        label={ringsLabel(values)}
        className="self-center"
      />
      <ul
        className="grid gap-2"
        style={{ gridTemplateColumns: `repeat(${values.length}, minmax(0, 1fr))` }}
      >
        {values.map((v) => {
          const isOver = v.goal !== null && v.activity.used > v.goal;
          return (
            <li
              key={v.agent}
              title={measuredTitle(ringsLabel([v], isEmpty))}
              className="flex min-w-0 flex-col items-center pt-2 pb-1.5"
            >
              <span className="flex items-center gap-1.5 text-sm font-medium">
                <span
                  aria-hidden
                  className="size-2 rounded-full"
                  style={{ backgroundColor: AGENT_COLOR[v.agent] }}
                />
                {AGENT_LABEL[v.agent]}
              </span>
              {/* 빈 날은 "기록 없음"을 숫자 자리 높이 그대로 조용히 둔다(세 칸 모두 큰 글씨면 시끄럽다). */}
              {isEmpty ? (
                <span className="flex h-[1.875rem] items-center text-sm text-muted-foreground">
                  {t.common.noRecord}
                </span>
              ) : (
                <span
                  className={`text-2xl leading-tight font-semibold tabular-nums ${isOver ? "text-warn" : ""}`}
                >
                  {v.isUnread ? "—" : amount.format(v.activity.used)}
                </span>
              )}
              {!isEmpty && !v.isUnread && (
                <span className="text-xs text-muted-foreground">{t.daily.unit(v.isCredits)}</span>
              )}
              {/* 넘침을 색만으로 알리지 않는다(6절 3). */}
              {isOver && <span className="text-xs font-medium text-warn">{t.daily.over}</span>}
              {/* 빈 날은 목표 줄을 보이지 않는다(design.md 1절: 모르는 값은 줄 숨김). */}
              {!isEmpty && (
                <span className="mt-1.5 w-full border-t pt-1 text-center text-xs text-balance text-muted-foreground tabular-nums">
                  {v.isUnread
                    ? t.daily.limitUnread
                    : v.goal === null
                      ? t.daily.noMonthlyLimit
                      : t.daily.goalLabel(t.daily.amount(amount.format(v.goal), v.isCredits))}
                </span>
              )}
            </li>
          );
        })}
      </ul>
      <dl className="flex flex-col gap-2 text-sm">
        {lines.map((line) => (
          <div key={line.label} title={line.title} className="flex items-baseline gap-2">
            <dt className="text-muted-foreground">{line.label}</dt>
            <span
              aria-hidden
              className="flex-1 border-b border-dotted border-muted-foreground/50"
            />
            <dd className="font-semibold tabular-nums">{line.value}</dd>
          </div>
        ))}
      </dl>
    </section>
  );
}

function HourlyCard({ values }: { values: readonly AgentDay[] }) {
  return (
    <section aria-label={t.daily.hourly} className={`${CARD} flex flex-col gap-6 p-5`}>
      {values.map((v) => {
        const noun = v.agent === "kiro" ? t.daily.question : t.daily.request;
        const busiest = busiestHour(v.activity.requests);
        // 그날 요청이 없으면 빈 축 대신 조용한 한 줄만 둔다(에이전트마다).
        const summary =
          busiest === null
            ? t.daily.noneThisDay(v.agent === "kiro")
            : t.daily.busiestAt(
                hourRangeText(busiest),
                integer.format(v.activity.requests[busiest]),
                v.agent === "kiro",
              );
        return (
          <div key={v.agent} className="flex flex-col gap-2">
            <div>
              <h3 className="font-semibold">
                {AGENT_LABEL[v.agent]} <span className="text-muted-foreground">{noun}</span>
              </h3>
              <p className="text-sm text-muted-foreground tabular-nums">{summary}</p>
            </div>
            {busiest !== null && (
              <HourlyChart
                agent={v.agent}
                requests={v.activity.requests}
                noun={noun}
                label={t.daily.chart(AGENT_LABEL[v.agent], noun, summary)}
              />
            )}
          </div>
        );
      })}
    </section>
  );
}
