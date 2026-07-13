import type { StemSeparationPurpose } from "./stemSeparate";

export type StemResultsByPurpose = Record<StemSeparationPurpose, string[]>;

export function emptyStemResults(): StemResultsByPurpose {
  return {
    vocals: [],
    instruments: [],
    instruments_6s: [],
  };
}

export function rememberStemPaths(
  results: StemResultsByPurpose,
  purpose: StemSeparationPurpose,
  paths: string[]
): StemResultsByPurpose {
  return {
    ...results,
    [purpose]: [...paths],
  };
}

export function stemPathsForPurpose(
  results: StemResultsByPurpose,
  purpose: StemSeparationPurpose
): string[] {
  return results[purpose];
}
