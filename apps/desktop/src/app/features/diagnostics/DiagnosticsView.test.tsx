import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { DiagnosticsSnapshot } from "../../../ipc/types";
import { DiagnosticsView } from "./DiagnosticsView";

const DIAGNOSTICS: DiagnosticsSnapshot = {
  daemonVersion: "0.1.0",
  protocolVersion: 1,
  schemaVersion: 2,
  daemonInstanceId: "daemon-instance",
  dataPath: "/Users/test/Library/Application Support/CLI Master",
  runtimePath: "/tmp/cli-master-test",
  logPath: "/Users/test/Library/Application Support/CLI Master/logs",
  effectivePath: ["/usr/local/bin", "/usr/bin"],
  recentIssues: [],
};

describe("DiagnosticsView", () => {
  it("renders local diagnostics in the canvas visual language", async () => {
    const user = userEvent.setup();
    const onOpenCanvas = vi.fn();
    const onRetryConnection = vi.fn();

    render(
      <DiagnosticsView
        onOpenCanvas={onOpenCanvas}
        onRetryConnection={onRetryConnection}
        onLoad={vi.fn().mockResolvedValue(DIAGNOSTICS)}
      />,
    );

    expect(await screen.findByText(DIAGNOSTICS.dataPath)).toBeVisible();
    expect(
      screen.getByRole("heading", { name: "Diagnostics", level: 1 }),
    ).toBeVisible();
    expect(screen.getByText(/Everything looks healthy\./)).toBeVisible();

    await user.click(screen.getByRole("button", { name: "Back to canvas" }));
    await user.click(screen.getByRole("button", { name: "Reconnect" }));

    expect(onOpenCanvas).toHaveBeenCalledOnce();
    expect(onRetryConnection).toHaveBeenCalledOnce();
  });
});
