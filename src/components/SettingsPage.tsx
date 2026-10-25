import { useEffect, useId, useState, type ReactNode } from "react";

import { ClaudeCodeSettings } from "@/components/ClaudeCodeSettings";
import { CustomTerminalSetting } from "@/components/CustomTerminalSetting";
import { DismissedPaths } from "@/components/DismissedPaths";
import { Button } from "@/components/ui/button";
import { api, type SettingKey, type SettingValues } from "@/lib/api";
import type { SettingsState } from "@/lib/useSettings";
import { cn } from "@/lib/utils";

type SettingsPageProps = {
  settings: SettingsState;
  /** Leave the page (Esc). */
  onClose: () => void;
  /** A dismissed path was restored, so Suggestions may have changed. */
  onRestored?: () => void;
};

/**
 * Settings in the detail pane: one scrolling page, autosaved (spec §9.1).
 * To add a setting, add a row to a `SettingsSection` (or a new section).
 */
export function SettingsPage({ settings, onClose, onRestored }: SettingsPageProps) {
  const { snapshot, error, change, reset } = settings;
  const [dataError, setDataError] = useState<string | null>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !e.defaultPrevented) onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  const locked = snapshot === null || snapshot.fileError !== null;
  const row = <K extends SettingKey>(key: K) => ({
    value: snapshot?.values[key],
    warning: snapshot?.warnings[key],
    disabled: locked,
    onChange: (value: SettingValues[K]) => void change(key, value),
  });
  /** Only the options this platform offers (e.g. Custom alone off Windows). */
  const offered = <K extends SettingKey>(key: K, options: { value: SettingValues[K] & string; label: string }[]) => {
    const choices = snapshot?.choices[key];
    return choices ? options.filter((o) => choices.includes(o.value)) : options;
  };
  const preset = snapshot?.values["terminal.preset"];

  return (
    <div className="flex h-full flex-col gap-6 overflow-y-auto p-6">
      <h2 className="text-xl font-semibold">Settings</h2>

      {snapshot?.fileError && (
        <div
          role="alert"
          className="flex flex-col gap-2 rounded-md border border-destructive/50 p-3 text-sm text-destructive"
        >
          <p>
            settings.json could not be read, so the defaults are in use. It will not be changed until
            you fix it or reset it.
          </p>
          <p className="font-mono text-xs">{snapshot.fileError}</p>
          <div>
            <Button size="sm" variant="outline" onClick={() => void reset()}>
              Reset to defaults
            </Button>
          </div>
        </div>
      )}
      {error !== null && (
        <div role="alert" className="rounded-md border border-destructive/50 p-3 text-sm text-destructive">
          Could not save settings: {error}
        </div>
      )}

      <SettingsSection title="Appearance">
        <ChoiceSetting
          label="Theme"
          options={[
            { value: "system", label: "System" },
            { value: "light", label: "Light" },
            { value: "dark", label: "Dark" },
          ]}
          {...row("theme")}
        />
      </SettingsSection>

      <SettingsSection title="Terminal">
        <ChoiceSetting
          label="Preset"
          options={offered("terminal.preset", [
            { value: "windowsTerminal", label: "Windows Terminal" },
            { value: "console", label: "Console window" },
            { value: "custom", label: "Custom" },
          ])}
          {...row("terminal.preset")}
        />
        {preset === "windowsTerminal" && (
          <ChoiceSetting
            label="Open in"
            options={[
              { value: "newWindow", label: "New window" },
              { value: "newTab", label: "New tab in most recent window" },
            ]}
            {...row("terminal.windowsTerminal.target")}
          />
        )}
        {preset === "custom" && <CustomTerminalSetting {...row("terminal.custom")} />}
      </SettingsSection>

      <SettingsSection title="Agents › Claude Code">
        <ClaudeCodeSettings
          executable={row("agents.claude-code.executable")}
          transcriptRoot={row("agents.claude-code.transcriptRoot")}
        />
      </SettingsSection>

      <SettingsSection title="Suggestions">
        <DismissedPaths onRestored={onRestored} />
      </SettingsSection>

      <SettingsSection title="Data">
        <div className="flex flex-wrap gap-2">
          <Button variant="outline" onClick={() => void api.rescanTranscripts()}>
            Rescan transcripts
          </Button>
          <Button
            variant="outline"
            onClick={() =>
              void api.openDataFolder().then((r) => setDataError(r.ok ? null : r.error.message))
            }
          >
            Open data folder
          </Button>
        </div>
        {dataError !== null && <p className="text-xs text-destructive">{dataError}</p>}
      </SettingsSection>
    </div>
  );
}

export function SettingsSection({ title, children }: { title: string; children: ReactNode }) {
  const id = useId();
  return (
    <section aria-labelledby={id} className="flex flex-col gap-4">
      <h3 id={id} className="border-b pb-1 text-sm font-semibold text-muted-foreground uppercase">
        {title}
      </h3>
      {children}
    </section>
  );
}

/** A labelled row with an optional inline warning under it. */
export function SettingRow({
  label,
  labelId,
  warning,
  children,
}: {
  label: string;
  labelId: string;
  warning?: string;
  children: ReactNode;
}) {
  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <span id={labelId} className="text-sm font-medium">
          {label}
        </span>
        {children}
      </div>
      {warning && <p className="text-xs text-destructive">{warning}</p>}
    </div>
  );
}

/** A segmented single choice, e.g. System / Light / Dark. */
export function ChoiceSetting<V extends string>({
  label,
  options,
  value,
  warning,
  disabled,
  onChange,
}: {
  label: string;
  options: { value: V; label: string }[];
  value: V | undefined;
  warning?: string;
  disabled: boolean;
  onChange: (value: V) => void;
}) {
  const labelId = useId();
  return (
    <SettingRow label={label} labelId={labelId} warning={warning}>
      <div role="radiogroup" aria-labelledby={labelId} className="inline-flex rounded-md border p-0.5">
        {options.map((option) => {
          const checked = option.value === value;
          return (
            <button
              key={option.value}
              type="button"
              role="radio"
              aria-checked={checked}
              disabled={disabled}
              onClick={() => {
                if (!checked) onChange(option.value);
              }}
              className={cn(
                "rounded-sm px-3 py-1 text-sm disabled:opacity-50",
                checked ? "bg-accent font-medium text-accent-foreground" : "hover:bg-accent/60",
              )}
            >
              {option.label}
            </button>
          );
        })}
      </div>
    </SettingRow>
  );
}
