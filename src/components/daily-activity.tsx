// 대시보드 위쪽(docs/decisions/0015): 요일 줄, 큰 링 + 숫자 칸 + 점선 줄, 시간대 막대.
// 값은 `metricsStore`의 시간 버킷을 지역 날짜로 묶은 것(src/lib/daily.ts). 타이머 없음.
import { ChevronLeft, ChevronRight } from "lucide-react";

import { ActivityRings, type Ring } from "@/components/activity-rings";
import { HourlyChart } from "@/components/hourly-chart";
import { measuredTitle } from "@/components/info-section";
import { Button } from "@/components/ui/button";
import { AGENT_COLOR, AGENT_DESCRIPTORS, AGENT_LABEL, hasFeature } from "@/lib/agents";
import {
  busiestHour,
  dailyGoal,
  dayActivity,
  hourRangeText,
  isSameLocalDay,
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
  /** % 에이전트인데 요청은 있고 쓴 %p는 0이다. 한도 표본이 아직 오르지 않았거나 앱이 꺼져 있던 때라 0 %를 확정할 수 없다. */
  isPending: boolean;
};

function agentDays(
  agents: readonly Agent[],
  metrics: MetricsSnapshot | undefined,
  settings: Settings,
  usages: readonly AgentUsage[] | undefined,
  day: Date,
): AgentDay[] {
  return agents.map((agent) => {
    const m = metrics?.agents.find((a) => a.agent === agent);
    const activity = dayActivity(m?.hourly ?? [], day);
    const isCredits = hasFeature(agent, "creditLimit");
    const isUnread = !isCredits && m !== undefined && m.bars.length === 0 && activity.used === 0;
    return {
      agent,
      activity,
      goal: dailyGoal(
        agent,
        settings.tuning,
        usages?.find((u) => u.agent === agent),
        day,
      ),
      isCredits,
      // 5시간 막대(`bars`)는 한도 표본으로만 생긴다. 비어 있으면 그날 %도 읽지 못한 것이다.
      isUnread,
      // 한도 %는 표본(앱이 켜져 있을 때만 기록)의 1%p 단위 증가분이라, 오늘은 요청이 있어도 아직 0일 수 있다(0015).
      // 지난날의 0은 확정값이라 오늘만 "집계 전"으로 보인다.
      isPending:
        isSameLocalDay(day, new Date()) &&
        !isCredits &&
        !isUnread &&
        activity.used === 0 &&
        activity.requests.some((n) => n > 0),
    };
  });
}

function rings(values: readonly AgentDay[]): Ring[] {
  return values.flatMap((v) =>
    v.goal === null || v.isUnread || v.isPending
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
      if (v.isPending) return `${AGENT_LABEL[v.agent]} ${t.daily.usagePending}`;
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
  usages: readonly AgentUsage[] | undefined;
};

export function DailyActivity({ days, selected, onSelect, settings, usages }: Props) {
  const { value: metrics, error } = useSnapshot(metricsStore, "daily");
  const agents = settings.enabledAgents;
  const week = days.map((day) => agentDays(agents, metrics, settings, usages, day));
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
      <div className="grid grid-cols-1 items-start gap-3 min-[900px]:grid-cols-2">
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
    <section aria-label={t.daily.usedToday} className={`${CARD} flex flex-col gap-4 p-5`}>
      <div className="flex items-center gap-5">
        <ActivityRings
          rings={rings(values)}
          size={104}
          stroke={10}
          gap={3}
          label={ringsLabel(values)}
        />
        <ul className="flex min-w-0 flex-1 flex-col gap-2.5">
          {values.map((v) => {
            const isOver = v.goal !== null && v.activity.used > v.goal;
            const isQuiet = isEmpty || v.isPending;
            return (
              <li
                key={v.agent}
                title={measuredTitle(ringsLabel([v], isEmpty))}
                className="flex min-w-0 items-baseline gap-2"
              >
                <span className="flex w-16 shrink-0 items-center gap-1.5 text-sm font-medium">
                  <span
                    aria-hidden
                    className="size-2 rounded-full"
                    style={{ backgroundColor: AGENT_COLOR[v.agent] }}
                  />
                  {AGENT_LABEL[v.agent]}
                </span>
                {/* 기록이 없거나 아직 집계 전이면 큰 숫자 대신 조용한 글씨(확정된 0을 내지 않는다). */}
                {isQuiet || v.isUnread ? (
                  <>
                    {!isEmpty && (
                      <span className="text-lg leading-tight font-semibold text-muted-foreground">
                        —
                      </span>
                    )}
                    <span className="text-sm text-muted-foreground">
                      {isEmpty
                        ? t.common.noRecord
                        : v.isUnread
                          ? t.daily.limitUnread
                          : t.daily.usagePending}
                    </span>
                  </>
                ) : (
                  <>
                    <span
                      className={`text-lg leading-tight font-semibold tabular-nums ${isOver ? "text-warn" : ""}`}
                    >
                      {amount.format(v.activity.used)}
                    </span>
                    <span className="text-xs text-muted-foreground">
                      {t.daily.unit(v.isCredits)}
                    </span>
                    {/* 넘침을 색만으로 알리지 않는다(6절 3). */}
                    {isOver && (
                      <span className="text-xs font-medium text-warn">{t.daily.over}</span>
                    )}
                  </>
                )}
                {/* 빈 날은 목표를 보이지 않는다(design.md 1절: 모르는 값은 줄 숨김). */}
                {!isEmpty && (
                  <span className="ml-auto text-xs text-muted-foreground tabular-nums">
                    {v.goal === null
                      ? t.daily.noMonthlyLimit
                      : t.daily.goalLabel(t.daily.amount(amount.format(v.goal), v.isCredits))}
                  </span>
                )}
              </li>
            );
          })}
        </ul>
      </div>
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
  const active = values.filter((v) => busiestHour(v.activity.requests) !== null);
  const quiet = values.filter((v) => busiestHour(v.activity.requests) === null);
  return (
    <section aria-label={t.daily.hourly} className={`${CARD} flex flex-col gap-5 p-5`}>
      {active.map((v) => {
        const isQuestion = AGENT_DESCRIPTORS[v.agent].unit === "question";
        const noun = isQuestion ? t.daily.question : t.daily.request;
        const busiest = busiestHour(v.activity.requests) ?? 0;
        const summary = t.daily.busiestAt(
          hourRangeText(busiest),
          integer.format(v.activity.requests[busiest]),
          isQuestion,
        );
        return (
          <div key={v.agent} className="flex flex-col gap-2">
            <div>
              <h3 className="font-semibold">
                {AGENT_LABEL[v.agent]} <span className="text-muted-foreground">{noun}</span>
              </h3>
              <p className="text-sm text-muted-foreground tabular-nums">{summary}</p>
            </div>
            <HourlyChart
              agent={v.agent}
              requests={v.activity.requests}
              noun={noun}
              label={t.daily.chart(AGENT_LABEL[v.agent], noun, summary)}
            />
          </div>
        );
      })}
      {/* 그날 요청이 없는 에이전트는 머리글·빈 축 없이 한 줄로 모은다. */}
      {quiet.length > 0 && (
        <p className="text-sm text-muted-foreground">
          {t.daily.noneAgents(quiet.map((v) => AGENT_LABEL[v.agent]).join(", "))}
        </p>
      )}
    </section>
  );
}
