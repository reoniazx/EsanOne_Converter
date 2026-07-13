import { describe, expect, it } from "vitest";
import { youtubeThumbnailUrl, youtubeVideoId } from "./youtubeThumbnail";

describe("youtubeThumbnailUrl", () => {
  it("extracts IDs from supported YouTube URL forms", () => {
    const id = "dQw4w9WgXcQ";
    expect(youtubeVideoId(`https://www.youtube.com/watch?v=${id}`)).toBe(id);
    expect(youtubeVideoId(`https://youtu.be/${id}`)).toBe(id);
    expect(youtubeVideoId(`https://www.youtube.com/shorts/${id}`)).toBe(id);
    expect(youtubeVideoId(`https://music.youtube.com/embed/${id}`)).toBe(id);
  });

  it("builds the hqdefault thumbnail URL", () => {
    expect(youtubeThumbnailUrl("https://youtu.be/dQw4w9WgXcQ?t=12")).toBe(
      "https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg"
    );
  });

  it("rejects non-YouTube and insecure URLs", () => {
    expect(youtubeThumbnailUrl("https://youtube.com.evil.example/watch?v=dQw4w9WgXcQ")).toBeNull();
    expect(youtubeThumbnailUrl("http://www.youtube.com/watch?v=dQw4w9WgXcQ")).toBeNull();
    expect(youtubeThumbnailUrl("not a url")).toBeNull();
  });
});
