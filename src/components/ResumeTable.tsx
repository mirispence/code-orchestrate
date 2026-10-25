import { useId } from "react";
import { History } from "lucide-react";

import { headlineTokens } from "@/components/StatTiles";
import { Button } from "@/components/ui/button";
import type { Session } from "@/lib/api";
import { formatDuration, formatTokens, formatWhen } from "@/lib/format";

export const TRANSCRIPT_DELETED = "Transcript deleted";

type ResumeTableProps = {
  /** All the Workspace's Sessions, newest last activity first (the backend's order). */
  sessions: Session[];
  /** Whether a launch can start now (executable found, folder present, not busy). */
  canLaunch: boolean;
  /** Why launching is blocked, shown as the buttons' description; `hintId` names it. */
  hintId?: string;
  onResume: (sessionId: string) => void;
};

/** "Resume a past Session" (spec §8.2): every Session, no pagination. */
export function ResumeTable({ sessions, canLaunch, hintId, onResume }: ResumeTableProps) {
  const deletedId = useId();

  return (
    <section className="flex flex-col gap-2">
      <h3 className="text-sm font-medium">Resume a past Session</h3>
      {sessions.length === 0 ? (
        <p className="text-sm text-muted-foreground">No Sessions yet.</p>
      ) : (
        <div className="overflow-x-auto rounded-md border">
          <table className="w-full text-sm">
            <thead className="bg-muted/50 text-left text-xs text-muted-foreground">
              <tr>
                <th className="px-3 py-2 font-medium">Title</th>
                <th className="px-3 py-2 font-medium">ID</th>
                <th className="px-3 py-2 font-medium">Started</th>
                <th className="px-3 py-2 font-medium">Active time</th>
                <th className="px-3 py-2 font-medium">Tokens</th>
                <th className="px-3 py-2">
                  <span className="sr-only">Resume</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {sessions.map((s) => {
                const deleted = !s.transcriptExists;
                return (
                  <tr key={`${s.agentId}:${s.sessionId}`} className="border-t">
                    <td className="max-w-0 truncate px-3 py-2" title={s.title ?? undefined}>
                      {s.title ?? "Untitled"}
                    </td>
                    <td className="px-3 py-2 font-mono text-xs" title={s.sessionId}>
                      {s.sessionId.slice(0, 8)}
                    </td>
                    <td className="px-3 py-2 whitespace-nowrap">{formatWhen(s.startedAt)}</td>
                    <td className="px-3 py-2 whitespace-nowrap">{formatDuration(s.activeTimeMs)}</td>
                    <td className="px-3 py-2 whitespace-nowrap">{formatTokens(headlineTokens(s.tokens))}</td>
                    <td className="px-3 py-2 text-right">
                      {/* A disabled button gets no hover events, so the tooltip sits on a wrapper. */}
                      <span className="inline-block" title={deleted ? TRANSCRIPT_DELETED : undefined}>
                        <Button
                          size="sm"
                          variant="outline"
                          disabled={deleted || !canLaunch}
                          aria-describedby={deleted ? deletedId : hintId}
                          onClick={() => onResume(s.sessionId)}
                        >
                          <History />
                          Resume
                        </Button>
                      </span>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
      <span id={deletedId} className="sr-only">
        {TRANSCRIPT_DELETED}
      </span>
    </section>
  );
}
