export type DochordSearchConfig = {
  apiKey: string;
  searchEngineId: string;
};

export type DochordSearchResult = {
  title: string;
  link: string;
  snippet: string;
};

type SearchItem = {
  title?: unknown;
  link?: unknown;
  snippet?: unknown;
};

export function isDochordSearchConfigured(config: DochordSearchConfig): boolean {
  return Boolean(config.apiKey.trim() && config.searchEngineId.trim());
}

export function buildDochordSearchUrl(songTitle: string, config: DochordSearchConfig): string {
  const url = new URL("https://www.googleapis.com/customsearch/v1");
  url.searchParams.set("key", config.apiKey.trim());
  url.searchParams.set("cx", config.searchEngineId.trim());
  url.searchParams.set("q", `site:dochord.com ${songTitle.trim()}`);
  url.searchParams.set("num", "5");
  return url.toString();
}

function isDochordUrl(value: string): boolean {
  try {
    const url = new URL(value);
    return url.protocol === "https:" && (url.hostname === "dochord.com" || url.hostname === "www.dochord.com");
  } catch {
    return false;
  }
}

export function parseDochordResults(payload: unknown): DochordSearchResult[] {
  if (!payload || typeof payload !== "object" || !("items" in payload)) return [];
  const items = (payload as { items?: unknown }).items;
  if (!Array.isArray(items)) return [];

  const seen = new Set<string>();
  return items.reduce<DochordSearchResult[]>((results, item) => {
    if (!item || typeof item !== "object") return results;
    const { title, link, snippet } = item as SearchItem;
    if (typeof title !== "string" || typeof link !== "string" || !isDochordUrl(link) || seen.has(link)) {
      return results;
    }
    seen.add(link);
    results.push({
      title: title.trim() || "Dochord",
      link,
      snippet: typeof snippet === "string" ? snippet.trim() : "",
    });
    return results;
  }, []);
}

export async function searchDochord(
  songTitle: string,
  config: DochordSearchConfig,
  fetcher: typeof fetch = fetch
): Promise<DochordSearchResult[]> {
  const title = songTitle.trim();
  if (!title || !isDochordSearchConfigured(config)) return [];
  const response = await fetcher(buildDochordSearchUrl(title, config));
  if (!response.ok) throw new Error(`Google Search request failed (${response.status})`);
  return parseDochordResults(await response.json());
}
