import { describe, expect, it } from "vitest";
import { downsampleWaveform } from "./waveform";

describe("downsampleWaveform", () => {
  it("keeps the strongest amplitude from each waveform bucket", () => {
    const result = downsampleWaveform(new Float32Array([0.1, -0.8, 0.2, 0.4, -0.3, 0.9]), 3);
    expect(result).toHaveLength(3);
    expect(result[0]).toBeCloseTo(0.8);
    expect(result[1]).toBeCloseTo(0.4);
    expect(result[2]).toBeCloseTo(0.9);
  });

  it("returns an empty waveform for empty input", () => {
    expect(downsampleWaveform(new Float32Array(), 160)).toEqual([]);
  });
});
