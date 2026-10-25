import { useEffect, useId, useRef, useState, type ReactNode } from "react";

import { Input } from "@/components/ui/input";
import { api, type ClaudeCodeStatus, type SETTING, type SettingValues } from "@/lib/api";
import { cn } from "@/lib/utils";

type Row<K extends keyof SettingValues> = {
  value: SettingValues[K] | undefined;
  warning?: string;
  disabled: boolean;
  onChange: (value: SettingValues[K]) => void;
};

/**
 * Settings › Agents › Claude Code: the executable and transcript root
 * overrides, each with a status line (spec §9.1, §9.3). The executable is
 * re-probed when the page opens; an override change re-checks both.
 */
export function ClaudeCodeSettings({
  executable,
  transcriptRoot,
}: {
  executable: Row<typeof SETTING.executable>;
  transcriptRoot: Row<typeof SETTING.transcriptRoot>;
}) {
  const [status, setStatus] = useState<ClaudeCodeStatus | null>(null);
  const [statusError, setStatusError] = useState<string | null>(null);
  const opened = useRef(false);
  const requests = useRef(0);

  useEffect(() => {
    // The first check (Settings opening) re-probes; later ones reuse the
    // cached probe unless the executable override changed.
    const refresh = !opened.current;
    opened.current = true;
    const id = ++requests.current;
    api.claudeCodeStatus(refresh).then(
      (next) => {
        if (id !== requests.current) return;
        setStatus(next);
        setStatusError(null);
      },
      (e: unknown) => {
        if (id !== requests.current) return;
        setStatusError(e instanceof Error ? e.message : String(e));
      },
    );
  }, [executable.value, transcriptRoot.value]);

  const exe = status?.executable;
  const root = status?.transcriptRoot;
  return (
    <>
      <PathSetting label="Executable path" placeholder="claude on PATH" {...executable}>
        {statusError !== null ? (
          <Status bad>{statusError}</Status>
        ) : exe?.state === "found" ? (
          <Status>
            {exe.path}
            {exe.version ? ` · ${exe.version}` : ""}
          </Status>
        ) : exe?.state === "notFound" ? (
          <Status bad>Not found</Status>
        ) : null}
      </PathSetting>
      <PathSetting label="Transcript root" placeholder="CLAUDE_CONFIG_DIR, else ~/.claude" {...transcriptRoot}>
        {root?.state === "ok" ? (
          <Status>{root.path}</Status>
        ) : root?.state === "noProjects" ? (
          <Status bad>Transcript root has no projects folder</Status>
        ) : root?.state === "notFound" ? (
          <Status bad>Not found</Status>
        ) : null}
      </PathSetting>
    </>
  );
}

function Status({ bad = false, children }: { bad?: boolean; children: ReactNode }) {
  return <p className={cn("text-xs break-all", bad ? "text-destructive" : "text-muted-foreground")}>{children}</p>;
}

/** A path typed on one line, saved when the field is left or on Enter. Blank means the default. */
function PathSetting({
  label,
  placeholder,
  value,
  warning,
  disabled,
  onChange,
  children,
}: {
  label: string;
  placeholder: string;
  value: string | undefined;
  warning?: string;
  disabled: boolean;
  onChange: (value: string) => void;
  children?: ReactNode;
}) {
  const labelId = useId();
  const saved = value ?? "";
  const [text, setText] = useState(saved);

  // Follow the saved value when it changes elsewhere (a save, a hand edit).
  useEffect(() => setText(saved), [saved]);

  const commit = () => {
    if (text !== saved) onChange(text);
  };

  return (
    <div className="flex flex-col gap-1.5">
      <span id={labelId} className="text-sm font-medium">
        {label}
      </span>
      <Input
        aria-labelledby={labelId}
        className="font-mono"
        placeholder={placeholder}
        value={text}
        disabled={disabled}
        spellCheck={false}
        onChange={(e) => setText(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          // Esc leaves Settings (unmounting without a blur), so save first.
          if (e.key === "Enter" || e.key === "Escape") commit();
        }}
      />
      {children}
      {warning && <p className="text-xs text-destructive">{warning}</p>}
    </div>
  );
}
