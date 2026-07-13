import { describe, expect, it } from "vitest";
import { emptyStemResults, rememberStemPaths, stemPathsForPurpose } from "./stemResults";

describe("stem results by separation mode", () => {
  it("keeps completed files when switching between modes", () => {
    const vocals = ["C:/output/Vocals.wav"];
    const instruments = ["C:/output/Drums.wav", "C:/output/Bass.wav"];
    const withVocals = rememberStemPaths(emptyStemResults(), "vocals", vocals);
    const withInstruments = rememberStemPaths(withVocals, "instruments", instruments);

    expect(stemPathsForPurpose(withInstruments, "instruments")).toEqual(instruments);
    expect(stemPathsForPurpose(withInstruments, "vocals")).toEqual(vocals);
    expect(stemPathsForPurpose(withInstruments, "instruments_6s")).toEqual([]);
  });
});
