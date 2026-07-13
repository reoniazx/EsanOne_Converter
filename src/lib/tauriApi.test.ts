import { describe, expect, it } from "vitest";
import { parseStartupCliArgs } from "./tauriApi";

describe("parseStartupCliArgs", () => {
  it("parses the Key/BPM context-menu operation", () => {
    expect(
      parseStartupCliArgs(["esanone-convert.exe", "analyze", "--path", "D:\\Music\\song.wav"])
    ).toEqual({ analyze: { path: "D:\\Music\\song.wav" } });
  });

  it("marks context-menu separation for flattened output", () => {
    expect(
      parseStartupCliArgs([
        "esanone-convert.exe",
        "separate",
        "--model",
        "htdemucs_ft",
        "--path",
        "D:\\Music\\song.wav",
      ])
    ).toEqual({
      separate: {
        model: "htdemucs_ft",
        path: "D:\\Music\\song.wav",
        flattenOutput: true,
      },
    });
  });
});
