import { cn } from "@/lib/utils";

/** Marks a Workspace or Suggestion whose directory is gone (spec §6.3, §6.6). */
export function MissingBadge({ className }: { className?: string }) {
  return (
    <span className={cn("shrink-0 rounded-full bg-brand-soft px-1.75 text-[11px] leading-4 font-semibold text-brand", className)}>
      Missing
    </span>
  );
}
