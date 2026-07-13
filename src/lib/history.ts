import type { AnalyzeResult } from "./tauriApi";
import {
  emptyStemResults,
  type StemResultsByPurpose,
} from "./stemResults";
import type { StemSeparationPurpose } from "./stemSeparate";

export const HISTORY_STORAGE_KEY = "esanone_convert_history";
export const MAX_HISTORY_ITEMS = 30;

export type TrackHistoryItem = {
  id: string;
  title: string;
  audioPath: string;
  thumbnailUrl: string | null;
  youtubeTrack: boolean;
  analysis: AnalyzeResult | null;
  stemResults: StemResultsByPurpose;
  purpose: StemSeparationPurpose;
  updatedAt: number;
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value && typeof value === "object");
}

function parseAnalysis(value: unknown): AnalyzeResult | null {
  if (!isRecord(value)) return null;
  if (typeof value.bpm !== "number" || typeof value.key !== "string" || typeof value.durationSec !== "number") {
    return null;
  }
  return { bpm: value.bpm, key: value.key, durationSec: value.durationSec };
}

function parseStemResults(value: unknown): StemResultsByPurpose {
  if (!isRecord(value)) return emptyStemResults();
  const read = (purpose: StemSeparationPurpose): string[] =>
    Array.isArray(value[purpose]) ? value[purpose].filter((path): path is string => typeof path === "string") : [];
  return {
    vocals: read("vocals"),
    instruments: read("instruments"),
    instruments_6s: read("instruments_6s"),
  };
}

function parseEntry(value: unknown): TrackHistoryItem | null {
  if (!isRecord(value)) return null;
  if (
    typeof value.id !== "string" ||
    typeof value.title !== "string" ||
    typeof value.audioPath !== "string" ||
    !value.title.trim() ||
    !value.audioPath.trim()
  ) {
    return null;
  }
  const purpose = value.purpose;
  if (purpose !== "vocals" && purpose !== "instruments" && purpose !== "instruments_6s") return null;
  return {
    id: value.id,
    title: value.title,
    audioPath: value.audioPath,
    thumbnailUrl: typeof value.thumbnailUrl === "string" ? value.thumbnailUrl : null,
    youtubeTrack: value.youtubeTrack === true,
    analysis: parseAnalysis(value.analysis),
    stemResults: parseStemResults(value.stemResults),
    purpose,
    updatedAt: typeof value.updatedAt === "number" ? value.updatedAt : 0,
  };
}

export function parseHistory(raw: string | null): TrackHistoryItem[] {
  if (!raw) return [];
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.map(parseEntry).filter((item): item is TrackHistoryItem => item !== null).slice(0, MAX_HISTORY_ITEMS);
  } catch {
    return [];
  }
}

export function loadHistory(storage?: Pick<Storage, "getItem">): TrackHistoryItem[] {
  try {
    const source = storage ?? (typeof window !== "undefined" ? window.localStorage : undefined);
    return source ? parseHistory(source.getItem(HISTORY_STORAGE_KEY)) : [];
  } catch {
    return [];
  }
}

export function saveHistory(items: TrackHistoryItem[], storage?: Pick<Storage, "setItem">): void {
  try {
    const target = storage ?? (typeof window !== "undefined" ? window.localStorage : undefined);
    target?.setItem(HISTORY_STORAGE_KEY, JSON.stringify(items.slice(0, MAX_HISTORY_ITEMS)));
  } catch {
    // History is best-effort; a storage quota or privacy setting must not block conversion.
  }
}

export function upsertHistory(items: TrackHistoryItem[], item: TrackHistoryItem): TrackHistoryItem[] {
  return [item, ...items.filter((existing) => existing.id !== item.id)].slice(0, MAX_HISTORY_ITEMS);
}
