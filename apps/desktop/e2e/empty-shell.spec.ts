import { expect, test } from "@playwright/test";

test.describe("empty desktop shell", () => {
  test("explains why session and project actions are unavailable", async ({
    page,
  }) => {
    await page.goto("/");

    const newSession = page.getByRole("button", { name: "New Session" });
    await expect(newSession).toBeDisabled();
    await expect(newSession).toHaveAccessibleDescription("Add a project first");

    await expect(
      page.getByRole("button", { name: "Add Project" }),
    ).toHaveAccessibleDescription(
      "Available when the local daemon is connected.",
    );
  });

  test("reports an honest empty local workspace", async ({ page }) => {
    await page.goto("/");

    await expect(
      page.getByRole("heading", { name: "No project selected", level: 1 }),
    ).toBeVisible();
    await expect(
      page.getByRole("heading", { name: "Add a repository to begin" }),
    ).toBeVisible();
    await expect(page.getByRole("status")).toHaveText("Daemon unavailable");
    await expect(
      page.getByRole("navigation", { name: "Workspace navigation" }),
    ).toBeVisible();
  });

  test("places the workspace skip link first in keyboard order", async ({
    page,
  }) => {
    await page.goto("/");

    await page.keyboard.press("Tab");
    const skipLink = page.getByRole("link", { name: "Skip to workspace" });
    await expect(skipLink).toBeFocused();
    await expect(skipLink).toHaveAttribute("href", "#workspace");
    await expect(page.getByRole("main")).toHaveAttribute("id", "workspace");
  });
});
