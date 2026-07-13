import { describe, expect, it } from "vitest";
import { friendlyErrorKey } from "./errors";

describe("friendlyErrorKey", () => {
  it("turns rejected YouTube URLs into an actionable message", () => {
    expect(friendlyErrorKey("download", "Please paste a valid YouTube URL.")).toBe(
      "Enter a valid YouTube link"
    );
  });

  it("hides technical GPU details behind a clear recovery message", () => {
    expect(
      friendlyErrorKey(
        "gpu",
        "CUDA PyTorch installed but torch.cuda.is_available() is false."
      )
    ).toBe("GPU setup could not be completed");
  });

  it("keeps cancellation separate from a failed operation", () => {
    expect(friendlyErrorKey("separation", "Cancelled")).toBe("Operation cancelled");
  });

  it("uses an action-specific fallback for raw process errors", () => {
    expect(friendlyErrorKey("analysis", "Traceback ... ValueError")).toBe(
      "Analysis could not be completed"
    );
    expect(friendlyErrorKey("separation", "separation exited with code 1")).toBe(
      "Separation could not be completed"
    );
  });

  it("gives a platform-specific recovery message when Essentia is unavailable", () => {
    expect(friendlyErrorKey("analysis", "Essentia analysis is not available on this platform yet.")).toBe(
      "Essentia analysis is not available"
    );
  });
});
