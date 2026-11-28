import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import type { Settings, Tuning, TuningFieldSpec } from "@/lib/ipc";

/** Rust `Tuning::default()`와 같은 값(테스트 입력용). */
export const DEFAULT_TUNING: Tuning = {
  apiIntervalSecs: 300,
  kiroIntervalSecs: 600,
  refreshMinGapSecs: 30,
  maxBackoffSecs: 3600,
  ratesTtlSecs: 1800,
  contextTtlSecs: 600,
  metricsTtlSecs: 300,
  httpTimeoutSecs: 15,
  keychainTimeoutSecs: 10,
  kiroTimeoutSecs: 30,
  codexSessionsToSearch: 50,
  kiroSessionsToSearch: 50,
  shapeSessionsToSearch: 5,
  codexRolloutsChecked: 3,
  claudeLogTailBytes: 65536,
  bytesPerToken: 4,
  breakdownDays: 14,
  lowPercent: 20,
  recentBars: 8,
  barDays: 7,
  loginWarnDays: 3,
  claudeLogFiles: 50,
  topSessions: 3,
  marathonRequests: 150,
  largeContextTokens: 130000,
  dailyGoalPercent: 14,
  kiroDailyCredits: 0,
  compareDays: 7,
  checkModelDays: 30,
  checkAlertMin: 1,
  claudeHandoffTimeoutSecs: 300,
  claudeHandoffMinRemaining: 20,
  diagnoseRecentFolders: 5,
  diagnoseUsageDays: 30,
  diagnoseUsageFiles: 1000,
  diagnoseUsageMaxMib: 2048,
  diagnoseMinSessions: 10,
  scopeMinUses: 3,
  sessionIdleHours: 24,
  sessionChildCpuPercent: 2,
  sessionProtectedFolders: [],
  advanced: {
    goodR2: 0.7,
    rangeR2: 0.3,
    resetTolSecs: 600,
    holdoutTrain: 0.7,
    minSamplesPerVar: 3,
    claudeRateDays: 7,
    cacheMissRatio: 0.5,
  },
};

/** Rust `Tuning::spec()`(IPC fixture에서 읽는다. 범위를 TS에 따로 두지 않는다). */
export const TUNING_SPEC: TuningFieldSpec[] = (
  JSON.parse(readFileSync(resolve("src/test/fixtures/ipc/settings.json"), "utf8")) as Settings
).tuningSpec;
