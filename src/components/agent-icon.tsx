// 탭·대시보드의 에이전트 표시: 각 회사 마크를 글자색(currentColor) 한 색으로 그린다(결정 0025).
// 경로 출처: CodexBar(github.com/steipete/CodexBar, MIT) `Sources/CodexBar/Resources/ProviderIcon-{claude,codex,kiro}.svg`.
// 마크 자체는 각 회사(Anthropic·OpenAI·Amazon)의 상표다. 어느 서비스의 값인지 알리는 데만 쓴다.
import { AGENT_DESCRIPTORS } from "@/lib/agents";
import type { Agent } from "@/lib/ipc";

/** 탭용 에이전트 아이콘. 글자색(currentColor)을 따른다. */
export function AgentIcon({ agent, className }: { agent: Agent; className?: string }) {
  const { mark } = AGENT_DESCRIPTORS[agent];
  return (
    <svg viewBox={mark.viewBox} aria-hidden className={className}>
      <path d={mark.d} fill="currentColor" transform={mark.transform} />
    </svg>
  );
}
