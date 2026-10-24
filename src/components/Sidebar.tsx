import { Inbox, Plus } from "lucide-react";

import { Button } from "@/components/ui/button";

type SidebarProps = {
  onAddWorkspace: () => void;
  onOpenSuggestions: () => void;
};

export function Sidebar({ onAddWorkspace, onOpenSuggestions }: SidebarProps) {
  return (
    <aside className="flex w-72 shrink-0 flex-col gap-3 border-r bg-sidebar p-3 text-sidebar-foreground">
      <div className="flex items-center justify-between gap-2">
        <h1 className="text-base font-semibold">Code Orchestrate</h1>
        <Button size="sm" variant="outline" onClick={onAddWorkspace}>
          <Plus />
          Add Workspace
        </Button>
      </div>
      <Button variant="ghost" className="justify-start" onClick={onOpenSuggestions}>
        <Inbox />
        Suggestions
      </Button>
    </aside>
  );
}
