import { mockIPC } from "@tauri-apps/api/mocks";
import { expect, it } from "vitest";
import {
  listClaudeSessions,
  stopIdleClaudeSessions,
  type ClaudeSessions,
  type StopSessionsResult,
} from "@/lib/ipc";
import inventory from "@/test/fixtures/ipc/claude-sessions.json";
import result from "@/test/fixtures/ipc/stop-sessions-result.json";

it("reads Rust session DTOs and sends only the confirmed identities", async () => {
  const typedInventory: ClaudeSessions = inventory;
  const typedResult: StopSessionsResult = result;
  const targets = [{ pid: 42, sessionId: "fixture-session" }];
  mockIPC((cmd, args) => {
    if (cmd === "list_claude_sessions") return typedInventory;
    expect(cmd).toBe("stop_idle_claude_sessions");
    expect(args).toEqual({ targets });
    return typedResult;
  });
  expect(await listClaudeSessions()).toEqual(typedInventory);
  expect(await stopIdleClaudeSessions(targets)).toEqual(typedResult);
});
