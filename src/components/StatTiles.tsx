import { ChevronDown } from "lucide-react";
import { type ReactNode, useId, useState } from "react";

import type { TokenUsage, WorkspaceStats } from "@/lib/api";
import { formatDuration, formatTokens, formatWhen } from "@/lib/format";
import { cn } from "@/lib/utils";

/** The headline: input + output + cache write. Cache reads are excluded (spec §5.6). */
export function headlineTokens(tokens: TokenUsage): number | null {
  const parts = [tokens.input, tokens.output, tokens.cacheWrite].filter((n): n is number => n !== null);
  return parts.length === 0 ? null : parts.reduce((a, b) => a + b, 0);
}

const BREAKDOWN: [keyof TokenUsage, string][] = [
  ["input", "Input"],
  ["output", "Output"],
  ["cacheWrite", "Cache write"],
  ["cacheRead", "Cache read"],
];

/** The four stat tiles of a Workspace (spec §8.2). */
export function StatTiles({ stats }: { stats: WorkspaceStats }) {
  const [expanded, setExpanded] = useState(false);
  const breakdownId = useId();
  const breakdownText = BREAKDOWN.map(([key, label]) => `${label}: ${formatTokens(stats.tokens[key])}`).join("\n");

  return (
    <div role="group" aria-label="Stats" className="flex flex-col gap-2">
      <dl className="grid grid-cols-2 gap-2 sm:grid-cols-4">
        <Tile label="Sessions">{stats.sessionCount}</Tile>
        <Tile label="Last used">{formatWhen(stats.lastUsed)}</Tile>
        <Tile label="Session time">{formatDuration(stats.sessionTimeMs)}</Tile>
        <Tile label="Tokens" title={breakdownText}>
          {formatTokens(headlineTokens(stats.tokens))}
          <button
            type="button"
            aria-label="Token breakdown"
            aria-expanded={expanded}
            aria-controls={breakdownId}
            onClick={() => setExpanded((e) => !e)}
            className="ml-1 rounded p-0.5 align-middle text-muted-foreground hover:bg-muted"
          >
            <ChevronDown className={cn("size-4 transition-transform", expanded && "rotate-180")} />
          </button>
        </Tile>
      </dl>
      {expanded && (
        <ul id={breakdownId} aria-label="Token breakdown" className="grid grid-cols-2 gap-x-4 gap-y-1 text-sm sm:grid-cols-4">
          {BREAKDOWN.map(([key, label]) => (
            <li key={key} className="flex justify-between gap-2 rounded-md bg-muted px-2 py-1">
              <span className="text-muted-foreground">{label}</span>
              <span className="font-medium">{formatTokens(stats.tokens[key])}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/** A label under a big value (the label comes first in the DOM for screen readers). */
function Tile({ label, title, children }: { label: string; title?: string; children: ReactNode }) {
  return (
    <div className="flex flex-col-reverse rounded-lg border p-3" title={title}>
      <dt className="text-xs text-muted-foreground">{label}</dt>
      <dd className="text-lg font-semibold">{children}</dd>
    </div>
  );
}
