import { useEffect, useId, useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { api, type Result } from "@/lib/api";
import { formatArgv, parseArgv } from "@/lib/argv";

const COMMAND = "{command}";

/** A line as a template: `null` when empty, else an argv with `{command}`. */
function toTemplate(line: string): Result<string[] | null, string> {
  const parsed = parseArgv(line);
  if (!parsed.ok) return parsed;
  const argv = parsed.value;
  if (argv.length === 0) return { ok: true, value: null };
  if (argv.some((a) => a !== COMMAND && a.includes(COMMAND))) {
    return { ok: false, error: `${COMMAND} must be an argument on its own` };
  }
  if (!argv.includes(COMMAND)) return { ok: false, error: `The template must contain ${COMMAND}` };
  return { ok: true, value: argv };
}

type TestState = { kind: "idle" } | { kind: "running" } | { kind: "launched" } | { kind: "failed"; message: string };

/**
 * The custom terminal argv template, edited as one line with shell-style
 * quoting and saved as an array when the field is left (spec §9.2).
 */
export function CustomTerminalSetting({
  value,
  warning,
  disabled,
  onChange,
}: {
  value: string[] | null | undefined;
  warning?: string;
  disabled: boolean;
  onChange: (value: string[] | null) => void;
}) {
  const labelId = useId();
  const hintId = useId();
  const saved = formatArgv(value ?? []);
  const [line, setLine] = useState(saved);
  const [touched, setTouched] = useState(false);
  const [test, setTest] = useState<TestState>({ kind: "idle" });

  // Follow the saved value when it changes elsewhere (a save, a hand edit).
  useEffect(() => {
    setLine(saved);
    setTouched(false);
  }, [saved]);

  const template = toTemplate(line);
  const error = touched && !template.ok ? template.error : undefined;

  const commit = () => {
    setTouched(true);
    if (template.ok && formatArgv(template.value ?? []) !== saved) onChange(template.value);
  };

  const runTest = async () => {
    if (!template.ok || template.value === null) return;
    setTest({ kind: "running" });
    const result = await api.testTerminal(template.value);
    setTest(result.ok ? { kind: "launched" } : { kind: "failed", message: result.error.message });
  };

  return (
    <div className="flex flex-col gap-1.5">
      <span id={labelId} className="text-sm font-medium">
        Custom terminal
      </span>
      <div className="flex gap-2">
        <Input
          aria-labelledby={labelId}
          aria-describedby={hintId}
          aria-invalid={error !== undefined}
          className="font-mono"
          placeholder="alacritty --working-directory {cwd} -e {command}"
          spellCheck={false}
          value={line}
          disabled={disabled}
          onChange={(e) => {
            setLine(e.target.value);
            setTest({ kind: "idle" });
          }}
          onBlur={commit}
          onKeyDown={(e) => {
            // Esc leaves Settings (unmounting without a blur), so save first.
            if (e.key === "Enter" || e.key === "Escape") commit();
          }}
        />
        <Button
          variant="outline"
          disabled={disabled || !template.ok || template.value === null || test.kind === "running"}
          onClick={() => void runTest()}
        >
          Test
        </Button>
      </div>
      <p id={hintId} className="text-xs text-muted-foreground">
        {"{cwd}"} becomes the Workspace folder; {COMMAND} becomes the Claude Code command. Quote arguments
        with spaces.
      </p>
      {error && <p className="text-xs text-destructive">{error}</p>}
      {!error && warning && <p className="text-xs text-destructive">{warning}</p>}
      {test.kind === "launched" && (
        <p role="status" className="text-xs text-muted-foreground">
          Launched claude --version in your home folder through the template.
        </p>
      )}
      {test.kind === "failed" && (
        <p role="alert" className="text-xs text-destructive">
          {test.message}
        </p>
      )}
    </div>
  );
}
