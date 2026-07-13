import { describe, expect, it } from "vitest";
import { nextPhaseAfterYoutubeDownload, shouldAnalyzeYoutubeDownload } from "./youtubeDownloadFlow";

describe("YouTube download intent", () => {
  it("keeps download-only requests ready without starting analysis", () => {
    expect(shouldAnalyzeYoutubeDownload("downloadOnly")).toBe(false);
    expect(nextPhaseAfterYoutubeDownload("downloadOnly")).toBe("ready");
  });

  it("continues to analysis for the search workflow", () => {
    expect(shouldAnalyzeYoutubeDownload("analyze")).toBe(true);
    expect(nextPhaseAfterYoutubeDownload("analyze")).toBe("analyzing");
  });
});
