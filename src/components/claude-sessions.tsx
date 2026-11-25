import { Dialog } from "@base-ui/react/dialog";
import { useCallback, useEffect, useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import { StateText } from "@/components/state-text";
import { t } from "@/lib/i18n";
import {
  errorMessage,
  listClaudeSessions,
  stopIdleClaudeSessions,
  type ClaudeSession,
  type ClaudeSessions,
} from "@/lib/ipc";
import { formatDuration } from "@/lib/pace";

const statusLabel = (status: string | null) => {
  if (status === "waiting" || status === "busy" || status === "idle") return t.sessions[status];
  return t.sessions.unknown;
};

function SessionRows({ sessions }: { sessions: ClaudeSession[] }) {
  return (
    <ul className="flex max-h-[150px] flex-col gap-[6px] overflow-y-auto text-[10px]">
      {sessions.map((s, i) => (
        <li key={`${s.pid}-${s.sessionId}-${i}`} className="flex min-w-0 flex-col gap-px">
          <span className="tabular-nums">
            {statusLabel(s.status)}
            {s.idleSeconds !== null && ` · IDLE ${formatDuration(s.idleSeconds * 1000)}`}
            {s.pid !== null && ` · PID ${s.pid}`}
          </span>
          {s.cwd && (
            <span className="truncate text-muted-foreground" title={s.cwd}>
              {s.cwd}
            </span>
          )}
        </li>
      ))}
    </ul>
  );
}

export function ClaudeSessionsSection() {
  const [inventory, setInventory] = useState<ClaudeSessions | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<string | null>(null);
  const [targets, setTargets] = useState<ClaudeSession[] | null>(null);
  const [isBusy, setIsBusy] = useState(true);
  const request = useRef(0);
  const stopping = useRef(false);
  const load = useCallback(() => {
    const current = ++request.current;
    return listClaudeSessions()
      .then(
        (next) => {
          if (request.current !== current) return;
          setInventory(next);
          setError(null);
        },
        (reason: unknown) => {
          if (request.current !== current) return;
          setError(errorMessage(reason));
          setInventory(null);
        },
      )
      .finally(() => {
        if (request.current === current) setIsBusy(false);
      });
  }, []);
  useEffect(() => {
    void load();
    const counter = request;
    const onFocus = () => {
      if (!stopping.current) {
        setIsBusy(true);
        void load();
      }
    };
    window.addEventListener("focus", onFocus);
    return () => {
      counter.current++;
      window.removeEventListener("focus", onFocus);
    };
  }, [load]);

  const confirm = async () => {
    if (!targets || stopping.current) return;
    stopping.current = true;
    setIsBusy(true);
    setError(null);
    try {
      const next = await stopIdleClaudeSessions(
        targets.flatMap((s) =>
          s.pid !== null && s.sessionId !== null ? [{ pid: s.pid, sessionId: s.sessionId }] : [],
        ),
      );
      setResult([t.sessions.result(next.stopped, next.skipped.length), ...next.skipped].join("\n"));
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setTargets(null);
      stopping.current = false;
      setIsBusy(false);
    }
    // Refresh on this user action, retaining any stop failure message.
    try {
      setInventory(await listClaudeSessions());
    } catch (reason) {
      setError(errorMessage(reason));
      setInventory(null);
    }
  };
  const eligible = inventory?.sessions.filter((s) => s.stopEligible) ?? [];
  const sessions = inventory?.sessions ?? [];

  return (
    <section
      aria-label={t.sessions.title}
      className="mx-[20px] flex flex-col gap-[6px] border-t pt-[6px]"
    >
      <div className="flex items-center justify-between gap-1">
        <h3 className="text-[13px] font-medium">{t.sessions.title}</h3>
        <Button
          variant="ghost"
          size="xs"
          disabled={isBusy}
          onClick={() => {
            setIsBusy(true);
            void load();
          }}
        >
          {t.sessions.refresh}
        </Button>
      </div>
      {error && <StateText kind="error">{error}</StateText>}
      {!inventory && !error && <StateText kind="loading" />}
      {inventory &&
        (sessions.length === 0 ? (
          <StateText kind="empty">{t.sessions.empty}</StateText>
        ) : (
          <details>
            <summary className="cursor-pointer rounded text-[10px] tabular-nums hover:bg-accent focus-visible:ring-3 focus-visible:ring-ring active:translate-y-px">
              {(["waiting", "busy", "idle"] as const)
                .map(
                  (status) =>
                    `${t.sessions[status]} ${sessions.filter((s) => s.status === status).length}`,
                )
                .join(" · ")}
            </summary>
            <div className="pt-[6px]">
              <SessionRows sessions={sessions} />
            </div>
          </details>
        ))}
      {inventory?.canStop && (
        <Button
          variant="outline"
          size="xs"
          disabled={isBusy || eligible.length === 0}
          onClick={() => setTargets(eligible)}
        >
          {t.sessions.stop(inventory.idleHours, eligible.length)}
        </Button>
      )}
      {result && (
        <p
          role="status"
          className="whitespace-pre-line text-[10px] text-muted-foreground tabular-nums"
        >
          {result}
        </p>
      )}
      <Dialog.Root
        open={targets !== null}
        onOpenChange={(open) => {
          if (!open && !stopping.current) setTargets(null);
        }}
      >
        <Dialog.Portal>
          <Dialog.Backdrop className="fixed inset-0 bg-background/80" />
          <Dialog.Popup className="fixed top-1/2 left-1/2 flex max-h-[90vh] w-[286px] -translate-x-1/2 -translate-y-1/2 flex-col gap-3 overflow-y-auto rounded-lg border bg-popover p-3 text-popover-foreground shadow-lg">
            <Dialog.Title className="text-[13px] font-medium">{t.sessions.confirm}</Dialog.Title>
            <Dialog.Description className="text-[11px] text-pretty text-muted-foreground">
              {t.sessions.loss}
            </Dialog.Description>
            <SessionRows sessions={targets ?? []} />
            <div className="flex flex-wrap justify-end gap-2">
              <Button
                variant="outline"
                size="sm"
                disabled={isBusy}
                onClick={() => setTargets(null)}
              >
                {t.sessions.cancel}
              </Button>
              <Button size="sm" disabled={isBusy} onClick={() => void confirm()}>
                {t.sessions.confirmStop}
              </Button>
            </div>
          </Dialog.Popup>
        </Dialog.Portal>
      </Dialog.Root>
    </section>
  );
}
