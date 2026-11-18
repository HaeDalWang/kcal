// 대시보드 "점검" 탭(docs/decisions/0017 "화면 자리"): 켠 에이전트마다 칸 하나, 왼쪽 "최적화"(점검 카드), 오른쪽 "진단하기"(코칭 스위치, 0014).
// 탭을 열 때와 쓰기(적용·되돌리기·숨김·다시 알리기) 뒤에만 읽는다(타이머 없음).
import { useCallback, useEffect, useRef, useState } from "react";

import { AgentIcon } from "@/components/agent-icon";
import { AgentChecks, HiddenRow, type HandoffContext } from "@/components/checks-view";
import { CoachPanel } from "@/components/coach-view";
import { CARD } from "@/components/daily-activity";
import { StateText } from "@/components/state-text";
import { minRemaining, useUsageSnapshot } from "@/components/usage-popover";
import { AGENT_LABEL } from "@/lib/agents";
import { t } from "@/lib/i18n";
import { useSettings } from "@/lib/use-settings";
import { alertCount, isCheckedAgent, noChecksText } from "@/lib/check-copy";
import { useOpenRow } from "@/lib/open-row";
import {
  errorMessage,
  getChecks,
  getCoachLog,
  getInstalledAgents,
  getSwitches,
  restoreDismissedCheck,
  type Agent,
  type AppliedSwitch,
  type CoachCheck,
  type CoachSwitch,
} from "@/lib/ipc";

const LOADING = <StateText kind="loading" className="px-5 py-2.5" />;
/** 설정을 아직 못 읽었을 때(Rust `Tuning::default()`와 같은 값). */
const DEFAULT_MIN_REMAINING = 20;
const DEFAULT_TIMEOUT_SECS = 300;
const DAY_MS = 86_400_000;

/** 에이전트 칸의 id. 팝오버 "점검"으로 열면 알림 카드가 있는 첫 칸으로 스크롤한다. */
export const checksSectionId = (agent: Agent) => `checks-${agent}`;

type ChecksTabProps = {
  /** 켠 에이전트(`settings.enabledAgents` 순서). */
  agents: Agent[];
  /** 0보다 크고 바뀔 때마다(팝오버 "점검"으로 열림) 다시 읽고 알림 카드가 있는 첫 칸으로 스크롤한다. */
  focusKey: number;
  /** 진단하기 "효과 보기": 사용량 탭의 처방 효과로 간다. */
  onShowEffect: () => void;
};

export function ChecksTab({ agents, focusKey, onShowEffect }: ChecksTabProps) {
  const [checks, setChecks] = useState<CoachCheck[] | null>(null);
  const [switches, setSwitches] = useState<CoachSwitch[] | null>(null);
  // 이 컴퓨터에서 찾은 에이전트. 아직 모르거나 못 읽으면 null(찾은 것으로 본다).
  const [installed, setInstalled] = useState<Agent[] | null>(null);
  const [log, setLog] = useState<AppliedSwitch[]>([]);
  // 적용 기록을 받은 시각(유닉스 ms). 최적화 고친 줄의 기간은 이 시각에서 센다(렌더 중 시계를 읽지 않는다).
  const [logAt, setLogAt] = useState<number | null>(null);
  // 탭 전체에서 펼친 줄 하나(최적화·진단하기 모든 칸, design.md 6절 3).
  const openRow = useOpenRow();
  const [error, setError] = useState<string | null>(null);
  const [isBusy, setIsBusy] = useState(false);
  // 점검 목록을 받은 횟수. 같은 배열이 다시 와도 스크롤 판단이 돈다.
  const [loads, setLoads] = useState(0);
  const pendingScroll = useRef(false);
  // 남은 한도·기준은 이미 있는 출처(get_usage + usage-updated, get_settings + settings-updated)만 쓴다. polling 없음.
  const usage = useUsageSnapshot();
  const { settings } = useSettings();

  const load = useCallback(() => {
    const fail = (reason: unknown) => setError(errorMessage(reason));
    getChecks().then((next) => {
      setChecks(next);
      setLoads((n) => n + 1);
    }, fail);
    getSwitches().then(setSwitches, fail);
    getCoachLog().then((next) => {
      setLog(next);
      setLogAt(Date.now());
    }, fail);
    getInstalledAgents().then(setInstalled, () => setInstalled(null));
  }, []);

  useEffect(() => {
    if (focusKey > 0) pendingScroll.current = true;
    load();
  }, [load, focusKey]);

  useEffect(() => {
    if (!checks || !pendingScroll.current) return;
    pendingScroll.current = false;
    const first = agents.find((agent) => alertCount(checks, [agent]) > 0);
    if (first) document.getElementById(checksSectionId(first))?.scrollIntoView({ block: "start" });
  }, [loads, checks, agents]);

  const restore = (id: string) => {
    setError(null);
    setIsBusy(true);
    restoreDismissedCheck(id).then(
      () => {
        setIsBusy(false);
        load();
      },
      (reason: unknown) => {
        setIsBusy(false);
        setError(errorMessage(reason));
      },
    );
  };

  const hidden = checks?.filter((c) => c.isDismissed && agents.includes(c.agent)) ?? [];
  const claudeUsage = usage.snapshot?.agents.find((a) => a.agent === "claude");
  const handoff: HandoffContext = {
    remaining: claudeUsage ? minRemaining(claudeUsage, usage.now) : null,
    minRemaining: settings?.tuning.claudeHandoffMinRemaining ?? DEFAULT_MIN_REMAINING,
    timeoutSecs: settings?.tuning.claudeHandoffTimeoutSecs ?? DEFAULT_TIMEOUT_SECS,
  };
  // 고친 줄은 옆 진단하기 "최근 D일"과 같은 D(Rust `SwitchUsage.days` = tuning `diagnoseUsageDays`). 설정을 아직 모르면 기간 제한 없음.
  const recentDays = settings?.tuning.diagnoseUsageDays ?? null;
  const fixedSince = recentDays !== null && logAt !== null ? logAt - recentDays * DAY_MS : null;
  const isMissing = (agent: Agent) => agent === "claude" && installed?.includes(agent) === false;
  return (
    <div className="flex flex-col gap-3">
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}
      {agents.map((agent) => (
        <section
          key={agent}
          id={checksSectionId(agent)}
          aria-labelledby={`${checksSectionId(agent)}-title`}
          className={`${CARD} flex scroll-mt-4 flex-col gap-2 p-3`}
        >
          <h2
            id={`${checksSectionId(agent)}-title`}
            className="flex items-center gap-1.5 px-1 font-medium"
          >
            <AgentIcon agent={agent} className="size-5" />
            {AGENT_LABEL[agent]}
          </h2>
          <div className="grid grid-cols-1 gap-3 min-[900px]:grid-cols-2">
            <div role="group" aria-label={t.checksView.optimize} className="flex min-w-0 flex-col">
              <h3 className="px-5 text-sm font-medium">{t.checksView.optimize}</h3>
              {checks ? (
                <AgentChecks
                  agent={agent}
                  checks={checks.filter((c) => c.agent === agent)}
                  log={log}
                  emptyText={noChecksText(agent, AGENT_LABEL[agent], isMissing(agent))}
                  isClear={isCheckedAgent(agent, isMissing(agent))}
                  onChanged={load}
                  handoff={handoff}
                  fixedSince={fixedSince}
                  openRow={openRow}
                />
              ) : (
                LOADING
              )}
            </div>
            <div role="group" aria-label={t.checksView.diagnose} className="flex min-w-0 flex-col">
              <h3 className="px-5 text-sm font-medium">
                {t.checksView.diagnose}
                {/* 제목은 짧게, 하는 일은 같은 줄 작은 글씨로(옆 열 제목과 높이를 맞춘다). */}
                <span className="ml-1.5 text-xs font-normal text-muted-foreground">
                  {t.checksView.diagnoseHint}
                </span>
              </h3>
              {switches ? (
                <CoachPanel
                  agent={agent}
                  isMissing={isMissing(agent)}
                  switches={switches}
                  log={log}
                  onChanged={load}
                  onShowEffect={onShowEffect}
                  openRow={openRow}
                />
              ) : (
                LOADING
              )}
            </div>
          </div>
        </section>
      ))}
      <HiddenRow hidden={hidden} isBusy={isBusy} onRestore={restore} />
    </div>
  );
}
