import type { ReactNode } from "react";

import { t } from "@/lib/i18n";

type InfoSectionProps = {
  /** 쉬운 말 제목. 접근성 이름도 이것이다. */
  title: string;
  /** 큰 값 한 줄. 없으면 생략. */
  value?: ReactNode;
  /** 값 위 툴팁. 예상: "예상: 기록으로 계산한 값", 측정: "정확: 측정한 값". */
  valueTitle?: string;
  /** 일반 사용자 말 설명 한 줄(들). */
  description?: ReactNode;
  /** 제목 줄 오른쪽(갱신 시각 등). */
  aside?: ReactNode;
  /** 값과 자세히 사이에 항상 보이는 내용. */
  children?: ReactNode;
  /** 통계·근거. 대시보드 창에서만 쓰므로 접지 않고 값 아래에 늘 보인다(docs/decisions/0010). */
  details?: ReactNode;
};

/** 측정 값의 자세한 툴팁: "정확: 11/05 14:00 시작 · 31%". */
export const measuredTitle = (detail: string) => t.info.measuredDetail(detail);
/** 눌러서 여닫는 `<summary>`: 메뉴·버튼과 같은 hover 배경·포커스 링·누름(6절 4). 글자 크기·색은 쓰는 곳이 더한다. */
export const SUMMARY_CLASS =
  "-mx-1 w-fit cursor-default rounded-md px-1 hover:bg-accent hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring focus-visible:outline-none active:translate-y-px";

/** 디자인 틀(docs/decisions/0009): 제목 → 큰 값 → 설명 → 자세히(0010부터 대시보드에서 펼친 채). */
export function InfoSection({
  title,
  value,
  valueTitle,
  description,
  aside,
  children,
  details,
}: InfoSectionProps) {
  return (
    <section role="group" aria-label={title} className="flex flex-col gap-1 border-t px-3 py-2">
      <div className="flex items-baseline justify-between gap-2">
        <h2 className="text-xs font-medium text-muted-foreground">{title}</h2>
        {aside}
      </div>
      {value && (
        <p className="text-sm font-medium tabular-nums" title={valueTitle}>
          {value}
        </p>
      )}
      {description && <div className="text-xs text-muted-foreground">{description}</div>}
      {children}
      {details && (
        <div
          data-slot="details"
          className="mt-1 flex flex-col gap-0.5 text-xs text-muted-foreground tabular-nums"
        >
          {details}
        </div>
      )}
    </section>
  );
}
