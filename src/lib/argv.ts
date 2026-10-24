// The custom terminal template is edited as one line and stored as an argv
// array (spec §9.2). Quoting is shell-like, but backslashes are always literal
// so Windows paths need no escaping: whitespace separates arguments, and
// '...' or "..." group text (including spaces and the other kind of quote).

import type { Result } from "@/lib/api";

export function parseArgv(line: string): Result<string[], string> {
  const argv: string[] = [];
  let current = "";
  let inArg = false;
  let quote: "'" | '"' | null = null;
  for (const c of line) {
    if (quote) {
      if (c === quote) quote = null;
      else current += c;
    } else if (c === "'" || c === '"') {
      quote = c;
      inArg = true;
    } else if (/\s/.test(c)) {
      if (inArg) argv.push(current);
      current = "";
      inArg = false;
    } else {
      current += c;
      inArg = true;
    }
  }
  if (quote) return { ok: false, error: "A quote is not closed" };
  if (inArg) argv.push(current);
  return { ok: true, value: argv };
}

export function formatArgv(argv: string[]): string {
  return argv.map(formatArg).join(" ");
}

function formatArg(arg: string): string {
  if (arg !== "" && !/[\s'"]/.test(arg)) return arg;
  if (!arg.includes('"')) return `"${arg}"`;
  if (!arg.includes("'")) return `'${arg}'`;
  // Both kinds: quote each run with the kind it does not contain.
  return arg
    .split(/(")/)
    .filter((part) => part !== "")
    .map((part) => (part === '"' ? `'"'` : `"${part}"`))
    .join("");
}
