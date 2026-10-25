// Display formats for stats (spec §8.4): local times, `1h 23m` durations and
// abbreviated token counts. A value the Agent doesn't report is `null` and
// shows as a dash, never 0.

export const MISSING = "—";

/** `1h 23m`, or `23m` under an hour. */
export function formatDuration(ms: number | null): string {
  if (ms === null) return MISSING;
  const minutes = Math.floor(ms / 60_000);
  const hours = Math.floor(minutes / 60);
  return hours > 0 ? `${hours}h ${minutes % 60}m` : `${minutes}m`;
}

const UNITS = ["", "k", "M", "B"];

/** `999`, `12.3k`, `4.1M`. */
export function formatTokens(count: number | null): string {
  if (count === null) return MISSING;
  let value = count;
  let unit = 0;
  // Step up while one decimal would round to 1000 or more (999_960 is 1M, not 1000k).
  while (unit < UNITS.length - 1 && Math.abs(value) >= 999.95) {
    value /= 1_000;
    unit += 1;
  }
  return `${Number(value.toFixed(unit === 0 ? 0 : 1))}${UNITS[unit]}`;
}

function sameDay(a: Date, b: Date): boolean {
  return a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();
}

/** A local time: `Today 09:05`, `Yesterday 22:00`, `Sep 30` or `Dec 31, 2025`. */
export function formatWhen(epochMs: number | null, now: Date = new Date()): string {
  if (epochMs === null) return MISSING;
  const when = new Date(epochMs);
  const time = when.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
  if (sameDay(when, now)) return `Today ${time}`;
  const yesterday = new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1);
  if (sameDay(when, yesterday)) return `Yesterday ${time}`;
  return when.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: when.getFullYear() === now.getFullYear() ? undefined : "numeric",
  });
}
