import { expect, test, type Locator, type Page } from "@playwright/test";

test.use({
  viewport: { width: 1440, height: 900 },
  contextOptions: { reducedMotion: "reduce" },
});

test.beforeEach(async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("article")).toHaveCount(3);
});

test("duplicates and removes a pointer-selected group while preserving notes across reloads", async ({ page }) => {
  const terminal = canvasCard(page, "Terminal 1", "terminal");
  const note = canvasCard(page, "Notes", "note");
  await note.getByRole("textbox", { name: "Notes content" }).fill("Release checklist: test Linux and macOS.");
  await page.getByLabel("Move Terminal 1", { exact: true }).click();
  await page.getByLabel("Move Notes", { exact: true }).click({ modifiers: ["Shift"] });
  await expect(terminal).toHaveAccessibleDescription("Selected canvas item");
  await expect(note).toHaveAccessibleDescription("Selected canvas item");
  await expect(page.getByRole("status").filter({ hasText: /^2 selected/ })).toBeVisible();

  await page.getByRole("button", { name: "Duplicate selected canvas items" }).click();
  await expect(page.getByRole("article")).toHaveCount(5);
  await expect(canvasCard(page, "Terminal 1 copy", "terminal")).toHaveAccessibleDescription("Selected canvas item");
  await expect(canvasCard(page, "Notes copy", "note").getByRole("textbox")).toHaveValue("Release checklist: test Linux and macOS.");
  await page.reload();
  await expect(page.getByRole("article")).toHaveCount(5);
  await expect(canvasCard(page, "Notes copy", "note").getByRole("textbox")).toHaveValue("Release checklist: test Linux and macOS.");

  await canvasCard(page, "Terminal 1 copy", "terminal").focus();
  await canvasCard(page, "Notes copy", "note").focus();
  await page.keyboard.press("Shift+Space");
  await expect(page.getByRole("status").filter({ hasText: /^2 selected/ })).toBeVisible();
  await page.getByRole("button", { name: "Remove selected items from canvas", exact: true }).click();
  await expect(page.getByRole("article")).toHaveCount(3);
  await page.reload();
  await expect(canvasCard(page, "Terminal 1 copy", "terminal")).toHaveCount(0);
  await expect(canvasCard(page, "Notes copy", "note")).toHaveCount(0);
  await expect(note.getByRole("textbox")).toHaveValue("Release checklist: test Linux and macOS.");
  await expect(terminal).toBeVisible();
});

test("moves a keyboard-selected group with arrows and pointer drag without changing its spacing", async ({ page }) => {
  const first = canvasCard(page, "Terminal 1", "terminal");
  const second = canvasCard(page, "Terminal 2", "terminal");
  const note = canvasCard(page, "Notes", "note");
  await first.focus();
  await second.focus();
  await page.keyboard.press("Shift+Space");
  await expect(first).toHaveAccessibleDescription("Selected canvas item");
  await expect(second).toHaveAccessibleDescription("Selected canvas item");

  const initial = await positions([first, second, note]);
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("Alt+ArrowDown");
  await expect.poll(() => positions([first, second, note])).toEqual([
    { x: initial[0]!.x + 8, y: initial[0]!.y + 1 },
    { x: initial[1]!.x + 8, y: initial[1]!.y + 1 },
    initial[2],
  ]);

  const header = page.getByLabel("Move Terminal 2", { exact: true });
  await header.scrollIntoViewIfNeeded();
  const bounds = await header.boundingBox();
  expect(bounds).not.toBeNull();
  const start = { x: bounds!.x + 100, y: bounds!.y + bounds!.height / 2 };
  await page.mouse.move(start.x, start.y);
  await page.mouse.down();
  await page.mouse.move(start.x + 64, start.y + 40, { steps: 4 });
  await page.mouse.up();
  await expect.poll(() => positions([first, second, note])).toEqual([
    { x: initial[0]!.x + 72, y: initial[0]!.y + 41 },
    { x: initial[1]!.x + 72, y: initial[1]!.y + 41 },
    initial[2],
  ]);
  await expect(page.getByRole("status").filter({ hasText: /^2 selected/ })).toBeVisible();

  await page.keyboard.press("Shift+Space");
  await expect(first).toHaveAccessibleDescription("Selected canvas item");
  await expect(second).not.toHaveAccessibleDescription("Selected canvas item");
});

test("search filters results without replacing terminal cards and focuses the chosen note", async ({ page }, testInfo) => {
  const terminal = canvasCard(page, "Terminal 1", "terminal");
  const note = canvasCard(page, "Notes", "note");
  await note.getByRole("textbox").fill("Investigate the orange release checklist");
  const terminalElement = await terminal.elementHandle();
  expect(terminalElement).not.toBeNull();
  await page.getByRole("main").focus();
  await page.keyboard.press("ControlOrMeta+f");
  const search = page.getByRole("searchbox", { name: "Search canvas items" });
  const panel = page.getByRole("region", { name: "Canvas items", exact: true });
  await expect(search).toBeFocused();
  await search.fill("ORANGE checklist");
  await expect(panel.getByRole("status")).toHaveText("1 of 3 items");
  await expect(panel.getByRole("list", { name: "Canvas search results" }).getByRole("button")).toHaveCount(1);
  await expect(page.getByRole("article")).toHaveCount(3);
  expect(await terminalElement!.evaluate((element) => element.isConnected)).toBe(true);
  await page.screenshot({ path: testInfo.outputPath("canvas-desktop-search.png") });

  await search.press("ArrowDown");
  await expect(panel.getByRole("button", { name: /Notes Investigate/ })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(panel).toHaveCount(0);
  await expect(note).toBeFocused();
  await expect(note).toBeInViewport();

  await page.getByRole("button", { name: "Show canvas items", exact: true }).click();
  await search.fill("no-such-canvas-item");
  await expect(panel.getByRole("status")).toHaveText("0 of 3 items");
  await expect(panel.getByText(/No matching items/)).toBeVisible();
  await search.press("Escape");
  await expect(page.getByRole("button", { name: "Show canvas items", exact: true })).toBeFocused();
  expect(await terminalElement!.evaluate((element) => element.isConnected)).toBe(true);
});

test("editing a note keeps selection and delete shortcuts inside its text field", async ({ page }) => {
  await page.getByRole("button", { name: "Select all canvas items" }).click();
  const note = canvasCard(page, "Notes", "note");
  const editor = note.getByRole("textbox");
  const initial = await positions([note]);
  await editor.fill("Replace this note");
  await editor.press("ControlOrMeta+a");
  await editor.press("Backspace");
  await expect(editor).toHaveValue("");
  await expect(page.getByRole("article")).toHaveCount(3);
  await expect(page.getByRole("status").filter({ hasText: /^3 selected/ })).toBeVisible();
  await editor.fill("abc");
  await editor.press("ArrowLeft");
  await editor.press("Delete");
  await expect(editor).toHaveValue("ab");
  await expect.poll(() => positions([note])).toEqual(initial);
  await expect(page.getByRole("article")).toHaveCount(3);
});

test("creates and finds a Gemini draft in a compact window", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 640, height: 800 });
  await expect.poll(async () => (await page.getByRole("main").boundingBox())?.width).toBe(640);
  await page.getByRole("button", { name: "Add terminal card" }).click();
  const dialog = page.getByRole("dialog", { name: "New Terminal", exact: true });
  await dialog.getByRole("radio", { name: "Gemini", exact: true }).check();
  await expect(dialog.getByRole("textbox", { name: "Terminal name", exact: true })).toHaveValue("Gemini");
  await expect(dialog.getByRole("textbox", { name: "Command", exact: true })).toHaveValue("gemini");
  await dialog.getByRole("textbox", { name: "Working directory", exact: true }).fill("/workspace/gemini-review");
  await dialog.getByRole("button", { name: "Create terminal", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.getByRole("article")).toHaveCount(4);

  for (const width of [360, 640]) {
    await page.setViewportSize({ width, height: 800 });
    await expect.poll(async () => (await page.getByRole("main").boundingBox())?.width).toBe(width);
    await expect.poll(async () => {
      const status = await page.getByRole("status").filter({ hasText: /^1 selected/ }).boundingBox();
      const context = await page.getByRole("main").getByText("Workspace", { exact: true }).boundingBox();
      return status !== null && context !== null && status.y + status.height <= context.y;
    }).toBe(true);
  }

  await page.getByRole("button", { name: "Show canvas items", exact: true }).click();
  const search = page.getByRole("searchbox", { name: "Search canvas items" });
  await search.fill("gemini-review");
  const panel = page.getByRole("region", { name: "Canvas items", exact: true });
  await expect(panel.getByRole("status")).toHaveText("1 of 4 items");
  await expect(panel).toBeInViewport({ ratio: 1 });
  await page.screenshot({ path: testInfo.outputPath("canvas-compact-search.png") });
  await search.press("Enter");
  const gemini = canvasCard(page, "Gemini", "terminal");
  await expect(gemini).toBeFocused();
  await expect(gemini).toBeInViewport({ ratio: 1 });
  await page.reload();
  await expect(gemini).toHaveCount(1);
});

/** Resolves a canvas card through its user-facing accessible name. */
function canvasCard(page: Page, title: string, kind: "terminal" | "note") {
  return page.getByRole("article", { name: `${title}, ${kind} canvas item`, exact: true });
}

/** Reads rendered positions independently of canvas scrolling and zoom. */
async function positions(cards: readonly Locator[]) {
  return Promise.all(cards.map((card) => card.evaluate((element) => {
    const matrix = new DOMMatrix(getComputedStyle(element).transform);
    return { x: matrix.m41, y: matrix.m42 };
  })));
}
