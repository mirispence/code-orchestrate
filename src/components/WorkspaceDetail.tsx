import { Star } from "lucide-react";

import { TagList } from "@/components/TagList";
import { Button } from "@/components/ui/button";
import type { Workspace } from "@/lib/api";
import { cn } from "@/lib/utils";

type WorkspaceDetailProps = {
  workspace: Workspace;
  onEdit: () => void;
  onTogglePin: () => void;
};

/** Detail pane for the selected Workspace (spec §8.2). Later tickets add actions and stats. */
export function WorkspaceDetail({ workspace, onEdit, onTogglePin }: WorkspaceDetailProps) {
  return (
    <div className="flex flex-col gap-4 p-6">
      <WorkspaceHeader workspace={workspace} onEdit={onEdit} onTogglePin={onTogglePin} />
    </div>
  );
}

function WorkspaceHeader({ workspace, onEdit, onTogglePin }: WorkspaceDetailProps) {
  return (
    <header className="flex flex-col gap-2">
      <div className="flex items-start justify-between gap-4">
        <div className="flex min-w-0 flex-col gap-1">
          <h2 className="text-xl font-semibold break-words">{workspace.name}</h2>
          <p className="font-mono text-sm break-all text-muted-foreground">{workspace.path}</p>
        </div>
        <div className="flex shrink-0 gap-1">
          <Button size="sm" variant="ghost" onClick={onTogglePin}>
            <Star className={cn(workspace.pinned && "fill-brand text-brand")} />
            {workspace.pinned ? "Unpin" : "Pin"}
          </Button>
          <Button size="sm" variant="ghost" onClick={onEdit}>
            Edit
          </Button>
        </div>
      </div>
      <TagList tags={workspace.tags} />
      {workspace.note && <p className="text-sm whitespace-pre-wrap">{workspace.note}</p>}
    </header>
  );
}
