/// <reference types="node" />
import { readFileSync } from "node:fs";
import { join } from "node:path";
import process from "node:process";

// Read from disk: Vitest stubs CSS imports, even with `?raw`.
const css = readFileSync(join(process.cwd(), "src", "index.css"), "utf8");

/** The component sources, to check every colour class they use has a theme token. */
const sources = import.meta.glob<string>("./**/*.tsx", { query: "?raw", import: "default", eager: true });

/** Custom properties declared in a CSS block. */
function declaredIn(block: string): Set<string> {
  return new Set([...block.matchAll(/(--[\w-]+)\s*:/g)].map((m) => m[1]));
}

function blockAfter(marker: string): string {
  const start = css.indexOf(marker);
  if (start < 0) throw new Error(`no ${marker} in index.css`);
  const open = css.indexOf("{", start);
  let depth = 0;
  for (let i = open; i < css.length; i++) {
    if (css[i] === "{") depth++;
    else if (css[i] === "}" && --depth === 0) return css.slice(open + 1, i);
  }
  throw new Error(`unclosed ${marker}`);
}

const light = declaredIn(blockAfter(":root {"));
const dark = declaredIn(blockAfter("@media (prefers-color-scheme: dark)"));
const themeTokens = declaredIn(blockAfter("@theme inline"));

/** Utility suffixes that are not colours (sizes, sides, alignment, styles). */
const NOT_COLOURS = new Set([
  "xs", "sm", "md", "base", "lg", "xl", "2xl", "3xl", "left", "right", "center", "t", "b", "l", "r", "x", "y",
  "0", "2", "none", "hidden", "dashed", "solid", "collapse", "transparent", "current", "inherit", "white", "black",
  "ellipsis", "wrap", "nowrap", "balance", "pretty", "clip", "offset-background", "offset-2", "start", "end",
]);

describe("theme", () => {
  it("gives light and dark the same colour variables", () => {
    const colours = (s: Set<string>) => [...s].filter((v) => v !== "--radius").sort();
    expect(colours(dark)).toEqual(colours(light));
  });

  it("defines a token for every theme colour class the components use", () => {
    const used = new Set<string>();
    for (const [file, text] of Object.entries(sources)) {
      if (file.endsWith(".test.tsx")) continue;
      for (const m of text.matchAll(/(?<![\w-])(?:bg|text|border|ring|fill|outline|divide|shadow)-([a-z][\w-]*?)(?:\/\d+)?(?=["'`\s])/g)) {
        if (!NOT_COLOURS.has(m[1]) && !/^\[|^\d/.test(m[1])) used.add(m[1]);
      }
    }
    const missing = [...used].filter((name) => !themeTokens.has(`--color-${name}`) && !themeTokens.has(`--shadow-${name}`));
    expect(missing).toEqual([]);
  });

  it("uses the Segoe UI / system font stack", () => {
    expect(blockAfter("@theme inline")).toMatch(/--font-sans:\s*"Segoe UI Variable", "Segoe UI", system-ui/);
  });
});
