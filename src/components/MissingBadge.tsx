import { cn } from "@/lib/utils";

/** Marks a Workspace or Suggestion whose directory is gone (spec §6.3, §6.6). */
export function MissingBadge({ className }: { className?: string }) {
  return (
    <span className={cn("shrink-0 rounded bg-brand/15 px-1.5 py-0.5 text-xs font-medium text-brand", className)}>
      Missing
    </span>
  );
}
