import { formatDuration, formatTokens, formatWhen } from "./format";

describe("formatDuration", () => {
  it.each([
    [0, "0m"],
    [59_000, "0m"],
    [23 * 60_000, "23m"],
    [83 * 60_000, "1h 23m"],
    [2 * 3_600_000, "2h 0m"],
    [26 * 3_600_000 + 5 * 60_000, "26h 5m"],
  ])("%d ms is %s", (ms, text) => {
    expect(formatDuration(ms)).toBe(text);
  });

  it("shows a missing duration as a dash", () => {
    expect(formatDuration(null)).toBe("—");
  });
});

describe("formatTokens", () => {
  it.each([
    [0, "0"],
    [999, "999"],
    [1_000, "1k"],
    [12_345, "12.3k"],
    [640_000, "640k"],
    [999_960, "1M"],
    [4_100_000, "4.1M"],
    [2_500_000_000, "2.5B"],
  ])("%d is %s", (n, text) => {
    expect(formatTokens(n)).toBe(text);
  });

  it("shows an unreported count as a dash, never 0", () => {
    expect(formatTokens(null)).toBe("—");
  });
});

describe("formatWhen", () => {
  // Local-time constructors keep these tests independent of the time zone.
  const now = new Date(2026, 9, 24, 15, 30);

  it("shows today's times as a local clock time", () => {
    expect(formatWhen(new Date(2026, 9, 24, 9, 5).getTime(), now)).toMatch(/^Today,? 0?9:05/);
  });

  it("shows yesterday by name", () => {
    expect(formatWhen(new Date(2026, 9, 23, 22, 0).getTime(), now)).toMatch(/^Yesterday/);
  });

  it("shows an older date this year without the year", () => {
    const text = formatWhen(new Date(2026, 8, 30, 12, 0).getTime(), now);
    expect(text).toMatch(/Sep/);
    expect(text).not.toMatch(/2026/);
  });

  it("shows a date in another year with its year", () => {
    expect(formatWhen(new Date(2025, 11, 31, 12, 0).getTime(), now)).toMatch(/2025/);
  });

  it("shows never-used as a dash", () => {
    expect(formatWhen(null, now)).toBe("—");
  });
});
