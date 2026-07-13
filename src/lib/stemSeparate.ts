export const DEFAULT_STEM_MODEL = "htdemucs_ft";
export const MELBANDROFORMER_MODEL = "melbandroformer_vocals";
export const HTDEMUCS_6S_MODEL = "htdemucs_6s";

export const FOUR_STEM_NAMES = ["Drums", "Bass", "Other", "Vocals"] as const;
export const SIX_STEM_NAMES = [
  "Drums",
  "Bass",
  "Other",
  "Vocals",
  "Guitar",
  "Piano",
] as const;

export type StemSeparationPurpose = "vocals" | "instruments" | "instruments_6s";
export type StemDevicePreference = "auto" | "cpu";
export type StemModel =
  | typeof MELBANDROFORMER_MODEL
  | typeof DEFAULT_STEM_MODEL
  | typeof HTDEMUCS_6S_MODEL;

export interface StemGpuStatus {
  torchOk: boolean;
  torchInstalled: boolean;
  cudaAvailable: boolean;
  hasNvidiaAdapter: boolean;
  torchVersion?: string | null;
  deviceName?: string | null;
  memoryMb?: number | null;
}

export interface StemToolStatus {
  demucs: boolean;
  melBandRoFormer: boolean;
  python: boolean;
  pythonVersion?: string | null;
  gpu: StemGpuStatus;
}

export function expectedStemNames(model: string): string[] {
  if (model === HTDEMUCS_6S_MODEL) return [...SIX_STEM_NAMES];
  if (model === MELBANDROFORMER_MODEL) return ["Vocals", "Instrumental"];
  return [...FOUR_STEM_NAMES];
}

export const STEM_BACKENDS = {
  vocals: {
    model: MELBANDROFORMER_MODEL,
    label: "Vocals (MelBandRoFormer)",
    expectedStemNames: ["Vocals", "Instrumental"] as string[],
    readyKey: "melBandRoFormer" as const,
    progressPasses: 1,
  },
  instruments: {
    model: DEFAULT_STEM_MODEL,
    label: "Instruments (HT-Demucs)",
    expectedStemNames: [...FOUR_STEM_NAMES] as string[],
    readyKey: "demucs" as const,
    progressPasses: 4,
  },
  instruments_6s: {
    model: HTDEMUCS_6S_MODEL,
    label: "Instruments 6 (Guitar + Piano)",
    expectedStemNames: [...SIX_STEM_NAMES] as string[],
    readyKey: "demucs" as const,
    progressPasses: 1,
  },
} satisfies Record<
  StemSeparationPurpose,
  {
    model: StemModel;
    label: string;
    expectedStemNames: string[];
    readyKey: "melBandRoFormer" | "demucs";
    progressPasses: number;
  }
>;

export function resolveStemBackend(purpose: StemSeparationPurpose) {
  return STEM_BACKENDS[purpose];
}

export function isKnownStemModel(model: string): model is StemModel {
  return (
    model === MELBANDROFORMER_MODEL ||
    model === DEFAULT_STEM_MODEL ||
    model === HTDEMUCS_6S_MODEL
  );
}

export function purposeForStemModel(model: string): StemSeparationPurpose | null {
  if (model === MELBANDROFORMER_MODEL) return "vocals";
  if (model === DEFAULT_STEM_MODEL) return "instruments";
  if (model === HTDEMUCS_6S_MODEL) return "instruments_6s";
  return null;
}

export function stemReadyForPurpose(
  status: StemToolStatus | null,
  purpose: StemSeparationPurpose
): boolean {
  return Boolean(status?.[resolveStemBackend(purpose).readyKey]);
}

export function canEnableStemGpu(
  status: StemToolStatus | null,
  purpose: StemSeparationPurpose
): boolean {
  return Boolean(
    stemReadyForPurpose(status, purpose) &&
      status?.gpu.hasNvidiaAdapter &&
      status.gpu.torchOk &&
      !status.gpu.cudaAvailable
  );
}

export function stemExecutionDeviceLabel(
  status: StemToolStatus | null,
  preference: StemDevicePreference
): string {
  if (preference === "cpu") return "CPU";
  if (status?.gpu.cudaAvailable) {
    return status.gpu.deviceName ?? "GPU";
  }
  return "CPU";
}

/** Average multi-pass demucs tqdm into 0..1. */
export function createStemProgressTracker(passes: number) {
  let passIndex = 0;
  let lastInPass = 0;
  return {
    ingest(line: string): number | null {
      const match = line.match(/(\d+)%\|/);
      if (!match) return null;
      const pct = Number(match[1]) / 100;
      if (pct + 0.001 < lastInPass && passIndex + 1 < passes) {
        passIndex += 1;
      }
      lastInPass = pct;
      return Math.min(1, (passIndex + pct) / Math.max(1, passes));
    },
  };
}
