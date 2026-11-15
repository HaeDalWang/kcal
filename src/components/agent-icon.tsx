import { useId } from "react";

import type { Agent } from "@/lib/ipc";

/** kcal 불꽃(docs/assets/icon.svg). */
const FLAME =
  "M256 70 C300 146,388 196,388 314 C388 392,328 446,256 446 C184 446,124 392,124 314 C124 244,168 204,196 154 C204 204,224 230,248 240 C240 184,236 126,256 70 Z";

/** SVG mask 밝기 값(보이게·뚫기). 화면 색이 아니다(docs/decisions/0019 "mask 흑백"). */
const MASK_SHOW = "#fff";
const MASK_CUT = "#000";

/** 불꽃에 뚫는 머리글자 C·X·K. 상표 로고가 아니다. */
const LETTER: Record<Agent, string> = {
  claude: "M304 268 A72 72 0 1 0 304 378",
  codex: "M204 264 L308 384 M308 264 L204 384",
  kiro: "M210 256 L210 390 M306 256 L224 330 M248 316 L310 390",
};

/** 탭용 에이전트 아이콘. 글자색(currentColor)을 따른다. */
export function AgentIcon({ agent, className }: { agent: Agent; className?: string }) {
  // url(#…)에 넣을 수 있게 id에서 영숫자 외 문자를 뺀다.
  const mask = `agent-icon-${useId().replace(/[^\w-]/g, "")}`;
  return (
    <svg viewBox="62 64 388 388" aria-hidden className={className}>
      <mask id={mask}>
        <rect width="512" height="512" fill={MASK_SHOW} />
        <path
          d={LETTER[agent]}
          fill="none"
          stroke={MASK_CUT}
          strokeWidth="58"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      </mask>
      <path d={FLAME} fill="currentColor" mask={`url(#${mask})`} />
    </svg>
  );
}
