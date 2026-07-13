import { describe, expect, it } from "vitest";
import type { AnalyzeResult } from "./tauriApi";
import { formatKeyBpmToast } from "./keyBpmNotification";

describe("formatKeyBpmToast", () => {
  it("formats Key and BPM with the active locale labels", () => {
    const result: AnalyzeResult = { key: "Am", bpm: 120, durationSec: 180 };
    expect(formatKeyBpmToast(result, "Key", "BPM")).toEqual({
      title: "Analysis complete",
      body: "Key: Am · BPM: 120",
    });
    expect(formatKeyBpmToast(result, "คีย์", "BPM")).toEqual({
      title: "Analysis complete",
      body: "คีย์: Am · BPM: 120",
    });
  });
});
