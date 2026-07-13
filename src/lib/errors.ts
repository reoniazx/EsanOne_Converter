export type ErrorAction =
  | "download"
  | "analysis"
  | "install"
  | "gpu"
  | "separation"
  | "file"
  | "folder"
  | "shell"
  | "startup"
  | "cancel";

export function errorText(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  if (error && typeof error === "object" && "message" in error) {
    return String((error as { message?: unknown }).message ?? "");
  }
  return String(error ?? "");
}

export function friendlyErrorKey(action: ErrorAction, error: unknown): string {
  const raw = errorText(error).toLowerCase();

  if (raw.includes("cancel")) return "Operation cancelled";
  if (raw.includes("already running")) return "Another task is already running";
  if (raw.includes("valid youtube") || (action === "download" && raw.includes("youtube"))) {
    return "Enter a valid YouTube link";
  }
  if (raw.includes("permission") || raw.includes("access denied")) {
    return "File could not be accessed";
  }
  if (
    raw.includes("disk") ||
    raw.includes("space") ||
    raw.includes("output dir") ||
    raw.includes("output folder") ||
    raw.includes("cannot create output")
  ) {
    return "Could not save the result";
  }
  if (raw.includes("ffmpeg") || raw.includes("audio decoder")) {
    return "Audio decoder is missing";
  }
  if (raw.includes("essentia")) return "Essentia analysis is not available";
  if (raw.includes("python")) return "Python setup is not ready";
  if (raw.includes("weight") || raw.includes("model")) return "Model download failed";

  switch (action) {
    case "download":
      return "Download could not be completed";
    case "analysis":
      return "Analysis could not be completed";
    case "install":
      return "Tools could not be installed";
    case "gpu":
      return "GPU setup could not be completed";
    case "separation":
      return "Separation could not be completed";
    case "file":
      return "File could not be opened";
    case "folder":
      return "Folder could not be opened";
    case "shell":
      return "Shell menu setup could not be completed";
    case "cancel":
      return "Could not cancel the operation";
    case "startup":
      return "App setup could not be completed";
  }
}
