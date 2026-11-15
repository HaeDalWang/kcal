import { t } from "@/lib/i18n";

const SIZE = 48;
const STROKE = 6;
const RADIUS = (SIZE - STROKE) / 2;

type DialProps = {
  /** 남은 % (0~100). 값이 없으면 부르는 쪽이 다이얼을 숨긴다. */
  percent: number;
  color: string;
  isLow: boolean;
};

/** 원형 다이얼: 트랙 위에 `percent`만큼 채우고 가운데에 숫자, 그 아래 작게 "남음". 낮으면 destructive(이름 줄에 "낮음" 글자가 같이 붙는다). */
export function Dial({ percent, color, isLow }: DialProps) {
  const rounded = Math.round(percent);
  return (
    <div
      role="img"
      aria-label={t.popover.dial(rounded)}
      className={`relative shrink-0 ${isLow ? "text-destructive" : ""}`}
      style={{ width: SIZE, height: SIZE }}
    >
      <svg width={SIZE} height={SIZE} aria-hidden className="-rotate-90">
        <circle
          cx={SIZE / 2}
          cy={SIZE / 2}
          r={RADIUS}
          fill="none"
          strokeWidth={STROKE}
          className="stroke-foreground/10"
        />
        <circle
          cx={SIZE / 2}
          cy={SIZE / 2}
          r={RADIUS}
          fill="none"
          strokeWidth={STROKE}
          strokeLinecap="round"
          pathLength={100}
          strokeDasharray={`${Math.max(percent, 0.5)} 100`}
          stroke={isLow ? "currentColor" : color}
        />
      </svg>
      <span
        aria-hidden
        className="absolute inset-0 flex flex-col items-center justify-center gap-px tabular-nums"
      >
        <span className="flex items-baseline">
          <span className="text-[13px] leading-none font-semibold">{rounded}</span>
          <span className="text-[10px] leading-none">%</span>
        </span>
        <span className="text-[9px] leading-none text-muted-foreground">{t.popover.dialUnit}</span>
      </span>
    </div>
  );
}
