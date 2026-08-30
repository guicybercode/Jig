import { expect, test } from "@playwright/test";

test.describe("disconnected canvas shell", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
  });

  test("explains why local project actions are unavailable", async ({
    page,
  }) => {
    const addProject = page.getByRole("button", {
      name: "Add workspace project",
    });
    await expect(addProject).toBeDisabled();
    await expect(addProject).toHaveAccessibleDescription(
      "Reconnect the daemon to add a project.",
    );

    await expect(
      page.getByRole("region", { name: "Daemon disconnected" }),
    ).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Retry Connection" }),
    ).toBeEnabled();
  });

  test("keeps canvas navigation available while the daemon is offline", async ({
    page,
  }) => {
    await expect(
      page.getByRole("complementary", { name: "Canvas workspaces" }),
    ).toBeVisible();
    await expect(
      page.getByRole("navigation", { name: "Workspaces" }),
    ).toBeVisible();
    await expect(page.getByRole("button", { name: /My Workspace/ })).toBeVisible();
    await expect(page.getByRole("button", { name: "Settings" })).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Open diagnostics" }),
    ).toBeVisible();
  });

  test("hides and restores the canvas sidebar", async ({ page }) => {
    await page.getByRole("button", { name: "Hide workspace sidebar" }).click();
    const showSidebar = page.getByRole("button", {
      name: "Show workspace sidebar",
    });
    await expect(showSidebar).toBeVisible();

    await showSidebar.click();
    await expect(
      page.getByRole("complementary", { name: "Canvas workspaces" }),
    ).toBeVisible();
  });

  test("places the workspace skip link first in keyboard order", async ({
    page,
  }) => {
    await page.keyboard.press("Tab");
    const skipLink = page.getByRole("link", { name: "Skip to workspace" });
    await expect(skipLink).toBeFocused();
    await expect(skipLink).toHaveAttribute("href", "#workspace");
    await expect(page.getByRole("main")).toHaveAttribute("id", "workspace");
  });
});
