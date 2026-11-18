// 늘 실리는 규칙 카드(0018 작업 B)의 결과 칸: Claude가 규칙마다 제안한 것 한 줄씩 + "이대로 바꾸기".
// 적용은 Rust 보관본으로 한다(화면은 카드 id·규칙 열쇠·해시만 보낸다). 되돌리기는 진단하기 "최근 바꾼 것"에서.
// 문구는 `src/lib/check-copy.ts`(`ruleRowCopy`·`RULES_PREVIEW`).
import { TriangleAlert } from "lucide-react";
import { Fragment } from "react";

import { Button } from "@/components/ui/button";
import { ruleRowCopy, rulesRunningText, unknownKeysText } from "@/lib/check-copy";
import { t } from "@/lib/i18n";
import type { AppliedSwitch, RuleProposal, RulesPreview } from "@/lib/ipc";

const MUTED = "text-[10px] text-muted-foreground";

/** `` `이름` ``을 코드 글꼴로. */
export function Inline({ text }: { text: string }) {
  return (
    <>
      {text.split("`").map((part, i) =>
        i % 2 === 1 ? (
          <code key={i} className="rounded bg-muted px-1 font-mono text-[0.9em]">
            {part}
          </code>
        ) : (
          <Fragment key={i}>{part}</Fragment>
        ),
      )}
    </>
  );
}

function RuleRow({
  rule,
  limit,
  applied,
  isBusy,
  onApply,
}: {
  rule: RuleProposal;
  limit: number;
  applied: AppliedSwitch | undefined;
  isBusy: boolean;
  onApply: (rule: RuleProposal) => void;
}) {
  const copy = ruleRowCopy(rule, limit);
  const text = t.checks.rulesPreview;
  const isUnusable = rule.proposal.choice === "unusable";
  return (
    <li
      aria-label={rule.name}
      className="flex flex-col gap-0.5 border-t pt-1.5 text-xs [overflow-wrap:anywhere] first:border-t-0 first:pt-0"
    >
      <p className="font-medium tabular-nums">
        <Inline text={copy.title} />
      </p>
      <p className={isUnusable ? "text-destructive" : undefined}>
        <Inline text={copy.suggestion} />
      </p>
      {rule.reason && <p className={MUTED}>{rule.reason}</p>}
      {copy.loses && <p className="text-[10px]">{t.checksView.loses(copy.loses)}</p>}
      {/* 주의(오류 아님)는 --warn색 + 세모 아이콘: 색만으로 알리지 않는다(design.md 4절). */}
      {copy.warnings.map((w) => (
        <p key={w} className="flex items-start gap-1 text-[10px] text-warn">
          <TriangleAlert aria-hidden className="mt-px size-[10px] shrink-0" />
          {w}
        </p>
      ))}
      {applied ? (
        <p role="status" className="text-[10px] font-medium text-foreground">
          {`✓ ${text.done}`}
        </p>
      ) : (
        copy.canApply && (
          <Button
            size="xs"
            variant="outline"
            className="mt-0.5 w-fit"
            disabled={isBusy}
            aria-label={`${rule.name} ${text.apply}`}
            onClick={() => onApply(rule)}
          >
            {text.apply}
          </Button>
        )
      )}
    </li>
  );
}

/** 결과 칸. `preview`가 null이면 아직 도는 중. `applied`는 이 칸에서 바꾼 규칙(열쇠 → 기록). */
export function RulesPanel({
  preview,
  limit,
  applied,
  timeoutSecs,
  isBusy,
  onApply,
  onClose,
}: {
  preview: RulesPreview | null;
  limit: number;
  applied: Record<string, AppliedSwitch>;
  timeoutSecs: number;
  isBusy: boolean;
  onApply: (rule: RuleProposal) => void;
  onClose: () => void;
}) {
  const text = t.checks.rulesPreview;
  return (
    <section
      role="group"
      aria-label={text.title}
      className="flex flex-col gap-1.5 border-t px-5 py-2.5"
    >
      <h2 className="text-[13px] font-medium">{text.title}</h2>
      {!preview && (
        <p role="status" className="text-xs text-muted-foreground">
          {rulesRunningText(timeoutSecs)}
        </p>
      )}
      {preview && (
        <>
          <p className={MUTED}>{text.note}</p>
          {preview.rules.length === 0 && (
            <p className="text-xs text-muted-foreground">{text.empty}</p>
          )}
          <ul className="flex flex-col gap-1.5">
            {preview.rules.map((rule) => (
              <RuleRow
                key={rule.key}
                rule={rule}
                limit={limit}
                applied={applied[rule.key]}
                isBusy={isBusy}
                onApply={onApply}
              />
            ))}
          </ul>
          {preview.unknownKeys > 0 && (
            <p className="text-xs text-destructive">{unknownKeysText(preview.unknownKeys)}</p>
          )}
          <div className="flex justify-end pt-0.5">
            <Button size="xs" variant="ghost" onClick={onClose}>
              {text.close}
            </Button>
          </div>
        </>
      )}
    </section>
  );
}
