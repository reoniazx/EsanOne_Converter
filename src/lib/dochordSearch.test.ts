import { describe, expect, it } from "vitest";
import { buildDochordSearchUrl, parseDochordResults } from "./dochordSearch";

describe("dochord search", () => {
  it("builds a site-restricted Google Custom Search URL", () => {
    const url = buildDochordSearchUrl("เพลงทดสอบ", {
      apiKey: "test-key",
      searchEngineId: "test-cx",
    });
    const parsed = new URL(url);

    expect(parsed.origin + parsed.pathname).toBe("https://www.googleapis.com/customsearch/v1");
    expect(parsed.searchParams.get("key")).toBe("test-key");
    expect(parsed.searchParams.get("cx")).toBe("test-cx");
    expect(parsed.searchParams.get("q")).toBe("site:dochord.com เพลงทดสอบ");
  });

  it("keeps only valid Dochord result links", () => {
    expect(
      parseDochordResults({
        items: [
          { title: "เพลงทดสอบ", link: "https://www.dochord.com/123/", snippet: "คอร์ดเพลง" },
          { title: "ผลลัพธ์อื่น", link: "https://example.com/song", snippet: "อื่นๆ" },
          { title: "ลิงก์ปลอม", link: "https://dochord.com.evil.example/song", snippet: "ปลอม" },
        ],
      })
    ).toEqual([
      { title: "เพลงทดสอบ", link: "https://www.dochord.com/123/", snippet: "คอร์ดเพลง" },
    ]);
  });
});
