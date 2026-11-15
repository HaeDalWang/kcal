import { formatAge } from "@/lib/snapshot-store";

/** "N분 전 기준". 렌더할 때 계산한다(시계 타이머 없음). 값이 새로 오거나 팝오버를 다시 열 때 갱신된다. */
export function SnapshotAge({ computedAt }: { computedAt: string }) {
  return (
    <span className="text-xs text-muted-foreground tabular-nums">{formatAge(computedAt)}</span>
  );
}
