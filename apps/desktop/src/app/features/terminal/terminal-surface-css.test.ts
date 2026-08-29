// @ts-expect-error Vitest runs in Node; Node types are intentionally absent from the webview build.
import { readFileSync } from "node:fs";
import { expect, it } from "vitest";

const terminalSurfaceCss = readFileSync(
  "src/app/features/terminal/terminal-surface.css",
  "utf8",
);

it("keeps padding on xterm so FitAddon measures the useful viewport", () => {
  const viewportBlock = requireMatch(
    /\.terminal-surface__viewport\s*\{([^}]*)\}/,
    "terminal viewport CSS",
  );
  const xtermBlock = requireMatch(
    /\.terminal-surface__viewport\s*>\s*\.xterm\s*\{([^}]*)\}/,
    "xterm sizing CSS",
  );

  expect(viewportBlock).not.toMatch(/\bpadding\s*:/);
  expect(xtermBlock).toMatch(/box-sizing:\s*border-box/);
  expect(xtermBlock).toMatch(/padding:\s*var\(--space-2\)/);
});

function requireMatch(pattern: RegExp, label: string): string {
  const match = pattern.exec(terminalSurfaceCss);
  if (!match?.[1]) {
    throw new Error(`Expected ${label}.`);
  }
  return match[1];
}
