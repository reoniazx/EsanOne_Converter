import { describe, expect, it } from "vitest";
import {
  createStemProgressTracker,
  expectedStemNames,
  HTDEMUCS_6S_MODEL,
  MELBANDROFORMER_MODEL,
  resolveStemBackend,
  isKnownStemModel,
  purposeForStemModel,
  stemReadyForPurpose,
  type StemToolStatus,
} from "./stemSeparate";

const baseStatus = (partial: Partial<StemToolStatus> = {}): StemToolStatus => ({
  demucs: false,
  melBandRoFormer: false,
  python: true,
  gpu: {
    torchOk: true,
    torchInstalled: true,
    cudaAvailable: false,
    hasNvidiaAdapter: false,
  },
  ...partial,
});

describe("stemSeparate", () => {
  it("maps purposes to models and stems", () => {
    expect(resolveStemBackend("vocals").model).toBe(MELBANDROFORMER_MODEL);
    expect(resolveStemBackend("instruments").model).toBe("htdemucs_ft");
    expect(resolveStemBackend("instruments_6s").model).toBe(HTDEMUCS_6S_MODEL);
    expect(expectedStemNames(MELBANDROFORMER_MODEL)).toEqual(["Vocals", "Instrumental"]);
    expect(expectedStemNames(HTDEMUCS_6S_MODEL)).toContain("Guitar");
    expect(expectedStemNames(HTDEMUCS_6S_MODEL)).toContain("Piano");
  });

  it("accepts only the three supported model ids", () => {
    expect(isKnownStemModel(MELBANDROFORMER_MODEL)).toBe(true);
    expect(isKnownStemModel("htdemucs_ft")).toBe(true);
    expect(isKnownStemModel(HTDEMUCS_6S_MODEL)).toBe(true);
    expect(isKnownStemModel("htdemucs")).toBe(false);
    expect(purposeForStemModel("htdemucs_ft")).toBe("instruments");
    expect(purposeForStemModel("unknown-model")).toBeNull();
  });

  it("gates readiness per purpose", () => {
    expect(stemReadyForPurpose(baseStatus({ melBandRoFormer: true }), "vocals")).toBe(true);
    expect(stemReadyForPurpose(baseStatus({ melBandRoFormer: true }), "instruments")).toBe(false);
    expect(stemReadyForPurpose(baseStatus({ demucs: true }), "instruments_6s")).toBe(true);
  });

  it("averages multi-pass demucs progress", () => {
    const tracker = createStemProgressTracker(4);
    expect(tracker.ingest("10%|")).toBeCloseTo(0.1 / 4, 3);
    expect(tracker.ingest("100%|")).toBeCloseTo(1 / 4, 3);
    expect(tracker.ingest("50%|")).toBeCloseTo((1 + 0.5) / 4, 3);
  });
});
