// 최적화 칸(docs/decisions/0017 "첫 점검", 자리는 대시보드 점검 탭 — "화면 자리"): 잃는 것 없이 고칠 수 있는 설정 카드 → 바뀔 것 미리보기 → 확인 → 적용(백업) → 되돌리기.
// 값은 열 때와 적용·되돌리기·숨기기 뒤에만 읽는다(타이머 없음). 쓰기는 Rust가 점검 ID·옵션 ID로만 한다.
// "그대로 두기"는 카드를 숨긴다(설정은 그대로). 숨김 판정(`isDismissed`)은 Rust가 한다.
// 목록 모양(시안 J: 진행 표시 + 한 줄 목록 + 한 줄만 펼침)은 `checks-list.tsx`, 문구는 `src/lib/check-copy.ts`.
import { useCallback, useRef, useState } from "react";

import {
  AllClear,
  CheckRow,
  FixedRow,
  ProgressSummary,
  type HandoffContext,
} from "@/components/checks-list";
import { AGENT_COLOR } from "@/components/agent-metrics";
import { ChangePanel, DonePanel, LiveStatus, usePanelFocus } from "@/components/coach-view";
import { RulesPanel } from "@/components/rules-panel";
import { Button } from "@/components/ui/button";
import {
  fixedEntries,
  handoffErrorText,
  handoffRunningText,
  hiddenText,
  linesText,
  type CheckAction,
} from "@/lib/check-copy";
import { t } from "@/lib/i18n";
import { checkRowKey, useOpenRow, type OpenRow } from "@/lib/open-row";
import {
  applyCheck,
  applyClaudeHandoff,
  applyClaudeRule,
  askClaudeHandoff,
  askClaudeRules,
  dismissCheck,
  errorMessage,
  isHandoffError,
  previewCheck,
  undoSwitch,
  type Agent,
  type AppliedSwitch,
  type CheckPreview,
  type CoachCheck,
  type DiffLine,
  type HandoffPreview,
  type RuleProposal,
  type RulesPreview,
} from "@/lib/ipc";

type Step =
  | { kind: "preview"; action: CheckAction; preview: CheckPreview | null }
  /** Claude에게 맡김. `preview`가 null이면 아직 도는 중. */
  | { kind: "handoff"; preview: HandoffPreview | null }
  /** 규칙 카드(0018 작업 B)를 Claude에게 맡김. `applied`는 이 칸에서 바꾼 규칙(열쇠 → 기록). */
  | {
      kind: "rules";
      limit: number;
      preview: RulesPreview | null;
      applied: Record<string, AppliedSwitch>;
    }
  | { kind: "done"; applied: AppliedSwitch };

export type { HandoffContext };

/** Claude가 돌고 있다(다른 카드 버튼을 끈다). */
const isRunning = (step: Step | undefined) =>
  (step?.kind === "handoff" || step?.kind === "rules") && step.preview === null;

const NO_USAGE: HandoffContext = { remaining: null, minRemaining: 20, timeoutSecs: 300 };
const MUTED = "text-[10px] text-muted-foreground";

/** 점검 탭 맨 아래 "숨긴 카드 n개 · 다시 보기". 숨긴 카드가 없으면 그리지 않는다. */
export function HiddenRow({
  count,
  isBusy,
  onRestore,
}: {
  count: number;
  isBusy: boolean;
  onRestore: () => void;
}) {
  const text = hiddenText(count);
  if (text === null) return null;
  return (
    <div className="flex items-center gap-2 px-1 py-1">
      <span className="flex-1 text-xs text-muted-foreground">{text}</span>
      <Button size="xs" variant="outline" disabled={isBusy} onClick={onRestore}>
        {t.checks.restore}
      </Button>
    </div>
  );
}

/** 지금 미리 보거나 막 적용한 카드. `index`는 고를 때 목록 자리(적용 뒤 카드가 목록에서 빠져도 그 자리에 둔다). */
type Active = { check: CoachCheck; index: number; step: Step };

/** 대시보드 점검 탭의 "최적화" 칸(에이전트 하나). 적용 흐름은 그 카드 자리에서 하고 다른 카드는 그대로 보인다.
 * 목록은 점검 탭이 읽어 넘기고, 쓰기 뒤 `onChanged`로 다시 읽게 한다. */
export function AgentChecks({
  agent = "claude",
  checks,
  log = [],
  emptyText,
  isClear = false,
  onChanged,
  handoff = NO_USAGE,
  fixedSince = null,
  openRow,
}: {
  agent?: Agent;
  /** 이 에이전트의 카드(숨긴 것 포함, 숨긴 것은 그리지 않는다). */
  checks: CoachCheck[];
  /** 적용 기록(`get_coach_log`). 이 칸은 이 에이전트의 점검 기록 중 되돌리지 않은 것을 "고친 줄"로 보인다. */
  log?: AppliedSwitch[];
  emptyText: string;
  /** 점검이 돌았고 고칠 것이 없다: 빈 문장 대신 체크 표시와 함께 보인다. */
  isClear?: boolean;
  onChanged: () => void;
  /** 긴 안내문 카드의 한도 문. 없으면 한도를 모르는 것으로 본다(버튼 꺼짐). */
  handoff?: HandoffContext;
  /** 이 시각(유닉스 ms) 뒤의 적용 기록만 고친 줄·진행 숫자에 센다. null이면 기간 제한 없음. */
  fixedSince?: number | null;
  /** 점검 탭 전체의 펼친 줄(키 "check:<id>"). 없으면 이 칸 안에서만 하나. */
  openRow?: OpenRow;
}) {
  const [active, setActive] = useState<Active | null>(null);
  // 펼친 줄은 하나만(강조 버튼 하나, design.md 6절 3). 기본은 모두 접힘.
  const open = useOpenRow(openRow);
  // 고친 줄에서 되돌린 결과(맨 위 완료 패널).
  const [undone, setUndone] = useState<AppliedSwitch | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isBusy, setIsBusy] = useState(false);
  // 패널을 닫으면 그 줄로 포커스를 돌려준다(줄이 없어졌으면 이 칸 맨 위로).
  const [returnId, setReturnId] = useState<string | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const onReturned = useCallback(() => setReturnId(null), []);
  const visible = checks.filter((c) => !c.isDismissed);
  const fixed = fixedEntries(log, agent, fixedSince);
  const setStep = (step: Step) => setActive((prev) => prev && { ...prev, step });

  /** 한도를 쓰는 실행. 실패하면 카드로 돌아가고 이유를 보인다(파일은 그대로). */
  const askClaude = (check: CoachCheck, index: number) => {
    setActive({ check, index, step: { kind: "handoff", preview: null } });
    askClaudeHandoff(check.id).then(
      (preview) => setStep({ kind: "handoff", preview }),
      (reason: unknown) => {
        setActive(null);
        setError(isHandoffError(reason) ? handoffErrorText(reason) : errorMessage(reason));
      },
    );
  };

  /** 규칙 카드: 규칙마다 제안을 받는다(한도를 쓴다). 실패하면 카드로 돌아간다. */
  const askRules = (check: CoachCheck, index: number) => {
    const limit = check.facts.type === "rulesPaths" ? check.facts.limit : 0;
    const running = { kind: "rules", limit, applied: {} } as const;
    setActive({ check, index, step: { ...running, preview: null } });
    askClaudeRules(check.id).then(
      (preview) => setStep({ ...running, preview }),
      (reason: unknown) => {
        setActive(null);
        setError(isHandoffError(reason) ? handoffErrorText(reason) : errorMessage(reason));
      },
    );
  };

  const pick = (check: CoachCheck, action: CheckAction) => {
    setError(null);
    const index = visible.findIndex((c) => c.id === check.id);
    if (check.facts.type === "rulesPaths") {
      askRules(check, index);
      return;
    }
    if (check.facts.type === "longInstructions") {
      askClaude(check, index);
      return;
    }
    setActive({ check, index, step: { kind: "preview", action, preview: null } });
    previewCheck(check.id, action.option).then(
      (preview) => setStep({ kind: "preview", action, preview }),
      (reason: unknown) => {
        setActive(null);
        setError(errorMessage(reason));
      },
    );
  };

  /** 쓰고 나서 목록을 다시 읽게 한다. 결과가 있으면(적용·되돌리기) 그 자리에 완료 패널을 둔다. */
  const write = <T,>(action: () => Promise<T>, onDone: (result: T) => void) => {
    setError(null);
    setIsBusy(true);
    action().then(
      (result) => {
        setIsBusy(false);
        onDone(result);
        onChanged();
      },
      (reason: unknown) => {
        setIsBusy(false);
        setError(errorMessage(reason));
      },
    );
  };
  const showDone = (applied: AppliedSwitch) => setStep({ kind: "done", applied });
  const keep = (check: CoachCheck) =>
    write(
      () => dismissCheck(check.id),
      () => {},
    );
  const confirm = () => {
    if (!active) return;
    const { check, step } = active;
    if (step.kind === "handoff" && step.preview) {
      const { id, beforeHash } = step.preview;
      write(() => applyClaudeHandoff(id, beforeHash), showDone);
      return;
    }
    if (step.kind !== "preview" || !step.preview) return;
    const { beforeHash } = step.preview;
    write(() => applyCheck(check.id, step.action.option, beforeHash), showDone);
  };
  /** 규칙 하나를 바꾸고 그 줄에 완료 표시를 둔다(칸은 그대로). */
  const applyRule = (rule: RuleProposal) => {
    if (!active || active.step.kind !== "rules") return;
    const id = active.check.id;
    write(
      () => applyClaudeRule(id, rule.key, rule.beforeHash),
      (applied) =>
        setActive(
          (prev) =>
            prev &&
            (prev.step.kind === "rules"
              ? {
                  ...prev,
                  step: { ...prev.step, applied: { ...prev.step.applied, [rule.key]: applied } },
                }
              : prev),
        ),
    );
  };
  const undo = (id: string) => write(() => undoSwitch(id), showDone);
  const undoFixed = (id: string) => write(() => undoSwitch(id), setUndone);
  const close = () => {
    if (active && visible.some((c) => c.id === active.check.id)) setReturnId(active.check.id);
    else rootRef.current?.focus();
    setActive(null);
  };

  const isActiveListed = active !== null && visible.some((c) => c.id === active.check.id);
  const at = active ? Math.min(Math.max(active.index, 0), visible.length) : 0;
  const rows =
    active && !isActiveListed
      ? [...visible.slice(0, at), active.check, ...visible.slice(at)]
      : visible;

  const isListEmpty = rows.length === 0 && fixed.length === 0;
  const busy = isBusy || isRunning(active?.step);
  const doneApplied = active?.step.kind === "done" ? active.step.applied : undone;
  return (
    <div ref={rootRef} tabIndex={-1} className="flex flex-col outline-none">
      <LiveStatus
        text={doneApplied && t.checksView.done(doneApplied.name, doneApplied.action === "undo")}
      />
      {error && (
        <p role="alert" className="px-5 pt-1.5 text-[10px] text-destructive">
          {error}
        </p>
      )}
      {isListEmpty && isClear ? (
        <AllClear agent={agent} text={emptyText} />
      ) : isListEmpty ? (
        <p className="px-5 py-2.5 text-xs text-muted-foreground wrap-break-word select-text">
          {emptyText}
        </p>
      ) : (
        <ProgressSummary agent={agent} fixed={fixed.length} left={visible} />
      )}
      {undone && (
        <DonePanel
          applied={undone}
          title={t.checksView.done(undone.name, true)}
          isBusy={isBusy}
          onUndo={undoFixed}
          onClose={() => setUndone(null)}
        />
      )}
      {!isListEmpty && (
        <div className="flex flex-col gap-1.5 border-t py-2.5">
          {fixed.map((entry) => (
            <FixedRow key={entry.id} entry={entry} isBusy={busy} onUndo={undoFixed} />
          ))}
          {rows.map((check) =>
            active?.check.id === check.id ? (
              // 미리보기·완료도 펼친 줄과 같은 테두리 카드 안에 둔다(줄이 그 자리에서 다음 단계로 넘어간 것으로 읽힌다).
              <div
                key={check.id}
                className="mx-3 rounded-lg border [&>section]:border-t-0 [&>section]:px-2 [&>section]:py-1.5"
                style={{ borderColor: AGENT_COLOR[check.agent] }}
              >
                <ActivePanel
                  step={active.step}
                  timeoutSecs={handoff.timeoutSecs}
                  isBusy={isBusy}
                  onConfirm={confirm}
                  onApplyRule={applyRule}
                  onUndo={undo}
                  onClose={close}
                />
              </div>
            ) : (
              <CheckRow
                key={check.id}
                check={check}
                handoff={handoff}
                isOpen={open.key === checkRowKey(check.id)}
                isBusy={busy}
                isEmphasized={active === null}
                isReturn={returnId === check.id}
                onReturned={onReturned}
                onToggle={() => open.toggle(checkRowKey(check.id))}
                onPick={pick}
                onKeep={keep}
              />
            ),
          )}
        </div>
      )}
    </div>
  );
}

const DIFF_MARK: Record<DiffLine["kind"], string> = { same: "  ", removed: "- ", added: "+ " };
const DIFF_CLASS: Record<DiffLine["kind"], string> = {
  same: "text-muted-foreground",
  removed: "text-destructive",
  added: "text-foreground",
};

function HandoffResult({ preview }: { preview: HandoffPreview }) {
  const copy = t.checks.handoffPreview;
  return (
    <>
      <p className="text-xs tabular-nums">{linesText(preview.beforeLines, preview.afterLines)}</p>
      <p className="truncate text-[10px] text-muted-foreground" title={preview.file}>
        {preview.file}
      </p>
      <pre
        aria-label={t.checksView.diff}
        className="max-h-64 overflow-auto rounded-md bg-muted px-2 py-1 font-mono text-[10px] leading-relaxed whitespace-pre"
      >
        {preview.diff.map((line, i) => (
          <span key={i} className={`block ${DIFF_CLASS[line.kind]}`}>
            {`${DIFF_MARK[line.kind]}${line.text}`}
          </span>
        ))}
      </pre>
      {preview.removed.length > 0 && (
        <div className="text-xs">
          <p className={MUTED}>{copy.removedTitle}</p>
          <ul className="list-disc pl-4">
            {preview.removed.map((item) => (
              <li key={item}>{item}</li>
            ))}
          </ul>
        </div>
      )}
      {preview.missingImports.length > 0 && (
        <div role="alert" className="text-xs text-destructive">
          <p className="font-medium">{copy.missingTitle}</p>
          <p className="font-mono break-all">
            {preview.missingImports.map((i) => `@${i}`).join(", ")}
          </p>
          <p>{copy.missingNote}</p>
        </div>
      )}
      <p className={MUTED}>{preview.canApply ? copy.note : copy.cannotApply}</p>
    </>
  );
}

/** Claude가 고쳐 쓴 글 미리보기: 줄 단위 차이 + 합치거나 뺀 것 + 빠진 가져오기 경고. 적용은 Rust 보관본으로 한다. */
function HandoffPanel({
  preview,
  timeoutSecs,
  isBusy,
  onConfirm,
  onCancel,
}: {
  preview: HandoffPreview | null;
  timeoutSecs: number;
  isBusy: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const copy = t.checks.handoffPreview;
  const focus = usePanelFocus(onCancel, isBusy);
  return (
    <section
      {...focus}
      role="group"
      aria-label={copy.title}
      className="flex flex-col gap-1.5 border-t px-5 py-2.5 outline-none"
    >
      <h2 className="text-[13px] font-medium">{copy.title}</h2>
      {preview ? (
        <HandoffResult preview={preview} />
      ) : (
        <p role="status" className="text-xs text-muted-foreground">
          {handoffRunningText(timeoutSecs)}
        </p>
      )}
      {preview && (
        <div className="flex justify-end gap-1.5 pt-0.5">
          <Button size="xs" variant="ghost" onClick={onCancel} disabled={isBusy}>
            {t.common.cancel}
          </Button>
          <Button size="xs" onClick={onConfirm} disabled={isBusy || !preview.canApply}>
            {copy.confirm}
          </Button>
        </div>
      )}
    </section>
  );
}

function ActivePanel({
  step,
  timeoutSecs,
  isBusy,
  onConfirm,
  onApplyRule,
  onUndo,
  onClose,
}: {
  step: Step;
  timeoutSecs: number;
  isBusy: boolean;
  onConfirm: () => void;
  onApplyRule: (rule: RuleProposal) => void;
  onUndo: (id: string) => void;
  onClose: () => void;
}) {
  if (step.kind === "rules") {
    return (
      <RulesPanel
        preview={step.preview}
        limit={step.limit}
        applied={step.applied}
        timeoutSecs={timeoutSecs}
        isBusy={isBusy}
        onApply={onApplyRule}
        onClose={onClose}
      />
    );
  }
  if (step.kind === "handoff") {
    return (
      <HandoffPanel
        preview={step.preview}
        timeoutSecs={timeoutSecs}
        isBusy={isBusy}
        onConfirm={onConfirm}
        onCancel={onClose}
      />
    );
  }
  if (step.kind === "preview") {
    return (
      <ChangePanel
        title={step.action.label}
        note={t.checksView.note}
        file={step.preview?.file ?? null}
        rows={step.preview?.changes ?? null}
        isUnchanged={step.preview !== null && step.preview.changes.length === 0}
        confirmLabel={t.checksView.confirm}
        isBusy={isBusy}
        onConfirm={onConfirm}
        onCancel={onClose}
      />
    );
  }
  return (
    <DonePanel
      applied={step.applied}
      title={t.checksView.done(step.applied.name, step.applied.action === "undo")}
      isBusy={isBusy}
      onUndo={onUndo}
      onClose={onClose}
    />
  );
}
