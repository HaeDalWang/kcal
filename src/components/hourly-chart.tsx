// 0~24시 시간별 요청 막대(docs/decisions/0015). 높이는 그날 가장 많은 시간 기준.
import { AGENT_COLOR } from "@/components/agent-metrics";
import { HOURS_PER_DAY, hourText } from "@/lib/daily";
import { numberFormat, t } from "@/lib/i18n";
import type { Agent } from "@/lib/ipc";

const SLOT = 10;
const BAR = 4;
const WIDTH = HOURS_PER_DAY * SLOT;
const HEIGHT = 60;
/** 막대 위 여백(가장 긴 막대가 위에 붙지 않게). */
const TOP = 4;
/** 눈금 시각. 이름은 `t.daily.ticks`(같은 순서). */
const TICK_HOURS = [0, 6, 12, 18];

const integer = numberFormat({ maximumFractionDigits: 0 });

type Props = {
  agent: Agent;
  /** 지역 시각(0~23시)별 요청 수. */
  requests: readonly number[];
  /** "요청" 또는 "질문". */
  noun: string;
  label: string;
};

export function HourlyChart({ agent, requests, noun, label }: Props) {
  const max = Math.max(0, ...requests);
  return (
    <div className="flex flex-col gap-1">
      <svg
        role="img"
        aria-label={label}
        viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
        preserveAspectRatio="none"
        className="h-16 w-full overflow-visible text-muted-foreground"
      >
        <title>{label}</title>
        {requests.map((count, hour) => {
          if (count <= 0) return null;
          const height = Math.max(1.5, (count / max) * (HEIGHT - TOP));
          return (
            <rect
              key={hour}
              x={hour * SLOT + (SLOT - BAR) / 2}
              y={HEIGHT - height}
              width={BAR}
              height={height}
              fill={AGENT_COLOR[agent]}
            >
              <title>{t.daily.bar(hourText(hour), noun, integer.format(count))}</title>
            </rect>
          );
        })}
        <line
          x1={0}
          x2={WIDTH}
          y1={HEIGHT}
          y2={HEIGHT}
          stroke="currentColor"
          strokeOpacity={0.4}
          vectorEffect="non-scaling-stroke"
        />
        {[...TICK_HOURS, HOURS_PER_DAY].map((hour) => (
          <line
            key={hour}
            x1={hour * SLOT}
            x2={hour * SLOT}
            y1={HEIGHT}
            y2={HEIGHT + 3}
            stroke="currentColor"
            strokeOpacity={0.4}
            vectorEffect="non-scaling-stroke"
          />
        ))}
      </svg>
      <div aria-hidden className="relative mt-1 h-4 text-[11px] text-muted-foreground tabular-nums">
        {TICK_HOURS.map((hour, i) => (
          <span
            key={hour}
            className="absolute top-0 whitespace-nowrap"
            style={{ left: `${(hour / HOURS_PER_DAY) * 100}%` }}
          >
            {t.daily.ticks[i]}
          </span>
        ))}
        {t.daily.hourUnit && <span className="absolute top-0 right-0">{t.daily.hourUnit}</span>}
      </div>
    </div>
  );
}
