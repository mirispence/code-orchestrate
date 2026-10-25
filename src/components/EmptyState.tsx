import { Button } from "@/components/ui/button";

type EmptyStateProps = {
  onAddWorkspace: () => void;
  onOpenSuggestions: () => void;
};

/** Detail pane with nothing selected and no Workspaces (spec §8.1). */
export function EmptyState({ onAddWorkspace, onOpenSuggestions }: EmptyStateProps) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-4 p-8 text-center">
      <h2 className="text-2xl font-semibold tracking-tight">No Workspaces yet</h2>
      <p className="max-w-sm text-muted-foreground">
        Add a project folder as a Workspace, or check Suggestions for folders where you have
        already run Sessions.
      </p>
      <div className="flex gap-2">
        <Button className="bg-brand text-brand-foreground hover:bg-brand/90" onClick={onAddWorkspace}>
          Add Workspace
        </Button>
        <Button variant="outline" onClick={onOpenSuggestions}>
          Open Suggestions
        </Button>
      </div>
    </div>
  );
}
