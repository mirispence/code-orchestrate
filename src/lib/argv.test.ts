import { formatArgv, parseArgv } from "./argv";

describe("parseArgv", () => {
  it("splits on whitespace", () => {
    expect(parseArgv("  alacritty -e   {command} ")).toEqual({ ok: true, value: ["alacritty", "-e", "{command}"] });
  });

  it("keeps a quoted argument with spaces together", () => {
    expect(parseArgv('"C:\\Program Files\\Term\\term.exe" --cwd {cwd} {command}')).toEqual({
      ok: true,
      value: ["C:\\Program Files\\Term\\term.exe", "--cwd", "{cwd}", "{command}"],
    });
  });

  it("treats backslashes literally, so Windows paths need no escaping", () => {
    expect(parseArgv("C:\\tools\\term.exe {command}")).toEqual({ ok: true, value: ["C:\\tools\\term.exe", "{command}"] });
  });

  it("accepts single quotes, empty quotes and adjacent quoted parts", () => {
    expect(parseArgv(`say 'he said "hi"' "" --title="My term"`)).toEqual({
      ok: true,
      value: ["say", 'he said "hi"', "", "--title=My term"],
    });
  });

  it("rejects an unclosed quote", () => {
    expect(parseArgv('term "oops {command}')).toEqual({ ok: false, error: "A quote is not closed" });
  });
});

describe("formatArgv", () => {
  it("quotes only the arguments that need it, and round-trips", () => {
    const argv = ["C:\\Program Files\\t.exe", "-e", "{command}", "", 'say "hi"'];
    const line = formatArgv(argv);
    expect(line).toBe(`"C:\\Program Files\\t.exe" -e {command} "" 'say "hi"'`);
    expect(parseArgv(line)).toEqual({ ok: true, value: argv });
  });

  it("round-trips an argument with both kinds of quote", () => {
    const argv = [`it's "x"`];
    expect(parseArgv(formatArgv(argv))).toEqual({ ok: true, value: argv });
  });
});
