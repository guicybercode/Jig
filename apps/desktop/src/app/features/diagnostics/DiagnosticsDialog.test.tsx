import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { parseDaemonInstanceId, type DiagnosticsResponse } from "../../../ipc";
import { DiagnosticsDialog } from "./DiagnosticsDialog";

const report: DiagnosticsResponse = {
  daemonVersion: "0.1.0",
  protocolVersion: 1,
  schemaVersion: 3,
  daemonInstanceId: parseDaemonInstanceId(
    "01900000-0000-7000-8000-0000000000aa",
  ),
  dataPath: "~/.local/share/cli-master",
  runtimePath: "/tmp/cli-master",
  logPath: "~/.local/share/cli-master/logs",
  effectivePath: ["~/.local/bin", "/usr/bin"],
  recentIssues: [
    {
      code: "probe_failed",
      message: "TOKEN=[redacted]",
      action: "Check the executable path.",
    },
  ],
  exportText: '{"daemonVersion":"0.1.0","dataPath":"~/.local/share/cli-master"}',
};

describe("DiagnosticsDialog", () => {
  it("loads the typed snapshot and copies only the backend export", async () => {
    const user = userEvent.setup();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });

    render(
      <DiagnosticsDialog
        open
        load={async () => report}
        onClose={() => undefined}
      />,
    );

    const dialog = await screen.findByRole("dialog", { name: "Diagnostics" });
    expect(within(dialog).getByText("0.1.0")).toBeVisible();
    expect(within(dialog).getByText("~/.local/share/cli-master")).toBeVisible();

    await user.click(
      within(dialog).getByRole("button", {
        name: "Copy sanitized diagnostics",
      }),
    );

    expect(writeText).toHaveBeenCalledWith(report.exportText);
    expect(within(dialog).getByRole("status")).toHaveTextContent("Copied");
  });

  it("never derives a clipboard fallback from raw on-screen paths", async () => {
    const user = userEvent.setup();
    const writeText = vi.fn().mockRejectedValue(new Error("clipboard denied"));
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    const rawPath = "/Users/private-user/.local/share/cli-master";
    render(
      <DiagnosticsDialog
        open
        load={async () => ({
          ...report,
          dataPath: rawPath,
          exportText: '{"dataPath":"~/.local/share/cli-master"}',
        })}
        onClose={() => undefined}
      />,
    );
    const dialog = await screen.findByRole("dialog", { name: "Diagnostics" });

    await user.click(
      within(dialog).getByRole("button", {
        name: "Copy sanitized diagnostics",
      }),
    );

    expect(writeText).toHaveBeenCalledTimes(1);
    expect(String(writeText.mock.calls[0]?.[0])).not.toContain(rawPath);
    expect(within(dialog).getByRole("status")).toHaveTextContent(
      "raw on-screen response was not copied",
    );
  });

  it("does not render loader error details that may contain secrets", async () => {
    render(
      <DiagnosticsDialog
        open
        load={async () => {
          throw new Error("TOKEN=loader-secret");
        }}
        onClose={() => undefined}
      />,
    );

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Diagnostics could not be loaded safely",
    );
    expect(screen.queryByText(/loader-secret/)).not.toBeInTheDocument();
  });
});
