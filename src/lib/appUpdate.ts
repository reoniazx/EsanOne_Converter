import { getVersion } from "@tauri-apps/api/app";
import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export const UPDATE_CHECK_TIMEOUT_MS = 15_000;
export const UPDATE_DOWNLOAD_TIMEOUT_MS = 300_000;

export type UpdateFailureStage = "check" | "download" | "install" | "restart";

export function updateErrorKey(stage: UpdateFailureStage): string {
  switch (stage) {
    case "check":
      return "Unable to check for updates";
    case "download":
      return "Update download failed";
    case "install":
      return "Update installation failed";
    case "restart":
      return "Restart failed after update";
  }
}

export type UpdatePhase =
  | "idle"
  | "checking"
  | "available"
  | "upToDate"
  | "downloading"
  | "installing"
  | "restartRequired"
  | "error";

export interface UpdateStatus {
  phase: UpdatePhase;
  currentVersion: string;
  availableVersion: string | null;
  progress: number | null;
  error: string | null;
}

export function createIdleUpdateStatus(currentVersion = ""): UpdateStatus {
  return {
    phase: "idle",
    currentVersion,
    availableVersion: null,
    progress: null,
    error: null,
  };
}

export async function loadAppVersion(): Promise<string> {
  try {
    return await getVersion();
  } catch {
    return "";
  }
}

export async function checkForAppUpdate(): Promise<{
  update: Update | null;
  currentVersion: string;
}> {
  const currentVersion = await loadAppVersion();
  const update = await check({ timeout: UPDATE_CHECK_TIMEOUT_MS });
  return { update, currentVersion };
}

export async function downloadAndInstallUpdate(
  update: Update,
  onProgress?: (progress: number | null, phase: "downloading" | "installing") => void
): Promise<void> {
  let contentLength: number | null = null;
  let downloaded = 0;

  await update.downloadAndInstall((event: DownloadEvent) => {
    switch (event.event) {
      case "Started":
        contentLength = event.data.contentLength ?? null;
        downloaded = 0;
        onProgress?.(contentLength ? 0 : null, "downloading");
        break;
      case "Progress":
        downloaded += event.data.chunkLength;
        if (contentLength && contentLength > 0) {
          onProgress?.(Math.min(1, downloaded / contentLength), "downloading");
        } else {
          onProgress?.(null, "downloading");
        }
        break;
      case "Finished":
        onProgress?.(1, "installing");
        break;
    }
  }, { timeout: UPDATE_DOWNLOAD_TIMEOUT_MS });
}

export async function relaunchApp(): Promise<void> {
  await relaunch();
}
