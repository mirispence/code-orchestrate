import { Button } from "@/components/ui/button";
import type { AddWorkspaceError } from "@/lib/api";

/** A rejected path's message; a taken path links to the Workspace that has it (spec §6.2). */
export function AddWorkspaceErrorText({
  error,
  onShowWorkspace,
}: {
  error: AddWorkspaceError;
  onShowWorkspace: (id: number) => void;
}) {
  if (error.kind !== "pathTaken") return <>{error.message}</>;
  return (
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
  );
}
