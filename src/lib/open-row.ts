// 점검 탭 전체에서 펼친 줄은 하나(강조 버튼 하나, design.md 6절 3). 키는 "check:<id>"(최적화) / "switch:<target>"(진단하기).
// 점검 탭이 하나를 들고 두 칸에 내려준다. 칸을 따로 그리면(테스트) 칸 안 상태로 같은 동작을 한다.
import { useCallback, useState } from "react";

export type OpenRow = {
  /** 펼친 줄의 키. 모두 접혔으면 null. */
  key: string | null;
  /** 그 줄을 펼친다. 이미 펼친 줄이면 접는다. */
  toggle: (key: string) => void;
};

export const checkRowKey = (id: string) => `check:${id}`;
export const switchRowKey = (target: string) => `switch:${target}`;

/** `shared`가 있으면 그것을, 없으면 이 칸 안 상태를 쓴다. */
export function useOpenRow(shared?: OpenRow): OpenRow {
  const [key, setKey] = useState<string | null>(null);
  const toggle = useCallback((next: string) => setKey((open) => (open === next ? null : next)), []);
  return shared ?? { key, toggle };
}
