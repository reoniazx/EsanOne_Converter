import { describe, expect, it } from "vitest";
import { isCurrentSessionEvent } from "./sessionEvents";

describe("session event guards", () => {
  it("accepts only events for the active session", () => {
    expect(isCurrentSessionEvent("sep-2", { sessionId: "sep-2" })).toBe(true);
    expect(isCurrentSessionEvent("sep-2", { sessionId: "sep-1" })).toBe(false);
    expect(isCurrentSessionEvent(null, { sessionId: "sep-2" })).toBe(false);
  });
});
