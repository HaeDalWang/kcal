// 개발용 미리보기: 팝오버(폭 310)를 IPC fixture로 브라우저에 그린다(Tauri 없이 메뉴 "점검" 알림 확인용).
// 대시보드 미리보기와 같은 방법으로 연다(src/test/preview/popover.html).
// 기본은 이 Mac 카드(알림 1개, 점 있음), `?s=none`이면 그대로 두기·숨긴 카드만(점 없음). 앱 번들에는 들어가지 않는다.
import { mockIPC } from "@tauri-apps/api/mocks";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "@/App";
import type { Settings } from "@/lib/ipc";
import { followSystemTheme } from "@/lib/theme";
import settings from "@/test/fixtures/ipc/settings.json";
import usage from "@/test/fixtures/ipc/usage-snapshot.json";
import { ALL_HIDDEN, DASHBOARD_CHECKS } from "@/test/preview/checks-fixtures";
import "@/index.css";

const isNone = new URLSearchParams(location.search).get("s") === "none";
const checks = isNone ? ALL_HIDDEN : DASHBOARD_CHECKS;
const allAgents: Settings = {
  ...(settings as Settings),
  enabledAgents: ["claude", "codex", "kiro"],
};

mockIPC(
  (cmd) => {
    switch (cmd) {
      case "get_settings":
        return allAgents;
      case "get_usage":
        // fixture 끝의 대기 중 Claude는 직렬화 계약용이다. 실제 스냅샷은 에이전트당 한 줄.
        return {
          agents: usage.agents.filter(
            (a, i, all) => all.findIndex((b) => b.agent === a.agent) === i,
          ),
        };
      case "get_checks":
        return checks;
      default:
        return null;
    }
  },
  { shouldMockEvents: true },
);

followSystemTheme();
const root = document.getElementById("root");
if (!root) throw new Error("#root 요소가 없습니다.");
createRoot(root).render(
  <StrictMode>
    <div className="w-[310px] border bg-background text-foreground">
      <App />
    </div>
  </StrictMode>,
);
