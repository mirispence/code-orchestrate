import { useState } from "react";

import { MissingBadge } from "@/components/MissingBadge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { lastSegment, type Result, type Suggestion } from "@/lib/api";

type SuggestionsInboxProps = {
  suggestions: Suggestion[];
  /** Create a Workspace at the Suggestion's path. */
  onAdd: (name: string, path: string) => Promise<Result<unknown, { message: string }>>;
  onDismiss: (path: string) => Promise<Result<unknown, { message: string }>>;
};

/** Directories with Sessions that no Workspace covers (spec §8.3). */
export function SuggestionsInbox({ suggestions, onAdd, onDismiss }: SuggestionsInboxProps) {
  return (
    <div className="flex flex-col gap-4 p-6">
      <div>
        <h2 className="text-lg font-semibold">Suggestions</h2>
        <p className="text-sm text-muted-foreground">
          Folders where Sessions ran that are not Workspaces yet.
        </p>
      </div>
      {suggestions.length === 0 ? (
        <p className="text-sm text-muted-foreground">No Suggestions right now.</p>
      ) : (
        <ul className="flex flex-col divide-y rounded-md border">
          {suggestions.map((s) => (
            // Keyed by path so each row keeps its own typed name.
            <SuggestionRow key={s.path} suggestion={s} onAdd={onAdd} onDismiss={onDismiss} />
          ))}
        </ul>
      )}
    </div>
  );
}

function SuggestionRow({
  suggestion,
  onAdd,
  onDismiss,
}: { suggestion: Suggestion } & Pick<SuggestionsInboxProps, "onAdd" | "onDismiss">) {
  const { path, sessionCount, lastSeen, missing } = suggestion;
  const [name, setName] = useState(() => lastSegment(path));
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function act(run: () => Promise<Result<unknown, { message: string }>>) {
    setBusy(true);
    const result = await run();
    setBusy(false);
    setError(result.ok ? null : result.error.message);
  }

  return (
    <li aria-label={path} className="flex flex-col gap-2 p-3">
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-medium break-all">{path}</span>
        {missing && (
          <MissingBadge />
        )}
      </div>
      <div className="flex gap-3 text-xs text-muted-foreground">
        <span>{sessionCount === 1 ? "1 Session" : `${sessionCount} Sessions`}</span>
        {lastSeen !== null && <span>Last seen {new Date(lastSeen).toLocaleString()}</span>}
      </div>
      <form
        className="flex items-center gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          void act(() => onAdd(name.trim(), path));
        }}
      >
        {!missing && (
          <>
            <Input
              aria-label="Name"
              value={name}
              className="h-8 max-w-64"
              onChange={(e) => {
                setName(e.target.value);
                setError(null);
              }}
            />
            <Button type="submit" size="sm" disabled={busy || name.trim() === ""}>
              Add
            </Button>
          </>
        )}
        <Button type="button" size="sm" variant="outline" disabled={busy} onClick={() => void act(() => onDismiss(path))}>
          Dismiss
        </Button>
      </form>
      {error !== null && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}
    </li>
  );
}
