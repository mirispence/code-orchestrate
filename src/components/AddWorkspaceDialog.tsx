import { useState } from "react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { api, lastSegment, type AddWorkspaceError, type Workspace } from "@/lib/api";

type AddWorkspaceDialogProps = {
  /** The picked folder; the dialog is open while this is set. */
  path: string | null;
  onClose: () => void;
  onAdded: (workspace: Workspace) => void;
  /** Jump to the Workspace that already owns the path. */
  onShowWorkspace: (id: number) => void;
};

/** Name dialog shown after the folder picker (spec §8.1, §6.2). */
export function AddWorkspaceDialog({ path, onClose, onAdded, onShowWorkspace }: AddWorkspaceDialogProps) {
  return (
    <Dialog open={path !== null} onOpenChange={(open) => !open && onClose()}>
      <DialogContent>
        {path !== null && (
          // Keyed so each picked folder starts with a fresh form.
          <AddWorkspaceForm
            key={path}
            path={path}
            onAdded={onAdded}
            onShowWorkspace={onShowWorkspace}
          />
        )}
      </DialogContent>
    </Dialog>
  );
}

function AddWorkspaceForm({
  path,
  onAdded,
  onShowWorkspace,
}: { path: string } & Pick<AddWorkspaceDialogProps, "onAdded" | "onShowWorkspace">) {
  const [name, setName] = useState(() => lastSegment(path));
  const [error, setError] = useState<AddWorkspaceError | null>(null);
  const [saving, setSaving] = useState(false);

  async function save(event: React.FormEvent) {
    event.preventDefault();
    setSaving(true);
    const result = await api.addWorkspace(name.trim(), path);
    setSaving(false);
    if (result.ok) onAdded(result.value);
    else setError(result.error);
  }

  return (
    <form onSubmit={save} className="grid gap-4">
      <DialogHeader>
        <DialogTitle>Add Workspace</DialogTitle>
        <DialogDescription className="break-all">{path}</DialogDescription>
      </DialogHeader>
      <div className="grid gap-2">
        <Label htmlFor="workspace-name">Name</Label>
        <Input
          id="workspace-name"
          value={name}
          autoFocus
          onChange={(e) => {
            setName(e.target.value);
            setError(null);
          }}
        />
      </div>
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error.kind === "pathTaken" ? (
            <>
              Already a Workspace:{" "}
              <Button
                type="button"
                variant="link"
                className="h-auto p-0 text-destructive underline"
                onClick={() => onShowWorkspace(error.workspaceId)}
              >
                {error.workspaceName}
              </Button>
            </>
          ) : (
            error.message
          )}
        </p>
      )}
      <DialogFooter>
        <Button type="submit" disabled={saving || name.trim() === ""}>
          Save
        </Button>
      </DialogFooter>
    </form>
  );
}
