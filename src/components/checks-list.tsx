// 최적화 목록(시안 J, 2026-11-14): 맨 위 "고친 수 / 전체" + 얇은 진행 막대 + 회색 한 줄. 아래는 고친 줄(체크·취소선·되돌리기)
// 다음에 남은 카드 한 줄씩. 누른 줄 하나만 진단하기(E)와 같은 테두리 카드로 펼쳐 설명·근거 칩·바뀌는 줄·버튼을 보인다.
// 펼친 카드의 주 버튼만 강조 버튼이다(design.md 6절 3). 무엇을 하나·파일 경로·명령·문서는 접힌 "파일 위치와 출처" 안.
import { Check } from "lucide-react";
import { useId, useState } from "react";

import { AGENT_COLOR } from "@/components/agent-metrics";
import { CopyCommand } from "@/components/coach-view";
import { SUMMARY_CLASS } from "@/components/info-section";
import { Inline } from "@/components/rules-panel";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  appliedName,
  checkChips,
  checkCopy,
  diffLines,
  handoffGate,
  isHandoffCard,
  leftText,
  modelLabel,
  modelRows,
  remainingText,
  rowMeta,
  type CheckAction,
  type CheckCopy,
} from "@/lib/check-copy";
import { numberFormat, t } from "@/lib/i18n";
import type { Agent, AppliedSwitch, CoachCheck } from "@/lib/ipc";

/** Claude에게 맡기기 버튼 판정에 쓰는 값. `remaining`은 기존 사용량 출처(`get_usage`·`usage-updated`)의 Claude 최소 남은 %. */
export type HandoffContext = {
  remaining: number | null;
  minRemaining: number;
  timeoutSecs: number;
};

const integer = numberFormat({ maximumFractionDigits: 0 });
const MUTED = "text-[10px] text-muted-foreground";
/** 펼친 카드의 바뀌는 줄 미리보기 최대 줄 수(전부는 "파일 위치와 출처"). */
const DIFF_PREVIEW_LINES = 4;
/** 진행 막대 폭(px). 숫자가 함께 있어 막대는 보조다. */
const PROGRESS_WIDTH = 160;

const fileName = (path: string) => path.split(/[/\\]/).pop() ?? path;

const EXPLAIN = ["what", "mine", "who", "better"] as const;

function Explain({ copy }: { copy: CheckCopy }) {
  return (
    <dl className="flex flex-col gap-1 text-xs">
      {EXPLAIN.map((key) => (
        <div key={key}>
          <dt className={MUTED}>{t.checksView[key]}</dt>
          <dd>
            {copy[key]}
            {key === "mine" && copy.mineList.length > 0 && (
              <ol className="mt-0.5 list-decimal pl-4">
                {copy.mineList.map((item) => (
                  <li key={item}>{item}</li>
                ))}
              </ol>
            )}
          </dd>
        </div>
      ))}
    </dl>
  );
}

type CardHandlers = {
  onPick: (check: CoachCheck, action: CheckAction) => void;
  onKeep: (check: CoachCheck) => void;
};

/** 화면에서 강조 버튼은 하나(design.md 6절 3): 펼친 카드의 주 버튼만 `default`, 나머지는 `outline` + "추천" 표시. */
type Emphasis = { isEmphasized: boolean };

function RecommendedMark() {
  return <span className="rounded bg-current/20 px-1 text-[10px]">{t.checksView.recommended}</span>;
}

/** 추천이 `keep`인 카드의 주 버튼. 누르면 카드를 숨긴다. */
function KeepAction({
  reason,
  isBusy,
  isEmphasized,
  onKeep,
}: {
  reason: string;
  isBusy: boolean;
  onKeep: () => void;
} & Emphasis) {
  return (
    <div className="flex flex-col gap-0.5">
      <div className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5">
        <Button
          size="xs"
          variant={isEmphasized ? "default" : "outline"}
          className="w-fit"
          disabled={isBusy}
          onClick={onKeep}
        >
          {t.checks.keepLabel}
          <RecommendedMark />
        </Button>
        <span className="text-xs text-muted-foreground">{reason}</span>
      </div>
      <p className={MUTED}>{t.checks.keepNote}</p>
    </div>
  );
}

/** 쓰기 버튼 한 줄 + 설명. 추천이면 "추천" 표시, 강조 칸이면 `default`. */
function WritableAction({
  action,
  isRecommended,
  isEmphasized,
  isBusy,
  onPick,
}: {
  action: CheckAction;
  isRecommended: boolean;
  isBusy: boolean;
  onPick: () => void;
} & Emphasis) {
  return (
    <div className="flex flex-col gap-0.5">
      <Button
        size="xs"
        variant={isRecommended && isEmphasized ? "default" : "outline"}
        className="w-fit"
        disabled={isBusy}
        onClick={onPick}
      >
        {action.label}
        {isRecommended && <RecommendedMark />}
      </Button>
      {action.note && <p className={MUTED}>{action.note}</p>}
    </div>
  );
}

function Actions({
  check,
  copy,
  isBusy,
  isEmphasized,
  onPick,
  onKeep,
}: {
  check: CoachCheck;
  copy: CheckCopy;
  isBusy: boolean;
} & CardHandlers &
  Emphasis) {
  const facts = check.facts;
  return (
    <div className="flex flex-col gap-1.5 pt-0.5">
      {copy.keepReason !== null && (
        <KeepAction
          reason={copy.keepReason}
          isBusy={isBusy}
          isEmphasized={isEmphasized}
          onKeep={() => onKeep(check)}
        />
      )}
      {copy.actions.map((action) =>
        action.isWritable ? (
          <WritableAction
            key={action.option}
            action={action}
            // 숨기기(keep)가 추천인 카드는 쓰기 버튼을 추천하지 않는다.
            isRecommended={action.isRecommended && copy.keepReason === null}
            isEmphasized={isEmphasized}
            isBusy={isBusy}
            onPick={() => onPick(check, action)}
          />
        ) : (
          facts.type === "agentsMdHook" && (
            <div key={action.option} className="flex flex-col gap-0.5">
              <p className={`${MUTED} break-all`}>{facts.settingsFile}</p>
              <CopyCommand command={facts.command} label={t.checksView.copy} />
            </div>
          )
        ),
      )}
    </div>
  );
}

/** Claude에게 맡기는 카드의 버튼: 주 버튼 + "그대로 두기"(숨김) + "한도를 씁니다 · 지금 남은 한도 N%" 한 줄. */
function HandoffActions({
  check,
  copy,
  handoff,
  isBusy,
  isEmphasized,
  onPick,
  onKeep,
}: {
  check: CoachCheck;
  copy: CheckCopy;
  handoff: HandoffContext;
  isBusy: boolean;
} & CardHandlers &
  Emphasis) {
  const action = copy.actions[0];
  if (!action) return null;
  const gate = handoffGate(handoff.remaining, handoff.minRemaining);
  return (
    <div className="flex flex-col gap-0.5 pt-0.5">
      <div className="flex flex-wrap items-center gap-1.5">
        <Button
          size="xs"
          variant={isEmphasized ? "default" : "outline"}
          disabled={isBusy || !gate.isOpen}
          onClick={() => onPick(check, action)}
        >
          {action.label}
        </Button>
        <Button size="xs" variant="ghost" disabled={isBusy} onClick={() => onKeep(check)}>
          {t.checks.keepLabel}
        </Button>
      </div>
      <p className={`${MUTED} tabular-nums`}>
        {`${t.checksView.usesLimit} · ${remainingText(handoff.remaining)}`}
      </p>
      {gate.reason && <p className="text-[10px] text-destructive">{gate.reason}</p>}
    </div>
  );
}

/** 접힌 "파일 위치와 출처": 무엇을 하나·내 것 중 바뀌는 것·누가·좋아지는 것, 파일 경로, 바뀌는 키 전부, 명령, 문서, 확인 날짜. */
function Details({ check, copy }: { check: CoachCheck; copy: CheckCopy }) {
  const f = check.facts;
  const rows = modelRows(f);
  const writable = check.options.filter((o) => o.isWritable && o.changes.length > 0);
  const labelOf = (id: string) => copy.actions.find((a) => a.option === id)?.label ?? id;
  return (
    <details className="text-[10px]">
      <summary className={`${SUMMARY_CLASS} cursor-pointer text-muted-foreground`}>
        {t.checksView.sources}
      </summary>
      <div className="flex flex-col gap-1 pt-1.5">
        {copy.note && <p className="text-xs text-muted-foreground">{copy.note}</p>}
        <Explain copy={copy} />
      </div>
      <div className="flex flex-col gap-1.5 pt-1.5 text-muted-foreground">
        <p className="break-all select-text">{t.checksView.file(check.file)}</p>
        {writable.map((o) => (
          <div key={o.id}>
            <p className="text-foreground">{t.checksView.changesIf(labelOf(o.id))}</p>
            {o.changes.map((c) => (
              <p key={c.location} className="font-mono break-all select-text">
                {`${c.location}: ${c.before ?? t.common.none} → ${c.after ?? t.common.removed}`}
              </p>
            ))}
          </div>
        ))}
        {f.type === "gitInstructions" && (
          <ul className="flex flex-col gap-0.5">
            {f.matches.map((m) => (
              <li key={`${m.file}:${m.line}`} className="break-all select-text">
                <span className="font-mono">{`${m.file}:${m.line}`}</span>
                {` ${m.text}`}
              </li>
            ))}
          </ul>
        )}
        {f.type === "agentsMdHook" && (
          <p className="break-all select-text">{`hook: ${f.command} · ${f.agentsMd}`}</p>
        )}
        {f.type === "sameSkill" && (
          <>
            <p className="break-all select-text">{t.checksView.personal(f.personalPath)}</p>
            <p className="break-all select-text">{t.checksView.plugin(f.pluginId, f.pluginPath)}</p>
          </>
        )}
        {f.type === "rulesPaths" && (
          <>
            <p className="font-mono break-all">
              claude -p --safe-mode --model claude-opus-5-5 --effort medium --tools ""
            </p>
            <ul className="flex flex-col gap-0.5">
              {f.rules.map((r) => (
                <li key={r.name} className="break-all select-text">
                  {t.checksView.ruleChars(r.name, integer.format(r.chars))}
                </li>
              ))}
            </ul>
          </>
        )}
        {f.type === "longInstructions" && (
          <>
            <p className="font-mono break-all">
              claude -p --safe-mode --model claude-opus-5-5 --effort medium --tools ""
            </p>
            {f.imports.length > 0 && (
              <p className="break-all select-text">
                {t.checksView.imports(f.imports.map((i) => `@${i}`).join(", "))}
              </p>
            )}
          </>
        )}
        {rows.length > 0 && (
          <table className="w-full text-left tabular-nums">
            <thead>
              <tr className="text-foreground">
                <th className="font-normal">{t.checksView.model}</th>
                <th className="text-right font-normal">{t.checksView.requests}</th>
                <th className="pl-2 font-normal">{t.checksView.current}</th>
                <th className="font-normal">{t.checksView.verdict}</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => (
                <tr key={r.label}>
                  <td>{r.label}</td>
                  <td className="text-right">{integer.format(r.requests)}</td>
                  <td className="pl-2">{r.current}</td>
                  <td>{r.verdict}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        {"unknownModels" in f && f.unknownModels.length > 0 && (
          <p>
            {t.checksView.unknownModels(f.unknownModels.map((m) => modelLabel(m.model)).join(", "))}
          </p>
        )}
        <ul className="flex flex-col gap-0.5">
          {check.docs.map((url) => (
            <li key={url} className="font-mono break-all select-text">
              {url}
            </li>
          ))}
        </ul>
        <p>{t.checks.verified}</p>
      </div>
    </details>
  );
}

/** 줄 앞 체크 원. 고친 줄은 에이전트 색으로 채우고 체크 표시(색만으로 알리지 않는다: 취소선·"고침" 읽기 이름도 있다). */
function CheckMark({
  agent,
  isOn,
  isDecorative = false,
}: {
  agent: Agent;
  isOn: boolean;
  /** 옆 글자가 뜻을 말하면 읽기 이름을 빼고 숨긴다. */
  isDecorative?: boolean;
}) {
  if (!isOn) {
    return (
      <span
        aria-hidden
        className="size-3.5 shrink-0 rounded-full border-[1.5px] border-muted-foreground/70"
      />
    );
  }
  return (
    <span
      role={isDecorative ? undefined : "img"}
      aria-hidden={isDecorative || undefined}
      aria-label={isDecorative ? undefined : t.checksView.fixed}
      style={{ backgroundColor: AGENT_COLOR[agent] }}
      className="inline-grid size-3.5 shrink-0 place-items-center rounded-full text-background"
    >
      <Check aria-hidden className="size-2.5" strokeWidth={3} />
    </span>
  );
}

/** 바뀌는 줄 몇 줄(기존 `changes` before/after). 빼는 줄 `-`(빨강), 넣는 줄 `+`. 전부는 "파일 위치와 출처". */
function DiffPreview({ check }: { check: CoachCheck }) {
  const { lines, more } = diffLines(check, DIFF_PREVIEW_LINES);
  if (lines.length === 0) return null;
  return (
    <div
      aria-label={t.checksView.diff}
      className="flex flex-col gap-0.5 rounded-md bg-muted px-2 py-1 font-mono text-[10px] leading-relaxed"
    >
      <span className="font-sans text-muted-foreground">{fileName(check.file)}</span>
      {lines.map((line, i) => (
        <span
          key={i}
          className={`break-all ${line.kind === "removed" ? "text-destructive" : "text-foreground"}`}
        >
          {`${line.kind === "removed" ? "- " : "+ "}${line.text}`}
        </span>
      ))}
      {more > 0 && (
        <span className="font-sans text-muted-foreground">
          {t.checksView.moreLines(integer.format(more))}
        </span>
      )}
    </div>
  );
}

/** 접힌 줄의 작은 버튼: 쓰기 추천 → "고치기", keep 추천 → "그대로 두기", 맡기기 → "Claude에게". 안내만 하는 카드는 없음. */
function RowAction({
  check,
  copy,
  handoff,
  isBusy,
  onPick,
  onKeep,
}: {
  check: CoachCheck;
  copy: CheckCopy;
  handoff: HandoffContext;
  isBusy: boolean;
} & CardHandlers) {
  const button = (label: string, onClick: () => void, isDisabled = false) => (
    <Button
      size="xs"
      variant="outline"
      className="relative z-10"
      aria-label={`${copy.shortTitle} ${label}`}
      disabled={isBusy || isDisabled}
      onClick={onClick}
    >
      {label}
    </Button>
  );
  if (isHandoffCard(check)) {
    const action = copy.actions[0];
    if (!action) return null;
    const gate = handoffGate(handoff.remaining, handoff.minRemaining);
    return button(t.checksView.rowAsk, () => onPick(check, action), !gate.isOpen);
  }
  if (copy.keepReason !== null) return button(t.checks.keepLabel, () => onKeep(check));
  const action = copy.actions.find((a) => a.isRecommended && a.isWritable);
  return action ? button(t.checksView.rowFix, () => onPick(check, action)) : null;
}

/** 남은 카드 한 줄. 접히면 ○ 짧은 제목 · 메타 · 작은 버튼, 펼치면 테두리 카드(진단하기 E의 펼친 줄과 같은 강조). */
export function CheckRow({
  check,
  handoff,
  isOpen,
  isBusy,
  isEmphasized,
  onToggle,
  onPick,
  onKeep,
}: {
  check: CoachCheck;
  handoff: HandoffContext;
  isOpen: boolean;
  isBusy: boolean;
  onToggle: () => void;
} & CardHandlers &
  Emphasis) {
  const detailsId = useId();
  const copy = checkCopy(check);
  const meta = rowMeta(check);
  return (
    <div
      role="group"
      aria-label={copy.title}
      className={`flex flex-col gap-1.5 rounded-lg border text-xs ${isOpen ? "mx-3 px-2 py-1.5" : "mx-5 border-transparent"}`}
      style={isOpen ? { borderColor: AGENT_COLOR[check.agent] } : undefined}
    >
      {/* 줄 전체가 펼치기 버튼의 누르는 자리다: 버튼의 ::after가 줄을 덮고, 작은 버튼만 그 위(z-10)에 둔다(중첩 button 없음). */}
      <div className="relative flex items-center gap-2 rounded-md hover:bg-accent">
        <button
          type="button"
          aria-expanded={isOpen}
          aria-controls={detailsId}
          onClick={onToggle}
          className="flex min-h-6 min-w-0 flex-1 cursor-pointer items-center gap-1.5 text-left after:absolute after:inset-0 after:rounded-md after:content-[''] focus-visible:outline-none focus-visible:after:ring-3 focus-visible:after:ring-ring active:translate-y-px"
        >
          <CheckMark agent={check.agent} isOn={false} />
          <span className="truncate font-medium" title={copy.title}>
            {copy.shortTitle}
          </span>
          {meta && (
            <span
              className={`ml-auto shrink-0 pl-2 whitespace-nowrap tabular-nums ${meta.isEstimate ? "font-semibold" : "text-[11px] text-muted-foreground"}`}
              title={meta.isEstimate ? t.info.estimated : undefined}
            >
              {meta.text}
            </span>
          )}
        </button>
        {!isOpen && (
          <RowAction
            check={check}
            copy={copy}
            handoff={handoff}
            isBusy={isBusy}
            onPick={onPick}
            onKeep={onKeep}
          />
        )}
      </div>
      {isOpen && (
        <div id={detailsId} className="flex flex-col gap-1.5 pl-5">
          <p className="text-xs">
            <Inline text={copy.headline} />
          </p>
          <ul className="flex flex-wrap gap-1 text-[10px] tabular-nums">
            {checkChips(check).map((chip) => (
              <li key={chip}>
                <Badge variant="outline" className="h-4 px-1.5 py-0 text-[10px] leading-none">
                  {chip}
                </Badge>
              </li>
            ))}
          </ul>
          <DiffPreview check={check} />
          {isHandoffCard(check) ? (
            <HandoffActions
              check={check}
              copy={copy}
              handoff={handoff}
              isBusy={isBusy}
              isEmphasized={isEmphasized}
              onPick={onPick}
              onKeep={onKeep}
            />
          ) : (
            <Actions
              check={check}
              copy={copy}
              isBusy={isBusy}
              isEmphasized={isEmphasized}
              onPick={onPick}
              onKeep={onKeep}
            />
          )}
          <Details check={check} copy={copy} />
        </div>
      )}
    </div>
  );
}

/** 고친 줄: ✓ 흐린 취소선 이름 + "되돌리기"(기존 `undoSwitch` 흐름). 이름은 적용 기록의 이름이다. */
export function FixedRow({
  entry,
  isBusy,
  onUndo,
}: {
  entry: AppliedSwitch;
  isBusy: boolean;
  onUndo: (id: string) => void;
}) {
  const name = appliedName(entry);
  return (
    <div className="mx-5 flex min-h-6 items-center gap-2 text-xs text-muted-foreground">
      <CheckMark agent={entry.agent} isOn />
      <s
        className="min-w-0 flex-1 truncate decoration-muted-foreground"
        title={entry.created ? `${entry.file} → ${entry.created}` : entry.file}
      >
        {name}
      </s>
      <Button
        size="xs"
        variant="ghost"
        aria-label={`${name} ${t.coach.undo}`}
        disabled={isBusy}
        onClick={() => onUndo(entry.id)}
      >
        {t.coach.undo}
      </Button>
    </div>
  );
}

/** 맨 위 요약: 큰 숫자 "고친 수 / 전체" + 얇은 진행 막대(숫자가 함께 있다) + 회색 한 줄 + 오른쪽 "읽는 법". */
export function ProgressSummary({
  agent,
  fixed,
  left,
}: {
  agent: Agent;
  fixed: number;
  left: CoachCheck[];
}) {
  const [isLegendOpen, setIsLegendOpen] = useState(false);
  const legendId = useId();
  const total = fixed + left.length;
  const percent = total === 0 ? 0 : (fixed / total) * 100;
  return (
    <div className="flex flex-col gap-1 px-5 pt-2.5 pb-2">
      <div className="flex items-start gap-2">
        <div className="flex min-w-0 flex-1 flex-col gap-1">
          <p
            className="text-2xl leading-tight font-semibold tabular-nums"
            aria-label={t.checksView.progress(integer.format(fixed), integer.format(total))}
          >
            {`${integer.format(fixed)} / ${integer.format(total)}`}
            <span className="ml-1 text-xs font-medium text-muted-foreground">
              {t.checksView.fixedUnit}
            </span>
          </p>
          <div
            aria-hidden
            className="h-1.5 max-w-full overflow-hidden rounded-full bg-muted"
            style={{ width: PROGRESS_WIDTH }}
          >
            <div
              className="h-full rounded-full"
              style={{ width: `${percent}%`, backgroundColor: AGENT_COLOR[agent] }}
            />
          </div>
          <p className="text-[10px] text-muted-foreground tabular-nums">{leftText(left)}</p>
        </div>
        <Button
          size="xs"
          variant="ghost"
          aria-expanded={isLegendOpen}
          aria-controls={legendId}
          onClick={() => setIsLegendOpen((open) => !open)}
          className="text-muted-foreground"
        >
          {t.coach.howToRead}
        </Button>
      </div>
      {isLegendOpen && (
        <p id={legendId} className="text-[10px] text-muted-foreground">
          {t.checksView.legend}
        </p>
      )}
    </div>
  );
}

/** 고칠 것이 없을 때(점검은 돌았다): 빈칸 대신 고친 줄과 같은 체크 표시로 "다 됐다"를 보인다. */
export function AllClear({ agent, text }: { agent: Agent; text: string }) {
  return (
    <p className="flex items-center gap-1.5 px-5 py-2.5 text-xs font-medium">
      <CheckMark agent={agent} isOn isDecorative />
      {text}
    </p>
  );
}
