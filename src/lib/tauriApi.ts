import { invoke } from "@tauri-apps/api/core";
import {
  isKnownStemModel,
  type StemModel,
  type StemSeparationPurpose,
  type StemToolStatus,
} from "./stemSeparate";

export interface AnalyzeResult {
  bpm: number;
  key: string;
  durationSec: number;
}

export interface ShellMenuStatus {
  registered: boolean;
  available: boolean;
  detail: string;
}

export interface StartupCli {
  separate?: {
    model: string;
    path: string;
    outputDir?: string | null;
    flattenOutput?: boolean;
  } | null;
  analyze?: {
    path: string;
  } | null;
}

export async function getStartupCli(): Promise<StartupCli> {
  return invoke("get_startup_cli");
}

export function parseStartupCliArgs(args: string[]): StartupCli {
  const separateStart = args.indexOf("separate");
  const analyzeStart = args.indexOf("analyze");
  if (analyzeStart >= 0 && (separateStart < 0 || analyzeStart < separateStart)) {
    let path: string | undefined;
    for (let index = analyzeStart + 1; index < args.length; index += 1) {
      const arg = args[index];
      if (arg === "--path" && args[index + 1]) {
        path = args[++index];
      } else if (!arg.startsWith("-") && !path) {
        path = arg;
      }
    }
    return path ? { analyze: { path } } : {};
  }
  if (separateStart < 0) return {};
  let model: string | undefined;
  let path: string | undefined;
  let outputDir: string | undefined;
  for (let index = separateStart + 1; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === "--model" && args[index + 1]) {
      model = args[++index];
    } else if (arg === "--path" && args[index + 1]) {
      path = args[++index];
    } else if (arg === "--output-dir" && args[index + 1]) {
      outputDir = args[++index];
    } else if (!arg.startsWith("-") && !path) {
      path = arg;
    }
  }
  if (!model || !path || !isKnownStemModel(model)) return {};
  return {
    separate: {
      model,
      path,
      ...(outputDir ? { outputDir } : {}),
      flattenOutput: !outputDir,
    },
  };
}

export async function defaultOutputDir(): Promise<string> {
  return invoke("default_output_dir");
}

export async function revealInFolder(path: string): Promise<void> {
  return invoke("reveal_in_folder", { path });
}

export async function stemToolStatus(): Promise<StemToolStatus> {
  return invoke("stem_tool_status");
}

export async function stemToolInstall(
  sessionId: string,
  purpose: StemSeparationPurpose
): Promise<void> {
  return invoke("stem_tool_install", { sessionId, purpose });
}

export async function stemGpuInstall(
  sessionId: string,
  target: "cuda" | "cpu"
): Promise<void> {
  return invoke("stem_gpu_install", { sessionId, target });
}

export async function stemSeparateStart(args: {
  inputPath: string;
  outputDir: string;
  sessionId: string;
  model: StemModel;
  devicePreference?: string | null;
  flattenOutput?: boolean;
}): Promise<void> {
  return invoke("stem_separate_start", {
    inputPath: args.inputPath,
    outputDir: args.outputDir,
    sessionId: args.sessionId,
    model: args.model,
    devicePreference: args.devicePreference ?? null,
    flattenOutput: args.flattenOutput ?? false,
  });
}

export async function stemSeparateStop(sessionId: string): Promise<void> {
  return invoke("stem_separate_stop", { sessionId });
}

export async function youtubeDownload(args: {
  sessionId: string;
  url: string;
  outputDir: string;
}): Promise<void> {
  return invoke("youtube_download", args);
}

export async function youtubeCancel(): Promise<void> {
  return invoke("youtube_cancel");
}

export async function analyzeAudio(
  sessionId: string,
  inputPath: string
): Promise<void> {
  return invoke("analyze_audio", { sessionId, inputPath });
}

export async function analyzeCancel(): Promise<void> {
  return invoke("analyze_cancel");
}

export async function shellMenuStatus(): Promise<ShellMenuStatus> {
  return invoke("shell_menu_status");
}

export async function shellMenuRegister(): Promise<ShellMenuStatus> {
  return invoke("shell_menu_register");
}

export async function shellMenuUnregister(): Promise<ShellMenuStatus> {
  return invoke("shell_menu_unregister");
}
