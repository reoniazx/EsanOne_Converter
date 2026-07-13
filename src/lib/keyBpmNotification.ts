import type { AnalyzeResult } from "./tauriApi";

export interface KeyBpmToast {
  title: string;
  body: string;
}

function formatBpm(bpm: number): string {
  return Number.isInteger(bpm) ? String(bpm) : bpm.toFixed(1);
}

export function formatKeyBpmToast(
  result: AnalyzeResult,
  keyLabel: string,
  bpmLabel: string,
  title = "Analysis complete"
): KeyBpmToast {
  return {
    title,
    body: `${keyLabel}: ${result.key} · ${bpmLabel}: ${formatBpm(result.bpm)}`,
  };
}
