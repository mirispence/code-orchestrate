import { X } from "lucide-react";
import { useId, useState } from "react";

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
import { api, type Workspace, type WorkspaceError } from "@/lib/api";
import { addTag, tagKey } from "@/lib/tags";

type EditWorkspaceDialogProps = {
  /** The Workspace being edited; the dialog is open while this is set. */
  workspace: Workspace | null;
  /** Tags any Workspace uses, for autocomplete. */
  tagsInUse: string[];
  onClose: () => void;
  onSaved: (workspace: Workspace) => void;
  onRemoved: (id: number) => void;
};

/** Edit dialog: name, note, tags and Remove Workspace (spec §8.2, §6.2). */
export function EditWorkspaceDialog({ workspace, onClose, ...rest }: EditWorkspaceDialogProps) {
  return (
    <Dialog open={workspace !== null} onOpenChange={(open) => !open && onClose()}>
      {/* Bounded to the window so many tags never push Save off-screen. */}
      <DialogContent className="max-h-[calc(100vh-2rem)] overflow-y-auto">
        {workspace !== null && (
          // Keyed so each opening starts from the saved values.
          <EditWorkspaceForm key={workspace.id} workspace={workspace} {...rest} />
        )}
      </DialogContent>
    </Dialog>
  );
}

function EditWorkspaceForm({
  workspace,
  tagsInUse,
  onSaved,
  onRemoved,
}: { workspace: Workspace } & Omit<EditWorkspaceDialogProps, "workspace" | "onClose">) {
  const [confirmingRemove, setConfirmingRemove] = useState(false);
  const [name, setName] = useState(workspace.name);
  const [note, setNote] = useState(workspace.note ?? "");
  const [tags, setTags] = useState(workspace.tags);
  const [draftTag, setDraftTag] = useState("");
  const [error, setError] = useState<WorkspaceError | null>(null);
  const [saving, setSaving] = useState(false);
  const suggestionsId = useId();

  const suggestions = tagsInUse.filter((t) => !tags.some((own) => tagKey(own) === tagKey(t)));

  function commitDraft() {
    setTags((current) => addTag(current, draftTag));
    setDraftTag("");
  }

  async function save(event: React.FormEvent) {
    event.preventDefault();
    setSaving(true);
    const result = await api.updateWorkspace(workspace.id, {
      name: name.trim(),
      note: note.trim(),
      tags: addTag(tags, draftTag),
    });
    setSaving(false);
    if (result.ok) onSaved(result.value);
    else setError(result.error);
  }

  async function remove() {
    setSaving(true);
    const result = await api.removeWorkspace(workspace.id);
    setSaving(false);
    if (result.ok) onRemoved(workspace.id);
    else {
      setConfirmingRemove(false);
      setError(result.error);
    }
  }

  if (confirmingRemove) {
    return (
      <div className="grid gap-4">
        <DialogHeader>
          <DialogTitle>Edit Workspace</DialogTitle>
          <DialogDescription>
            Remove <strong className="text-foreground">{workspace.name}</strong>? Its tags go with it. The folder
            and its Sessions are not touched.
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button type="button" variant="outline" onClick={() => setConfirmingRemove(false)}>
            Keep
          </Button>
          <Button type="button" variant="destructive" disabled={saving} onClick={() => void remove()}>
            Remove
          </Button>
        </DialogFooter>
      </div>
    );
  }

  return (
    <form onSubmit={save} className="grid gap-4">
      <DialogHeader>
        <DialogTitle>Edit Workspace</DialogTitle>
        <DialogDescription className="break-all">{workspace.path}</DialogDescription>
      </DialogHeader>
      <div className="grid gap-2">
        <Label htmlFor="edit-workspace-name">Name</Label>
        <Input
          id="edit-workspace-name"
          value={name}
          autoFocus
          onChange={(e) => {
            setName(e.target.value);
            setError(null);
          }}
        />
      </div>
      <div className="grid gap-2">
        <Label htmlFor="edit-workspace-tag">Tags</Label>
        {tags.length > 0 && (
          <ul className="flex flex-wrap gap-1">
            {tags.map((tag) => (
              <li
                key={tagKey(tag)}
                className="flex items-center gap-0.5 rounded-full bg-muted py-0.5 pr-1 pl-2 text-xs text-muted-foreground"
              >
                {tag}
                <button
                  type="button"
                  aria-label={`Remove tag ${tag}`}
                  className="rounded-full p-0.5 hover:bg-background"
                  onClick={() => setTags((current) => current.filter((t) => t !== tag))}
                >
                  <X className="size-3" />
                </button>
              </li>
            ))}
          </ul>
        )}
        <Input
          id="edit-workspace-tag"
          aria-label="Add tag"
          placeholder="Add a tag and press Enter"
          list={suggestionsId}
          value={draftTag}
          onChange={(e) => setDraftTag(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === ",") {
              e.preventDefault();
              commitDraft();
            }
          }}
        />
        <datalist id={suggestionsId}>
          {suggestions.map((tag) => (
            <option key={tag} value={tag} />
          ))}
        </datalist>
      </div>
      <div className="grid gap-2">
        <Label htmlFor="edit-workspace-note">Note</Label>
        <textarea
          id="edit-workspace-note"
          rows={3}
          value={note}
          onChange={(e) => setNote(e.target.value)}
          className="w-full min-w-0 rounded-md border border-input bg-transparent px-3 py-2 text-sm shadow-xs outline-none focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 dark:bg-input/30"
        />
      </div>
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error.message}
        </p>
      )}
      <DialogFooter className="sm:justify-between">
        <Button type="button" variant="ghost" className="text-destructive" onClick={() => setConfirmingRemove(true)}>
          Remove Workspace
        </Button>
        <Button type="submit" disabled={saving || name.trim() === ""}>
          Save
        </Button>
      </DialogFooter>
    </form>
  );
}
