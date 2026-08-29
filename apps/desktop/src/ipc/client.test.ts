import { describe, expect, it } from "vitest";

import {
  createIpcClient,
  createMockIpcClient,
  disconnectedError,
  IpcRequestError,
  PROTOCOL_V1,
  rejectWith,
} from "./index";
import { helloFixture } from "../test/ipc-fixtures";

describe("createIpcClient", () => {
  it("wraps transport failures as IpcRequestError", async () => {
    const client = createIpcClient({
      async send() {
        throw disconnectedError;
      },
      listen() {
        return () => undefined;
      },
    });

    await expect(client.request("system.hello", {})).rejects.toBeInstanceOf(
      IpcRequestError,
    );
  });

  it("forwards typed successes and drops unknown events", async () => {
    const hello = helloFixture();
    const seen: string[] = [];
    const client = createIpcClient({
      async send() {
        return hello;
      },
      listen(listener) {
        listener({ kind: "not-an-event" });
        listener({
          kind: "event",
          version: PROTOCOL_V1,
          event: "project.removed",
          sequence: 1,
          payload: { projectId: hello.instanceId },
        });
        return () => undefined;
      },
    });
    client.subscribe((event) => {
      seen.push(event.event);
    });

    await expect(client.request("system.hello", {})).resolves.toEqual(hello);
    expect(seen).toEqual(["project.removed"]);
  });
});

describe("createMockIpcClient", () => {
  it("captures handlers at queue time so later handlers cannot rewrite earlier requests", async () => {
    const client = createMockIpcClient({
      "system.hello": () => helloFixture({ daemonVersion: "first" }),
    });
    client.stall();
    const first = client.request("system.hello", {});
    client.setHandler("system.hello", () =>
      helloFixture({ daemonVersion: "second" }),
    );
    const second = client.request("system.hello", {});

    await client.flushLast();
    await expect(second).resolves.toMatchObject({ daemonVersion: "second" });
    await client.flushNext();
    await expect(first).resolves.toMatchObject({ daemonVersion: "first" });
  });

  it("rejects missing handlers with a request error", async () => {
    const client = createMockIpcClient();
    await expect(client.request("state.snapshot", {})).rejects.toMatchObject({
      error: { message: "No mock handler for state.snapshot." },
    });
    expect(() =>
      rejectWith({ code: "E", message: "nope" })(),
    ).toThrow(IpcRequestError);
  });
});
