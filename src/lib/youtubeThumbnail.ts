const YOUTUBE_HOSTS = new Set(["youtube.com", "m.youtube.com", "music.youtube.com"]);
const VIDEO_ID_PATTERN = /^[A-Za-z0-9_-]{11}$/;

function validVideoId(value: string | null | undefined): string | null {
  const id = value?.trim() ?? "";
  return VIDEO_ID_PATTERN.test(id) ? id : null;
}

export function youtubeVideoId(raw: string): string | null {
  try {
    const parsed = new URL(raw.trim());
    if (parsed.protocol !== "https:") return null;

    const host = parsed.hostname.toLowerCase().replace(/^www\./, "");
    if (host === "youtu.be") {
      return validVideoId(parsed.pathname.split("/").filter(Boolean)[0]);
    }
    if (!YOUTUBE_HOSTS.has(host)) return null;

    const queryId = validVideoId(parsed.searchParams.get("v"));
    if (queryId) return queryId;

    const segments = parsed.pathname.split("/").filter(Boolean);
    const markerIndex = segments.findIndex((segment) =>
      ["shorts", "embed", "live"].includes(segment.toLowerCase())
    );
    return markerIndex >= 0 ? validVideoId(segments[markerIndex + 1]) : null;
  } catch {
    return null;
  }
}

export function youtubeThumbnailUrl(raw: string): string | null {
  const id = youtubeVideoId(raw);
  return id ? `https://i.ytimg.com/vi/${id}/hqdefault.jpg` : null;
}
