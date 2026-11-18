// 대시보드 일일 활동(docs/decisions/0015): Rust가 낸 UTC 시간 버킷을 지역 날짜·시각으로 묶는다.
// Rust `time`은 local offset을 끄고 쓰므로(0010) 지역 경계는 여기서 자른다. 모두 순수 함수.
import type { Agent, AgentUsage, HourBucket, Tuning } from "@/lib/ipc";
import { kiroMonthlyLimit } from "@/lib/breakdown";
import { t } from "@/lib/i18n";
import { hasFeature } from "@/lib/agents";

export const HOURS_PER_DAY = 24;

/** 하루 하나의 합. `requests`는 지역 시각(0~23시)별. */
export type DayActivity = {
  used: number;
  requests: number[];
};

/** 지역 날짜의 자정. */
export function startOfLocalDay(time: number | Date): Date {
  const date = new Date(time);
  return new Date(date.getFullYear(), date.getMonth(), date.getDate());
}

/** `today`가 들어간 날까지 최근 `count`일의 지역 자정. 오래된 순. */
export function recentDays(today: number | Date, count: number): Date[] {
  const last = startOfLocalDay(today);
  return Array.from(
    { length: count },
    (_, i) => new Date(last.getFullYear(), last.getMonth(), last.getDate() - (count - 1 - i)),
  );
}

/** 같은 지역 날짜인가. */
export function isSameLocalDay(a: Date, b: Date): boolean {
  return (
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate()
  );
}

/**
 * `day`(지역 날짜)에 시작한 버킷의 쓴 양 합과 지역 시각별 요청 수.
 * `ponytail:` 30·45분 단위 시간대는 UTC 정시 버킷이 지역 정시와 어긋나 한 시간씩 밀린다. 필요하면 Rust 버킷을 15분으로.
 */
export function dayActivity(hourly: readonly HourBucket[], day: Date): DayActivity {
  const requests = new Array<number>(HOURS_PER_DAY).fill(0);
  let used = 0;
  for (const bucket of hourly) {
    const start = new Date(bucket.startsAt);
    if (!isSameLocalDay(start, day)) continue;
    used += bucket.used;
    requests[start.getHours()] += bucket.requests;
  }
  return { used, requests };
}

/** 그 달의 일수. */
export function daysInMonth(day: Date): number {
  return new Date(day.getFullYear(), day.getMonth() + 1, 0).getDate();
}

/**
 * 하루 몫. Claude·Codex = `tuning.dailyGoalPercent`(%p), Kiro = `tuning.kiroDailyCredits`,
 * 0이면 월 한도 ÷ 그 달 일수. 월 한도를 모르면 null(링·몫을 숨긴다).
 */
export function dailyGoal(
  agent: Agent,
  tuning: Pick<Tuning, "dailyGoalPercent" | "kiroDailyCredits">,
  usage: AgentUsage | undefined,
  day: Date,
): number | null {
  if (!hasFeature(agent, "creditLimit")) return tuning.dailyGoalPercent;
  if (tuning.kiroDailyCredits > 0) return tuning.kiroDailyCredits;
  const limit = kiroMonthlyLimit(usage);
  return limit === null ? null : limit / daysInMonth(day);
}

/** 링 채움(0~1). 넘치면 1(꽉 찬 링), 몫을 모르면 0. */
export function ringFraction(used: number, goal: number | null): number {
  if (goal === null || goal <= 0) return 0;
  return Math.min(1, Math.max(0, used / goal));
}

/** 요청이 가장 많은 지역 시각(0~23). 같으면 이른 시각. 모두 0이면 null. */
export function busiestHour(requests: readonly number[]): number | null {
  let best: number | null = null;
  for (let hour = 0; hour < requests.length; hour += 1) {
    if (requests[hour] > 0 && (best === null || requests[hour] > requests[best])) best = hour;
  }
  return best;
}

/** "오후 1:00–2:00"(영어 "1:00–2:00 PM"). 오전·오후가 바뀌면 끝에도 붙인다("오전 11:00–오후 12:00"). */
export function hourRangeText(hour: number): string {
  return t.daily.hourRange(hour);
}

/** "오후 1시"(영어 "1 PM"). 막대 툴팁용. */
export function hourText(hour: number): string {
  return t.daily.hour(hour);
}
