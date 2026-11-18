// 동심원 링(docs/decisions/0015). 바깥부터 주어진 순서로 하나씩, 채움 = 오늘 쓴 양 ÷ 하루 몫(넘치면 꽉 찬 링).
import { AGENT_COLOR } from "@/components/agent-metrics";
import type { Agent } from "@/lib/ipc";

export type Ring = {
  agent: Agent;
  /** 0~1. */
  fraction: number;
};

/** 트랙에 섞는 에이전트 색 비율(%). 채움과 트랙 대비 3:1 이상(0019, 라이트 claude 3.10 · codex 3.02). */
const TRACK_PERCENT = 12;

/** 트랙: 같은 색을 회색 쪽으로 섞어 채도를 낮춘다(라이트·다크 모두 `--muted` 기준). */
function trackColor(agent: Agent): string {
  return `color-mix(in oklch, ${AGENT_COLOR[agent]} ${TRACK_PERCENT}%, var(--muted))`;
}

type Props = {
  rings: readonly Ring[];
  size: number;
  stroke: number;
  gap: number;
  /** 링 전체의 aria-label/title(값/몫). */
  label: string;
  className?: string;
};

export function ActivityRings({ rings, size, stroke, gap, label, className }: Props) {
  const center = size / 2;
  return (
    <svg
      role="img"
      aria-label={label}
      width={size}
      height={size}
      viewBox={`0 0 ${size} ${size}`}
      className={`shrink-0 -rotate-90 ${className ?? ""}`}
    >
      <title>{label}</title>
      {/* 그릴 링이 없으면(한도를 못 읽음·몫 모름) 빈 자리 대신 회색 트랙 하나를 둔다. */}
      {rings.length === 0 && (
        <circle
          cx={center}
          cy={center}
          r={center - stroke / 2}
          fill="none"
          strokeWidth={stroke}
          stroke="var(--border)"
        />
      )}
      {rings.map((ring, i) => {
        const radius = center - stroke / 2 - i * (stroke + gap);
        if (radius <= 0) return null;
        return (
          <g key={ring.agent} fill="none" strokeWidth={stroke}>
            <circle cx={center} cy={center} r={radius} stroke={trackColor(ring.agent)} />
            {ring.fraction > 0 && (
              <circle
                cx={center}
                cy={center}
                r={radius}
                stroke={AGENT_COLOR[ring.agent]}
                strokeLinecap="round"
                pathLength={100}
                strokeDasharray={`${ring.fraction * 100} 100`}
              />
            )}
          </g>
        );
      })}
    </svg>
  );
}
