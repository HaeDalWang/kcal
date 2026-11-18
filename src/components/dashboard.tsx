// 대시보드 창(docs/decisions/0010, 0015, 0016). 머리 아래 탭 두 개(0017 "화면 자리").
// 사용량 탭 — 위: 고른 날(최근 7일)의 링·숫자 칸·시간대 막대. 가운데: 처방 효과(주간 요약·처방별 전후). 아래: 에이전트별 상세(지금 값).
// 점검 탭 — 에이전트별 최적화·진단하기(`checks-tab.tsx`).
import { useEffect, useRef, useState, useSyncExternalStore } from "react";

import { MetricLines, UsageBars } from "@/components/agent-metrics";
import { AgentIcon } from "@/components/agent-icon";
import { CodexBehaviorDetails } from "@/components/behavior-section";
import { ContextList } from "@/components/context-list";
import { ChecksTab } from "@/components/checks-tab";
import { CARD, DailyActivity, DayPicker } from "@/components/daily-activity";
import { HEALTH_TITLE_ID, HealthCompare } from "@/components/health-compare";
import { RateLine } from "@/components/rate-line";
import { reasonStateKind, StateText } from "@/components/state-text";
import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { AGENT_LABEL, AgentSection, useUsageSnapshot } from "@/components/usage-popover";
import { kiroMonthlyLimit } from "@/lib/breakdown";
import { recentDays } from "@/lib/daily";
import {
  errorMessage,
  initialDashboardTab,
  onDashboardTab,
  refreshAll,
  type AgentUsage,
  type DashboardTab,
  type Settings,
  type UsageSnapshot,
} from "@/lib/ipc";
import { t } from "@/lib/i18n";
import { metricsStore } from "@/lib/snapshot-store";
import { useSettings } from "@/lib/use-settings";

/** 요일 줄에 보이는 날 수(오늘 포함). */
const WEEK_DAYS = 7;

/** 선택된 탭의 그림자를 없애 라이트에서 가볍게 보이게 한다(다크는 그림자가 안 보이므로 그대로). */
const TAB = "px-3 group-data-[variant=default]/tabs-list:data-active:shadow-none";

type UsageState = { snapshot: UsageSnapshot | null; error: string | null; now: number };

/** 값이 하나도 없고 이유만 있다(로그인 필요·설치 안 됨). 사용자가 할 일이라 첫 화면 맨 위에 둔다. */
const isUnreadable = (usage: AgentUsage | undefined): usage is AgentUsage & { reason: string } =>
  !!usage?.reason && usage.windows.length === 0;

/** 읽지 못한 에이전트의 이유. 빈 링 아래 스크롤 밖에 묻히지 않게 사용량 탭 맨 위에 한 번만 보인다. */
function UnreadableNotices({ settings, usage: state }: { settings: Settings; usage: UsageState }) {
  const unreadable = settings.enabledAgents
    .map((agent) => state.snapshot?.agents.find((a) => a.agent === agent))
    .filter(isUnreadable);
  if (unreadable.length === 0) return null;
  return (
    <section className={`${CARD} flex flex-col gap-2 p-3`}>
      {unreadable.map((usage) => (
        <div key={usage.agent} className="flex items-start gap-1.5 px-1">
          <AgentIcon agent={usage.agent} className="size-5 shrink-0" />
          <StateText kind={reasonStateKind(usage)} className="pt-0.5">
            {`${AGENT_LABEL[usage.agent]}: ${usage.reason}`}
          </StateText>
        </div>
      ))}
    </section>
  );
}

/** 기록(요청·막대)이 하나도 없는 첫 실행. 빈 카드만 늘어서지 않게 무엇이 필요한지 한 줄과 할 일 하나를 맨 위에 둔다.
 * 지표는 `DailyActivity`가 이미 불러온 `metricsStore` 값을 읽기만 한다(다시 부르지 않는다).
 * 사용량을 아직 확인하지 못한 줄(`checkedAt` null)이 있으면 불러오는 중이라 보이지 않는다. */
function EmptyGuide({
  settings,
  usage: state,
  onRefresh,
}: {
  settings: Settings;
  usage: UsageState;
  onRefresh: () => void;
}) {
  const { value: metrics } = useSyncExternalStore(metricsStore.subscribe, metricsStore.getSnapshot);
  const { snapshot } = state;
  if (!snapshot || metrics === undefined) return null;
  const isChecking = settings.enabledAgents.some(
    (agent) => snapshot.agents.find((a) => a.agent === agent)?.checkedAt === null,
  );
  const hasRecords = settings.enabledAgents.some((agent) => {
    const m = metrics?.agents.find((a) => a.agent === agent);
    return !!m && (m.bars.length > 0 || m.hourly.some((h) => h.requests > 0));
  });
  // 모든 에이전트가 읽지 못한 상태면 `UnreadableNotices`가 이미 할 일을 말한다(같은 안내를 두 번 띄우지 않는다).
  const isAllUnreadable =
    settings.enabledAgents.length > 0 &&
    settings.enabledAgents.every((agent) =>
      isUnreadable(snapshot.agents.find((a) => a.agent === agent)),
    );
  if (isChecking || hasRecords || isAllUnreadable) return null;
  return (
    <section className={`${CARD} flex items-center justify-between gap-3 p-3`}>
      <p className="px-1 text-sm text-muted-foreground">{t.dashboard.emptyGuide}</p>
      <Button variant="outline" size="sm" onClick={onRefresh} className="shrink-0">
        {t.app.refresh}
      </Button>
    </section>
  );
}

function AgentPanels({ settings, usage: state }: { settings: Settings; usage: UsageState }) {
  const { snapshot, error, now } = state;
  const { lowPercent, bytesPerToken, breakdownDays } = settings.tuning;
  return settings.enabledAgents.map((agent) => {
    const usage = snapshot?.agents.find((a) => a.agent === agent);
    return (
      <section
        key={agent}
        aria-labelledby={`dashboard-${agent}`}
        className={`${CARD} flex flex-col gap-2 p-3`}
      >
        <h2 id={`dashboard-${agent}`} className="flex items-center gap-1.5 px-3 font-semibold">
          <AgentIcon agent={agent} className="size-5" />
          {AGENT_LABEL[agent]}
        </h2>
        <div className="grid grid-cols-1 gap-3 min-[900px]:grid-cols-2">
          <div className="flex min-w-0 flex-col gap-3">
            {error && (
              <p role="alert" className="px-3 text-sm text-destructive">
                {error}
              </p>
            )}
            {/* 트레이용 20px 안쪽 여백(`AgentSection`·`MetricLines`·`UsageBars`)을 아래 칸(`InfoSection` px-3)의 왼쪽 선에 맞춘다. */}
            <div className="flex flex-col gap-3 empty:hidden [&>:is(ul,div)]:px-3 [&>section>*]:px-3">
              {usage && !isUnreadable(usage) && (
                <AgentSection usage={usage} now={now} lowPercent={lowPercent} />
              )}
              {/* 시작 컨텍스트는 오른쪽 `ContextList` 하나로 보인다(같은 값·구성이 두 번 나오지 않게). */}
              <MetricLines agent={agent} />
              {/* 막대가 적으면(Kiro 하루 1개 등) 넓은 대시보드에서 한 덩어리로 보인다. 폭을 막고 최근 쪽(오른쪽)에 붙인다. 트레이는 그대로. */}
              <UsageBars agent={agent} className="h-24 justify-end [&>li]:max-w-6" />
            </div>
            <RateLine
              agent={agent}
              breakdownDays={breakdownDays}
              kiroLimit={kiroMonthlyLimit(usage)}
            />
            {agent === "codex" && <CodexBehaviorDetails />}
          </div>
          {/* 2열일 때 오른쪽 첫 칸은 위 구분선 없이 왼쪽 첫 줄과 같은 높이에서 시작한다. */}
          <div className="min-w-0 min-[900px]:[&>section:first-child]:border-t-0">
            <ContextList agent={agent} charsPerToken={bytesPerToken} />
          </div>
        </div>
      </section>
    );
  });
}

/** 지금 탭과 점검 탭 스크롤 요청. `focusKey`는 팝오버 "점검"으로 열릴 때마다 늘고, 사용자가 탭을 누르면 0이다. */
type View = { tab: DashboardTab; focusKey: number };

function initialView(): View {
  const tab = initialDashboardTab(location.search) ?? "usage";
  return { tab, focusKey: tab === "checks" ? 1 : 0 };
}

/** 지금 탭. 새 창은 `?tab=`으로 시작하고, 이미 열린 창은 `openDashboard(tab)`이 보낸 event로 바꾼다.
 * 점검으로 바뀌면 `focusKey`를 늘려 다시 읽고 알림 칸으로 스크롤하게 한다. */
function useDashboardView() {
  const [view, setView] = useState<View>(initialView);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let isCancelled = false;
    let unlisten: (() => void) | undefined;
    onDashboardTab((tab) =>
      setView((prev) => ({ tab, focusKey: tab === "checks" ? prev.focusKey + 1 : 0 })),
    ).then(
      (stop) => (isCancelled ? stop() : (unlisten = stop)),
      (reason: unknown) => !isCancelled && setError(errorMessage(reason)),
    );
    return () => {
      isCancelled = true;
      unlisten?.();
    };
  }, []);
  return { view, setView, error };
}

export function Dashboard() {
  const { settings, error } = useSettings();
  const usage = useUsageSnapshot();
  const [refreshError, setRefreshError] = useState<string | null>(null);
  const { view, setView, error: tabError } = useDashboardView();
  // "효과 보기"로 사용량 탭에 왔을 때 처방 효과로 스크롤한다.
  const isEffectPending = useRef(false);
  // 오늘에서 며칠 전인가(0 = 오늘). 자정이 지나도 "오늘"을 가리키도록 날짜 대신 거리로 둔다.
  const [daysAgo, setDaysAgo] = useState(0);
  // `now`는 usage 수신·창 포커스 때만 바뀐다(타이머 없음).
  const days = recentDays(usage.now, WEEK_DAYS);
  const selected = WEEK_DAYS - 1 - daysAgo;
  const select = (index: number) => setDaysAgo(WEEK_DAYS - 1 - index);
  const refresh = () => {
    setRefreshError(null);
    refreshAll().catch((reason: unknown) => setRefreshError(errorMessage(reason)));
  };
  const showEffect = () => {
    isEffectPending.current = true;
    setView({ tab: "usage", focusKey: 0 });
  };
  useEffect(() => {
    if (!isEffectPending.current || view.tab !== "usage") return;
    isEffectPending.current = false;
    document.getElementById(HEALTH_TITLE_ID)?.scrollIntoView({ block: "start" });
  }, [view.tab]);
  const shownError = error ?? refreshError ?? tabError;
  return (
    <div className="h-full overflow-y-auto bg-background">
      <main className="mx-auto max-w-5xl p-4 break-keep">
        <Tabs
          value={view.tab}
          onValueChange={(tab: DashboardTab) => setView({ tab, focusKey: 0 })}
          className="gap-3"
        >
          {/* 도구막대 한 줄: 제목 · 탭 | 날짜(사용량 탭만) · 새로고침. 좁으면 오른쪽 묶음이 통째로 다음 줄로 간다. */}
          <header className="flex flex-wrap items-center gap-x-3 gap-y-2">
            <h1 className="text-sm font-semibold">{t.dashboard.title}</h1>
            <TabsList>
              <TabsTrigger value="usage" className={TAB}>
                {t.dashboard.usage}
              </TabsTrigger>
              <TabsTrigger value="checks" className={TAB}>
                {t.app.checks}
              </TabsTrigger>
            </TabsList>
            <div className="ml-auto flex items-center gap-2">
              {view.tab === "usage" && (
                <DayPicker days={days} selected={selected} onSelect={select} />
              )}
              <Button variant="outline" onClick={refresh}>
                {t.app.refresh}
              </Button>
            </div>
          </header>
          {shownError && (
            <p role="alert" className="text-sm text-destructive">
              {shownError}
            </p>
          )}
          {!error && !settings && <StateText kind="loading" />}
          {settings && (
            <>
              {/* 사용량은 숨겨도 그대로 둔다(다시 와도 다시 읽지 않는다). 점검은 열 때마다 새로 읽는다. */}
              <TabsContent value="usage" keepMounted className="flex flex-col gap-3">
                <UnreadableNotices settings={settings} usage={usage} />
                <EmptyGuide settings={settings} usage={usage} onRefresh={refresh} />
                <DailyActivity
                  days={days}
                  selected={selected}
                  onSelect={select}
                  settings={settings}
                  kiroUsage={usage.snapshot?.agents.find((a) => a.agent === "kiro")}
                />
                <HealthCompare
                  refreshKey={`${settings.tuning.compareDays}:${settings.enabledAgents.join(",")}`}
                />
                <AgentPanels settings={settings} usage={usage} />
              </TabsContent>
              <TabsContent value="checks">
                <ChecksTab
                  agents={settings.enabledAgents}
                  focusKey={view.focusKey}
                  onShowEffect={showEffect}
                />
              </TabsContent>
            </>
          )}
        </Tabs>
      </main>
    </div>
  );
}
