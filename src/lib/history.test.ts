import { describe, expect, it } from "vitest";
import { emptyStemResults } from "./stemResults";
import { parseHistory, upsertHistory, type TrackHistoryItem } from "./history";

function entry(overrides: Partial<TrackHistoryItem> = {}): TrackHistoryItem {
  return {
    id: "song-1",
    title: "เพลงทดสอบ",
    audioPath: "C:/music/song.wav",
    thumbnailUrl: null,
    youtubeTrack: false,
    analysis: null,
    stemResults: emptyStemResults(),
    purpose: "vocals",
    updatedAt: 1,
    ...overrides,
  };
}

describe("song history", () => {
  it("puts the latest song first and updates an existing song", () => {
    const first = entry();
    const second = entry({ id: "song-2", title: "เพลงสอง", audioPath: "C:/music/two.wav", updatedAt: 2 });
    const updated = entry({ title: "เพลงทดสอบ (แก้ไข)", updatedAt: 3 });

    expect(upsertHistory(upsertHistory([first], second), updated)).toEqual([updated, second]);
  });

  it("ignores malformed persisted history", () => {
    expect(parseHistory(JSON.stringify([{ title: "ไม่มี path" }, entry()]))).toEqual([entry()]);
    expect(parseHistory("not-json")).toEqual([]);
  });
});
