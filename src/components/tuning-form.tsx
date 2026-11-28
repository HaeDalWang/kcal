import { ChevronRight } from "lucide-react";
import { useState, type FormEvent, type ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  errorMessage,
  isAppError,
  POPOVER_HEIGHT_RANGE,
  type AdvancedTuning,
  type Settings,
  type Tuning,
  type TuningFieldSpec,
  type TuningKey,
} from "@/lib/ipc";
import { t } from "@/lib/i18n";

/** 화면 표시 단위. 저장 값 = 화면 값 × `factor`. 범위·기본값은 Rust spec(저장 단위)에서 바꿔 보인다. 이름은 `t.tuning.units`. */
const UNITS = {
  minutes: 60,
  hours: 1,
  seconds: 1,
  kib: 1024,
  mib: 1,
  count: 1,
  days: 1,
  percent: 1,
  bytes: 1,
  ratio: 1,
  times: 1,
  tokens: 1,
  points: 1,
  credits: 1,
} as const;
type Unit = keyof typeof UNITS;

type Section =
  "limits" | "cache" | "timeouts" | "scan" | "estimate" | "pattern" | "display" | "advanced";
const SECTIONS: Exclude<Section, "advanced">[] = [
  "limits",
  "cache",
  "timeouts",
  "scan",
  "estimate",
  "pattern",
  "display",
];

type FieldInfo = { unit: Unit; section: Section };

/** 필드 단위·섹션(화면 쪽 정보만). 이름·도움말은 `t.tuning.fields`, 범위·기본값은 `Settings.tuningSpec`. */
const FIELDS: Record<TuningKey, FieldInfo> = {
  sessionIdleHours: { unit: "hours", section: "limits" },
  sessionChildCpuPercent: { unit: "percent", section: "limits" },
  apiIntervalSecs: { unit: "minutes", section: "limits" },
  kiroIntervalSecs: { unit: "minutes", section: "limits" },
  refreshMinGapSecs: { unit: "seconds", section: "limits" },
  maxBackoffSecs: { unit: "minutes", section: "limits" },
  ratesTtlSecs: { unit: "minutes", section: "cache" },
  contextTtlSecs: { unit: "minutes", section: "cache" },
  metricsTtlSecs: { unit: "minutes", section: "cache" },
  httpTimeoutSecs: { unit: "seconds", section: "timeouts" },
  keychainTimeoutSecs: { unit: "seconds", section: "timeouts" },
  kiroTimeoutSecs: { unit: "seconds", section: "timeouts" },
  claudeHandoffTimeoutSecs: { unit: "minutes", section: "timeouts" },
  claudeHandoffMinRemaining: { unit: "percent", section: "limits" },
  codexSessionsToSearch: { unit: "count", section: "scan" },
  kiroSessionsToSearch: { unit: "count", section: "scan" },
  shapeSessionsToSearch: { unit: "count", section: "scan" },
  codexRolloutsChecked: { unit: "count", section: "scan" },
  claudeLogTailBytes: { unit: "kib", section: "scan" },
  breakdownDays: { unit: "days", section: "scan" },
  loginWarnDays: { unit: "days", section: "limits" },
  barDays: { unit: "days", section: "scan" },
  claudeLogFiles: { unit: "count", section: "scan" },
  compareDays: { unit: "days", section: "scan" },
  checkModelDays: { unit: "days", section: "scan" },
  diagnoseRecentFolders: { unit: "count", section: "scan" },
  diagnoseUsageDays: { unit: "days", section: "scan" },
  diagnoseUsageFiles: { unit: "count", section: "scan" },
  diagnoseUsageMaxMib: { unit: "mib", section: "scan" },
  diagnoseMinSessions: { unit: "count", section: "scan" },
  scopeMinUses: { unit: "count", section: "scan" },
  bytesPerToken: { unit: "bytes", section: "estimate" },
  lowPercent: { unit: "percent", section: "display" },
  recentBars: { unit: "count", section: "display" },
  topSessions: { unit: "count", section: "display" },
  checkAlertMin: { unit: "count", section: "display" },
  dailyGoalPercent: { unit: "points", section: "display" },
  kiroDailyCredits: { unit: "credits", section: "display" },
  marathonRequests: { unit: "times", section: "pattern" },
  largeContextTokens: { unit: "tokens", section: "pattern" },
  "advanced.goodR2": { unit: "ratio", section: "advanced" },
  "advanced.rangeR2": { unit: "ratio", section: "advanced" },
  "advanced.resetTolSecs": { unit: "seconds", section: "advanced" },
  "advanced.holdoutTrain": { unit: "ratio", section: "advanced" },
  "advanced.minSamplesPerVar": { unit: "count", section: "advanced" },
  "advanced.claudeRateDays": { unit: "days", section: "advanced" },
  "advanced.cacheMissRatio": { unit: "ratio", section: "advanced" },
};

/** 설정 묶음(macOS 설정 모양): 작은 캡션 + 둥근 면 + 가는 구분선으로 나뉜 줄. 팝오버 폭은 그대로. */
export const GROUP_CLASS =
  "flex flex-col divide-y divide-border rounded-lg bg-muted/50 [&>*]:px-2.5";
export const ROW_CLASS = "flex min-h-8 items-center gap-2 py-1";
export const CAPTION_CLASS = "mb-1 px-1.5 text-xs font-medium text-muted-foreground";
/** 접는 줄(`<summary>`): 묶음과 같은 면. 화살표는 열리면 돌아간다(motion-reduce면 바로). */
const DISCLOSURE_CLASS =
  "flex min-h-8 cursor-default list-none items-center gap-1.5 rounded-lg bg-muted/50 px-2.5 text-sm hover:bg-accent focus-visible:ring-3 focus-visible:ring-ring focus-visible:outline-none active:translate-y-px [&::-webkit-details-marker]:hidden";
const CHEVRON_CLASS =
  "size-4 shrink-0 text-muted-foreground transition-transform group-open/disclosure:rotate-90 motion-reduce:transition-none";

const unitLabel = (unit: Unit) => t.tuning.units[unit];
const KEYS = Object.keys(FIELDS) as TuningKey[];
const ADVANCED_PREFIX = "advanced.";

/** 머리(뒤로)·저장 줄·종료 줄 높이를 뺀 나머지만 스크롤 영역으로 쓴다(팝오버 상한 800 안). */
const CHROME_PX = 160;
const SCROLL_MAX_PX = POPOVER_HEIGHT_RANGE.max - CHROME_PX;

type Edits = Partial<Record<TuningKey, string>>;

export function readTuning(tuning: Tuning, key: TuningKey): number {
  if (key.startsWith(ADVANCED_PREFIX)) {
    return tuning.advanced[key.slice(ADVANCED_PREFIX.length) as keyof AdvancedTuning];
  }
  return tuning[key as Exclude<TuningKey, `advanced.${string}`>];
}

/** `key` 하나만 바꾼 새 tuning. */
export function withTuning(tuning: Tuning, key: TuningKey, value: number): Tuning {
  if (key.startsWith(ADVANCED_PREFIX)) {
    const name = key.slice(ADVANCED_PREFIX.length);
    return { ...tuning, advanced: { ...tuning.advanced, [name]: value } };
  }
  return { ...tuning, [key]: value };
}

/** 저장 단위 값 → 입력란 문자열(표시 단위). */
export function toDisplay(key: TuningKey, stored: number): string {
  return String(stored / UNITS[FIELDS[key].unit]);
}

/** 입력란 문자열 → 저장 단위 값. 정수 필드는 표시 단위로도 정수만 받는다(분 × 60 = 초). */
export function parseField(
  text: string,
  spec: TuningFieldSpec,
): { value: number } | { error: string } {
  const trimmed = text.trim();
  const shown = Number(trimmed);
  if (trimmed === "" || !Number.isFinite(shown)) return { error: t.tuning.number };
  if (spec.isInteger && !Number.isInteger(shown)) return { error: t.tuning.integer };
  const value = shown * UNITS[FIELDS[spec.key].unit];
  if (value < spec.min || value > spec.max) return { error: t.tuning.range(rangeText(spec)) };
  return { value };
}

/** "1–1440분". */
function rangeText(spec: TuningFieldSpec): string {
  const unit = unitLabel(FIELDS[spec.key].unit);
  const gap = unit ? t.tuning.unitGap : "";
  return `${toDisplay(spec.key, spec.min)}–${toDisplay(spec.key, spec.max)}${gap}${unit}`;
}

/** Rust `invalid_input` 메시지("…: advanced.goodR2, advanced.rangeR2")에 나온 필드 이름. */
export function fieldsInMessage(message: string): TuningKey[] {
  return message.split(/[\s,:]+/).filter((word): word is TuningKey => Object.hasOwn(FIELDS, word));
}

type TuningFormProps = {
  settings: Settings;
  isSaving: boolean;
  /** 저장. 실패하면 Rust 오류로 reject한다. */
  onSave: (tuning: Tuning) => Promise<void>;
  /** 스크롤 영역 맨 위(에이전트 토글 등). */
  children: ReactNode;
  /** 스크롤 영역 맨 아래(버전 줄). */
  footer?: ReactNode;
};

/** tuning 전 필드 편집. 바꾼 필드만 문자열로 들고 있다가 "저장" 때 검사·변환한다. */
export function TuningForm({ settings, isSaving, onSave, children, footer }: TuningFormProps) {
  const [edits, setEdits] = useState<Edits>({});
  const [errors, setErrors] = useState<Edits>({});
  const [protectedFolders, setProtectedFolders] = useState<string | null>(null);
  const [formError, setFormError] = useState<string | null>(null);
  const [isTuningOpen, setIsTuningOpen] = useState(false);
  const [isAdvancedOpen, setIsAdvancedOpen] = useState(false);
  const specs = new Map(settings.tuningSpec.map((spec) => [spec.key, spec]));
  const current = (key: TuningKey) => toDisplay(key, readTuning(settings.tuning, key));
  const isDirty = Object.keys(edits).length > 0 || protectedFolders !== null;

  const showErrors = (next: Edits) => {
    setErrors(next);
    if (Object.keys(next).length > 0) setIsTuningOpen(true);
    if (Object.keys(next).some((key) => key.startsWith(ADVANCED_PREFIX))) setIsAdvancedOpen(true);
  };

  const change = (key: TuningKey, text: string) => {
    setEdits((prev) => {
      const next = { ...prev, [key]: text };
      if (text === current(key)) delete next[key];
      return next;
    });
    setErrors((prev) => ({ ...prev, [key]: undefined }));
  };

  const resetToDefaults = () => {
    const next: Edits = {};
    for (const spec of settings.tuningSpec) {
      const text = toDisplay(spec.key, spec.default);
      if (text !== current(spec.key)) next[spec.key] = text;
    }
    setProtectedFolders("");
    setEdits(next);
    setErrors({});
    setFormError(null);
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    let tuning =
      protectedFolders === null
        ? settings.tuning
        : {
            ...settings.tuning,
            sessionProtectedFolders: protectedFolders
              .split("\n")
              .map((p) => p.trim())
              .filter(Boolean),
          };
    const nextErrors: Edits = {};
    for (const [key, text] of Object.entries(edits) as [TuningKey, string][]) {
      const spec = specs.get(key);
      if (!spec) continue;
      const parsed = parseField(text, spec);
      if ("error" in parsed) nextErrors[key] = parsed.error;
      else tuning = withTuning(tuning, key, parsed.value);
    }
    if (Object.keys(nextErrors).length > 0) {
      showErrors(nextErrors);
      setFormError(t.tuning.formInvalid);
      return;
    }
    onSave(tuning).then(
      () => {
        setProtectedFolders(null);
        setEdits({});
        setErrors({});
        setFormError(null);
      },
      (reason: unknown) => {
        const fields =
          isAppError(reason) && reason.kind === "invalid_input"
            ? fieldsInMessage(reason.message)
            : [];
        showErrors(Object.fromEntries(fields.map((key) => [key, t.tuning.rejected])));
        setFormError(errorMessage(reason));
      },
    );
  };

  const renderField = (key: TuningKey) => {
    const spec = specs.get(key);
    if (!spec || (key === "keychainTimeoutSecs" && !navigator.userAgent.includes("Mac"))) {
      return null;
    }
    const info = { ...FIELDS[key], ...t.tuning.fields[key] };
    const id = `tuning-${key}`;
    const error = errors[key];
    const shown = edits[key] ?? current(key);
    const defaultText = toDisplay(key, spec.default);
    // 기본값과 다를 때만 기본값을 보인다. 범위는 입력란 title과 오류 문구에만 둔다.
    const sub = [info.hint, shown !== defaultText ? t.tuning.defaultValue(defaultText) : null]
      .filter(Boolean)
      .join(" · ");
    const unit = unitLabel(info.unit);
    return (
      <div key={key} className="flex flex-col py-1">
        <div className="flex min-h-6 items-center gap-2">
          <label htmlFor={id} className="min-w-0 flex-1 text-sm">
            {info.label}
          </label>
          <Input
            id={id}
            inputMode={spec.isInteger ? "numeric" : "decimal"}
            value={shown}
            onChange={(event) => change(key, event.target.value)}
            disabled={isSaving}
            title={rangeText(spec)}
            aria-invalid={error ? true : undefined}
            aria-describedby={
              [sub && `${id}-sub`, error && `${id}-error`].filter(Boolean).join(" ") || undefined
            }
            className="h-6 w-16 bg-background px-1.5 text-right tabular-nums select-text dark:bg-background"
          />
          <span className="w-9 text-xs text-muted-foreground">{unit}</span>
        </div>
        {sub && (
          <p id={`${id}-sub`} className="text-xs text-pretty text-muted-foreground tabular-nums">
            {sub}
          </p>
        )}
        {error && (
          <p id={`${id}-error`} className="text-xs text-destructive">
            {error}
          </p>
        )}
      </div>
    );
  };

  const keysOf = (section: Section) => KEYS.filter((key) => FIELDS[key].section === section);

  return (
    <form onSubmit={submit} noValidate className="flex flex-col">
      <div
        className="flex flex-col gap-3 overflow-y-auto px-3 py-2"
        style={{ maxHeight: SCROLL_MAX_PX }}
      >
        {children}
        {/* 조절 값은 기본으로 접는다. 저장 줄도 펼쳤을 때만 보인다(에이전트·언어는 바로 저장). */}
        <details
          open={isTuningOpen}
          onToggle={(event) => setIsTuningOpen(event.currentTarget.open)}
          className="group/disclosure flex flex-col gap-3"
        >
          <summary className={DISCLOSURE_CLASS}>
            <span className="flex-1">{t.tuning.title}</span>
            <ChevronRight className={CHEVRON_CLASS} aria-hidden />
          </summary>
          {/* WebKit은 <details> 안을 flex로 놓지 않아 gap이 안 먹는다. 묶음 사이 간격은 이 div가 준다. */}
          <div className="flex flex-col gap-4 pt-3">
            <label className="flex flex-col gap-1 text-sm">
              {t.sessions.protectedFolders}
              <textarea
                value={protectedFolders ?? settings.tuning.sessionProtectedFolders.join("\n")}
                onChange={(event) => setProtectedFolders(event.target.value)}
                disabled={isSaving}
                rows={3}
                className="w-full resize-y rounded-md border bg-background p-2 text-xs select-text focus-visible:ring-3 focus-visible:ring-ring focus-visible:outline-none disabled:opacity-50"
                placeholder={t.sessions.protectedHint}
              />
            </label>
            {SECTIONS.map((section) => (
              <fieldset key={section} className="flex flex-col">
                <legend className={CAPTION_CLASS}>{t.tuning.sections[section]}</legend>
                <div className={GROUP_CLASS}>{keysOf(section).map(renderField)}</div>
              </fieldset>
            ))}
            <details
              open={isAdvancedOpen}
              onToggle={(event) => setIsAdvancedOpen(event.currentTarget.open)}
              className="group/disclosure flex flex-col gap-1.5"
            >
              <summary className={DISCLOSURE_CLASS}>
                <span className="flex-1">{t.tuning.advanced}</span>
                <ChevronRight className={CHEVRON_CLASS} aria-hidden />
              </summary>
              <div className="flex flex-col gap-1.5 pt-1.5">
                <p className="px-1.5 text-xs text-muted-foreground">{t.tuning.advancedNote}</p>
                <div className={GROUP_CLASS}>{keysOf("advanced").map(renderField)}</div>
              </div>
            </details>
          </div>
        </details>
        {footer}
      </div>
      {isTuningOpen && (
        <div className="flex flex-col gap-1.5 border-t px-3 py-2">
          {formError && (
            <p role="alert" className="text-xs text-destructive">
              {formError}
            </p>
          )}
          <div className="flex justify-end gap-2">
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={resetToDefaults}
              disabled={isSaving}
            >
              {t.tuning.reset}
            </Button>
            <Button type="submit" size="sm" disabled={!isDirty || isSaving}>
              {isSaving ? t.tuning.saving : t.tuning.save}
            </Button>
          </div>
        </div>
      )}
    </form>
  );
}
