// 값 칸의 불러오는 중·빈 값·오류 한 벌(docs/design.md 6절 4 "같은 상태는 어디서나 같은 모양").
// 크기는 팝오버·대시보드 모두 11px 하나. 자리 여백만 `className`으로 준다.
import { Info } from "lucide-react";
import type { ReactNode } from "react";

import { t } from "@/lib/i18n";
import type { AgentUsage } from "@/lib/ipc";

const CLASS = {
  loading: "text-[11px] text-muted-foreground",
  empty: "text-[11px] text-pretty text-muted-foreground",
  error: "text-[11px] text-destructive",
  // 할 일이 남은 상태(설치·로그인·허용). 고장이 아니므로 붉게 쓰지 않고 알림 역할도 주지 않는다.
  setup: "flex items-start gap-1 text-[11px] text-muted-foreground",
} as const;

/** 사용량 이유를 어떤 모양으로 보일지. 종류를 모르면 오류로 둔다(숨기는 쪽보다 낫다). */
export function reasonStateKind(usage: Pick<AgentUsage, "reasonKind">): "setup" | "error" {
  return usage.reasonKind === "setup" ? "setup" : "error";
}

type StateTextProps = {
  kind: keyof typeof CLASS;
  /** 오류·할 일 문구. 빈 값은 주면 그 문구(첫 실행 안내 등), 없으면 "값 없음". 불러오는 중은 정해진 문구. */
  children?: ReactNode;
  className?: string;
};

export function StateText({ kind, children, className = "" }: StateTextProps) {
  const text =
    kind === "loading"
      ? t.state.loading
      : kind === "empty"
        ? (children ?? t.state.noValue)
        : children;
  // key: 글자가 바뀌면 새 <p>로 갈아 끼운다. 숨은 팝오버에서 role="alert" 안의 글자만 바꾸면 WebKit이
  // 표시 갱신(display link)을 초당 ~60번 계속 깨운다(첫 실행 유휴 CPU 0.7%). 새로 끼운 요소는 괜찮다.
  return (
    <p
      key={typeof text === "string" ? text : undefined}
      role={kind === "error" ? "alert" : undefined}
      className={`${CLASS[kind]} ${className}`}
    >
      {kind === "setup" && <Info aria-hidden className="mt-[2px] size-[11px] shrink-0" />}
      {kind === "setup" ? <span>{text}</span> : text}
    </p>
  );
}
