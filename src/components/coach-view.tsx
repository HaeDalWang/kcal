// 5층 코칭 "진단하기"(docs/decisions/0014, 자리는 대시보드 점검 탭 — 0017 "화면 자리"): 끄면 아낄 수 있는 스위치 → 바꿀 줄 미리보기 → 확인 → 적용(백업) → 되돌리기.
// 값은 열 때와 적용·되돌리기 뒤에만 읽는다(타이머 없음). 쓰기는 Rust가 대상 ID로만 한다.
// Claude는 최근 N일 사용 기록과 지금 범위를 같이 보이고, 쓴 것은 끄기 대신 "그대로"나 "프로젝트에서만 켜기"(0022). 목록 모양은 `coach-list.tsx`.
import { useCallback, useEffect, useRef, useState, type KeyboardEvent } from "react";

import { Button } from "@/components/ui/button";
import { GROUP_TITLE, SwitchLists, groupOf } from "@/components/coach-list";
import { appliedName } from "@/lib/check-copy";
import {
  CHECK_TARGET_PREFIX,
  applyScopeMove,
  applySwitch,
  errorMessage,
  previewScopeMove,
  previewSwitch,
  undoSwitch,
  type Agent,
  type AppliedSwitch,
  type CoachSwitch,
  type ScopeMovePreview,
  type SwitchPreview,
} from "@/lib/ipc";
import { ratesStore, useSnapshot } from "@/lib/snapshot-store";
import { t } from "@/lib/i18n";
import type { OpenRow } from "@/lib/open-row";

/** 미리보기·완료 패널이 열리면 누른 버튼이 사라져 포커스가 body로 떨어진다. 패널로 옮기고 Escape로 닫게 한다(키보드 사용자는 이어서 Tab). */
export function usePanelFocus(onEscape: () => void, isDisabled = false) {
  const ref = useRef<HTMLElement>(null);
  useEffect(() => {
    ref.current?.focus();
  }, []);
  return {
    ref,
    tabIndex: -1,
    onKeyDown: (event: KeyboardEvent) => {
      if (event.key !== "Escape" || isDisabled) return;
      event.stopPropagation();
      onEscape();
    },
  };
}

/** 화면에 보일 최근 적용 기록 수. */
const RECENT_LOG = 3;

type Step =
  | { kind: "list" }
  | { kind: "preview"; target: CoachSwitch; enabled: boolean; preview: SwitchPreview | null }
  | { kind: "manual"; target: CoachSwitch }
  | { kind: "move"; target: CoachSwitch; preview: ScopeMovePreview | null }
  | { kind: "done"; applied: AppliedSwitch };

/** 바뀌는 곳 하나. `before`가 null이면 새로 넣고, `after`가 null이면 지운다. */
export type ChangeRow = { location: string; before: string | null; after: string | null };

/** 바꿀 내용 미리보기(코칭·최적화 공용). `rows`가 null이면 아직 미리 보는 중. */
export function ChangePanel({
  title,
  note,
  file,
  rows,
  isUnchanged,
  confirmLabel,
  isBusy,
  onConfirm,
  onCancel,
}: {
  title: string;
  note: string;
  file: string | null;
  rows: ChangeRow[] | null;
  /** 이미 그 값이라 바꿀 것이 없다. */
  isUnchanged: boolean;
  confirmLabel: string;
  isBusy: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const focus = usePanelFocus(onCancel, isBusy);
  return (
    <section
      {...focus}
      role="group"
      aria-label={t.coach.review}
      className="flex flex-col gap-1.5 border-t px-5 py-2.5 outline-none"
    >
      <h2 className="text-[13px] font-medium">{title}</h2>
      {!rows && <p className="text-[10px] text-muted-foreground">{t.coach.previewing}</p>}
      {rows && (
        <>
          <p className="text-[10px] text-muted-foreground">{note}</p>
          {file && (
            <p className="truncate text-[10px] text-muted-foreground" title={file}>
              {file}
            </p>
          )}
          {rows.map((row) => (
            <div key={row.location} className="flex flex-col gap-1.5">
              <p className="text-[10px] break-all text-muted-foreground">{row.location}</p>
              {/* 새로 넣는 줄은 `+`만, 지우는 줄은 `-`만 보인다("+ (지움)" 같은 가짜 줄 없이). 표시는 글자(-/+)와 색을 함께 쓴다. */}
              <pre className="overflow-x-auto rounded-md bg-muted px-2 py-1 font-mono text-[10px] leading-relaxed whitespace-pre">
                {row.before !== null && (
                  <span className="block text-destructive">{`- ${row.before}`}</span>
                )}
                {row.after !== null && (
                  <span className="block text-foreground">{`+ ${row.after}`}</span>
                )}
              </pre>
            </div>
          ))}
          {isUnchanged && <p className="text-[10px] text-muted-foreground">{t.coach.unchanged}</p>}
        </>
      )}
      <div className="flex justify-end gap-1.5 pt-0.5">
        <Button size="xs" variant="ghost" onClick={onCancel} disabled={isBusy}>
          {t.common.cancel}
        </Button>
        <Button size="xs" onClick={onConfirm} disabled={isBusy || !rows || isUnchanged}>
          {confirmLabel}
        </Button>
      </div>
    </section>
  );
}

function PreviewPanel({
  step,
  isBusy,
  onConfirm,
  onCancel,
}: {
  step: Extract<Step, { kind: "preview" }>;
  isBusy: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const { target, enabled, preview } = step;
  const isSkill = target.kind === "skill" || target.kind === "projectSkill";
  return (
    <ChangePanel
      title={
        isSkill
          ? t.coach.skillTitle(target.name, enabled)
          : t.coach.switchTitle(target.name, enabled)
      }
      note={isSkill ? t.coach.skillNote : t.coach.switchNote}
      file={preview?.file ?? null}
      rows={
        preview
          ? [{ location: preview.location, before: preview.before, after: preview.after }]
          : null
      }
      isUnchanged={preview !== null && !preview.changed}
      confirmLabel={isSkill ? t.coach.skillConfirm(enabled) : t.coach.confirm(enabled)}
      isBusy={isBusy}
      onConfirm={onConfirm}
      onCancel={onCancel}
    />
  );
}

function MovePanel({
  step,
  isBusy,
  onConfirm,
  onCancel,
}: {
  step: Extract<Step, { kind: "move" }>;
  isBusy: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const { target, preview } = step;
  const skipped = preview?.skipped ?? [];
  const note = target.agent === "codex" ? t.coach.moveNoteCodex : t.coach.moveNote;
  return (
    <ChangePanel
      title={t.coach.moveTitle(target.name)}
      note={skipped.length > 0 ? `${note} ${t.coach.skipped(skipped.join(", "))}` : note}
      file={null}
      rows={
        preview
          ? preview.changes.map((c) => ({
              location: `${c.file}${c.isNewFile ? ` (${t.coach.newFile})` : ""} · ${c.location}`,
              before: c.before,
              after: c.after,
            }))
          : null
      }
      isUnchanged={false}
      confirmLabel={t.coach.confirmMove}
      isBusy={isBusy}
      onConfirm={onConfirm}
      onCancel={onCancel}
    />
  );
}

function ManualPanel({ target, onCancel }: { target: CoachSwitch; onCancel: () => void }) {
  const focus = usePanelFocus(onCancel);
  return (
    <section
      {...focus}
      role="group"
      aria-label={t.coach.manual}
      className="flex flex-col gap-1.5 border-t px-5 py-2.5 outline-none"
    >
      <h2 className="text-[13px] font-medium">{target.name}</h2>
      <p className="text-xs">{target.instruction}</p>
      <div className="flex justify-end">
        <Button size="xs" variant="ghost" onClick={onCancel}>
          {t.coach.close}
        </Button>
      </div>
    </section>
  );
}

export function CopyCommand({ command, label }: { command: string; label?: string }) {
  const [copied, setCopied] = useState(false);
  const copy = () => {
    navigator.clipboard.writeText(command).then(
      () => setCopied(true),
      () => setCopied(false),
    );
  };
  return (
    <div className="flex items-center gap-1.5">
      <code className="min-w-0 flex-1 truncate font-mono text-[10px] select-text" title={command}>
        {command}
      </code>
      <Button size="xs" variant="ghost" onClick={copy}>
        {copied ? t.coach.copied : (label ?? t.coach.copyCommand)}
      </Button>
    </div>
  );
}

/** 완료 패널 제목의 기본값: 점검 기록은 이름만, 스위치는 "<이름> 켬·끔", 되돌리기는 "<이름> 되돌림". */
export const doneTitle = (applied: AppliedSwitch) =>
  applied.action === "undo" ? t.coach.undone(applied.name) : appliedName(applied);

/** 적용·되돌리기 결과를 화면 읽기 프로그램에 알리는 자리. 늘 그려 두고 글자만 바꾼다(새로 붙인 live 영역은 읽히지 않을 수 있다). */
export function LiveStatus({ text }: { text: string | null }) {
  return (
    <p role="status" className="sr-only">
      {text ?? ""}
    </p>
  );
}

export function DonePanel({
  applied,
  title,
  isBusy,
  onUndo,
  onClose,
}: {
  applied: AppliedSwitch;
  /** 없으면 `appliedName`(점검 기록은 이름만, 스위치는 "<이름> 켬·끔") 또는 "<이름> 되돌림". */
  title?: string;
  isBusy: boolean;
  onUndo: (id: string) => void;
  onClose: () => void;
}) {
  const isUndo = applied.action === "undo";
  const focus = usePanelFocus(onClose);
  return (
    <section
      {...focus}
      role="group"
      aria-label={t.coach.result}
      className="flex flex-col gap-1.5 border-t px-5 py-2.5 outline-none"
    >
      <h2 className="text-[13px] font-medium">{title ?? doneTitle(applied)}</h2>
      <p className="text-xs">{t.coach.restart}</p>
      {/* 새로 만든 파일(스킬 숨기기)은 백업이 없다. */}
      {applied.backup && (
        <p className="truncate text-[10px] text-muted-foreground" title={applied.backup}>
          {t.coach.backup(applied.backup)}
        </p>
      )}
      {!isUndo && <CopyCommand command={applied.undoCommand} />}
      <div className="flex justify-end gap-1.5">
        {!isUndo && (
          <Button size="xs" variant="outline" onClick={() => onUndo(applied.id)} disabled={isBusy}>
            {t.coach.undo}
          </Button>
        )}
        <Button size="xs" variant="ghost" onClick={onClose}>
          {t.coach.toList}
        </Button>
      </div>
    </section>
  );
}

/** 아직 되돌리지 않은 최근 적용 기록. "효과 보기"는 사용량 탭의 처방 효과(0016)로 간다. */
function RecentLog({
  log,
  isBusy,
  onUndo,
  onShowEffect,
}: {
  log: AppliedSwitch[];
  isBusy: boolean;
  onUndo: (id: string) => void;
  onShowEffect: () => void;
}) {
  const undone = new Set(log.flatMap((e) => (e.undoOf ? [e.undoOf] : [])));
  const open = log.filter((e) => e.action === "apply" && !undone.has(e.id)).slice(0, RECENT_LOG);
  if (open.length === 0) return null;
  return (
    <section
      role="group"
      aria-label={t.coach.recent}
      className="flex flex-col gap-1.5 border-t px-5 py-2.5"
    >
      <h2 className={GROUP_TITLE}>{t.coach.recent}</h2>
      <ul className="flex flex-col gap-1.5">
        {open.map((e) => (
          <li key={e.id} className="flex items-center gap-2 text-xs">
            <span
              className="min-w-0 flex-1 truncate"
              title={e.created ? `${e.file} → ${e.created}` : e.file}
            >
              {appliedName(e)}
            </span>
            <Button size="xs" variant="ghost" onClick={onShowEffect}>
              {t.coach.showEffect}
            </Button>
            <Button size="xs" variant="ghost" onClick={() => onUndo(e.id)} disabled={isBusy}>
              {t.coach.undo}
            </Button>
          </li>
        ))}
      </ul>
    </section>
  );
}

/** 대시보드 점검 탭의 "진단하기" 칸(에이전트 하나). 목록은 점검 탭이 읽어 넘기고, 쓰기 뒤 `onChanged`로 다시 읽게 한다. */
export function CoachPanel({
  agent,
  isMissing = false,
  switches,
  log,
  onChanged,
  onShowEffect,
  openRow,
}: {
  agent: Agent;
  /** 이 에이전트를 못 찾았다(점검 탭이 알려 준다). */
  isMissing?: boolean;
  /** 모든 에이전트의 스위치(`get_switches`). 이 칸은 `agent` 것만 보인다. */
  switches: CoachSwitch[];
  log: AppliedSwitch[];
  onChanged: () => void;
  /** "효과 보기": 사용량 탭의 처방 효과로 간다. */
  onShowEffect: () => void;
  /** 점검 탭 전체의 펼친 줄(키 "switch:<target>"). 없으면 이 칸 안에서만 하나. */
  openRow?: OpenRow;
}) {
  const [step, setStep] = useState<Step>({ kind: "list" });
  const [error, setError] = useState<string | null>(null);
  const [isBusy, setIsBusy] = useState(false);
  const rates = useSnapshot(ratesStore, "coach");
  // 패널을 닫으면 그 줄의 버튼으로 포커스를 돌려준다(줄이 없어졌거나 버튼이 없으면 이 칸 맨 위로).
  const [returnTarget, setReturnTarget] = useState<string | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const onReturned = useCallback(() => setReturnTarget(null), []);

  const pick = (item: CoachSwitch) => {
    setError(null);
    if (item.format === "manual") {
      setStep({ kind: "manual", target: item });
      return;
    }
    const enabled = !item.enabled;
    setStep({ kind: "preview", target: item, enabled, preview: null });
    previewSwitch(item.target, enabled).then(
      (preview) => setStep({ kind: "preview", target: item, enabled, preview }),
      (reason: unknown) => {
        setStep({ kind: "list" });
        setError(errorMessage(reason));
      },
    );
  };

  const pickMove = (item: CoachSwitch) => {
    setError(null);
    setStep({ kind: "move", target: item, preview: null });
    previewScopeMove(item.target).then(
      (preview) => setStep({ kind: "move", target: item, preview }),
      (reason: unknown) => {
        setStep({ kind: "list" });
        setError(errorMessage(reason));
      },
    );
  };

  const write = (action: () => Promise<AppliedSwitch>) => {
    setError(null);
    setIsBusy(true);
    action().then(
      (applied) => {
        setIsBusy(false);
        setStep({ kind: "done", applied });
        onChanged();
      },
      (reason: unknown) => {
        setIsBusy(false);
        setError(errorMessage(reason));
      },
    );
  };

  const confirm = () => {
    if (step.kind !== "preview" || !step.preview) return;
    const { target, enabled, preview } = step;
    write(() => applySwitch(target.target, enabled, preview.beforeHash));
  };
  const confirmMove = () => {
    if (step.kind !== "move" || !step.preview) return;
    const { target, preview } = step;
    write(() => applyScopeMove(target.target, preview.beforeHash));
  };
  const undo = (id: string) => write(() => undoSwitch(id));
  const shown = switches.filter((s) => s.agent === agent);
  const toList = () => {
    const opener =
      step.kind === "done" ? step.applied.target : "target" in step ? step.target.target : null;
    const row = shown.find((s) => s.target === opener);
    // "그대로 둠" 줄은 접힌 목록 안이라 포커스를 못 받는다: 이 칸 맨 위로.
    if (row && groupOf(row) !== "keep") setReturnTarget(row.target);
    else rootRef.current?.focus();
    setStep({ kind: "list" });
  };
  // 최적화 점검 기록은 최적화 칸의 고친 줄에서 되돌린다(같은 되돌리기를 두 칸에 두지 않는다).
  const shownLog = log.filter(
    (e) => e.agent === agent && !e.target.startsWith(CHECK_TARGET_PREFIX),
  );
  return (
    <div ref={rootRef} tabIndex={-1} className="flex flex-col outline-none">
      <LiveStatus text={step.kind === "done" ? doneTitle(step.applied) : null} />
      {error && (
        <p role="alert" className="px-5 pt-1.5 text-[10px] text-destructive">
          {error}
        </p>
      )}
      {step.kind === "list" && (
        <>
          <SwitchLists
            agent={agent}
            isMissing={isMissing}
            switches={shown}
            rate={rates.value?.codex}
            onPick={pick}
            onMove={pickMove}
            openRow={openRow}
            returnTarget={returnTarget}
            onReturned={onReturned}
          />
          <RecentLog log={shownLog} isBusy={isBusy} onUndo={undo} onShowEffect={onShowEffect} />
        </>
      )}
      {step.kind === "preview" && (
        <PreviewPanel step={step} isBusy={isBusy} onConfirm={confirm} onCancel={toList} />
      )}
      {step.kind === "move" && (
        <MovePanel step={step} isBusy={isBusy} onConfirm={confirmMove} onCancel={toList} />
      )}
      {step.kind === "manual" && <ManualPanel target={step.target} onCancel={toList} />}
      {step.kind === "done" && (
        <DonePanel applied={step.applied} isBusy={isBusy} onUndo={undo} onClose={toList} />
      )}
    </div>
  );
}
