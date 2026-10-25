import { cn } from "@/lib/utils";

/** A Workspace's tags as plain chips. Tags have no colours (spec §6.2). */
export function TagList({ tags, className }: { tags: string[]; className?: string }) {
  if (tags.length === 0) return null;
  return (
    <ul aria-label="Tags" className={cn("flex flex-wrap gap-1", className)}>
      {tags.map((tag) => (
        <li key={tag} className="rounded-full bg-muted px-2 py-0.5 text-[11px] leading-4 text-muted-foreground">
          {tag}
        </li>
      ))}
    </ul>
  );
}
