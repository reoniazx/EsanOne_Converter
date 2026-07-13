import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  getVersion: vi.fn(),
  check: vi.fn(),
  relaunch: vi.fn(),
}));

vi.mock("@tauri-apps/api/app", () => ({ getVersion: mocks.getVersion }));
vi.mock("@tauri-apps/plugin-updater", () => ({ check: mocks.check }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: mocks.relaunch }));

import {
  checkForAppUpdate,
  createIdleUpdateStatus,
  downloadAndInstallUpdate,
  updateErrorKey,
} from "./appUpdate";

beforeEach(() => {
  vi.clearAllMocks();
});

describe("createIdleUpdateStatus", () => {
  it("starts idle with optional version", () => {
    expect(createIdleUpdateStatus("0.1.0")).toEqual({
      phase: "idle",
      currentVersion: "0.1.0",
      availableVersion: null,
      progress: null,
      error: null,
    });
  });
});

describe("checkForAppUpdate", () => {
  it("uses a bounded request timeout", async () => {
    const update = { version: "0.1.1" };
    mocks.getVersion.mockResolvedValue("0.1.0");
    mocks.check.mockResolvedValue(update);

    await expect(checkForAppUpdate()).resolves.toEqual({ update, currentVersion: "0.1.0" });
    expect(mocks.check).toHaveBeenCalledWith({ timeout: 15_000 });
  });
});

describe("downloadAndInstallUpdate", () => {
  it("uses a bounded download timeout and reports download/install phases", async () => {
    const events: Array<[number | null, "downloading" | "installing"]> = [];
    const update = {
      downloadAndInstall: vi.fn(async (onEvent: (event: unknown) => void) => {
        onEvent({ event: "Started", data: { contentLength: 100 } });
        onEvent({ event: "Progress", data: { chunkLength: 40 } });
        onEvent({ event: "Finished" });
      }),
    };

    await downloadAndInstallUpdate(update as never, (progress, phase) => {
      events.push([progress, phase]);
    });

    expect(update.downloadAndInstall).toHaveBeenCalledWith(expect.any(Function), { timeout: 300_000 });
    expect(events).toEqual([
      [0, "downloading"],
      [0.4, "downloading"],
      [1, "installing"],
    ]);
  });
});

describe("updateErrorKey", () => {
  it("maps each update failure stage to a user-facing message key", () => {
    expect(updateErrorKey("check")).toBe("Unable to check for updates");
    expect(updateErrorKey("download")).toBe("Update download failed");
    expect(updateErrorKey("install")).toBe("Update installation failed");
    expect(updateErrorKey("restart")).toBe("Restart failed after update");
  });
});
