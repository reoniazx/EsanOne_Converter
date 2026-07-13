export type YoutubeDownloadIntent = "analyze" | "downloadOnly";

export function shouldAnalyzeYoutubeDownload(intent: YoutubeDownloadIntent): boolean {
  return intent === "analyze";
}

export function nextPhaseAfterYoutubeDownload(intent: YoutubeDownloadIntent): "analyzing" | "ready" {
  return shouldAnalyzeYoutubeDownload(intent) ? "analyzing" : "ready";
}
