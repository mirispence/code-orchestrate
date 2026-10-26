import { X } from "lucide-react";

import { Button } from "@/components/ui/button";

type ErrorToastProps = {
  message: string;
  /** Optional, with `onAction`: one action button. */
  actionLabel?: string;
  onAction?: () => void;
  onDismiss: () => void;
};

/** A persistent error toast in the bottom-right corner, with at most one action. */
export function ErrorToast({ message, actionLabel, onAction, onDismiss }: ErrorToastProps) {
  return (
    <div
      role="alert"
      className="fixed right-4 bottom-4 z-50 flex max-w-md items-start gap-3 rounded-lg border border-destructive/50 bg-popover p-3 text-sm text-popover-foreground shadow-lg"
    >
      <p className="min-w-0 flex-1 break-words text-destructive">{message}</p>
      {actionLabel !== undefined && onAction !== undefined && (
        <Button size="sm" variant="outline" onClick={onAction}>
          {actionLabel}
        </Button>
      )}
      <Button size="icon" variant="ghost" className="size-8" aria-label="Dismiss" onClick={onDismiss}>
        <X />
      </Button>
    </div>
  );
}
