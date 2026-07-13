import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent } from "react";
import { listen } from "@tauri-apps/api/event";
import { convertFileSrc } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { motion, AnimatePresence } from "motion/react";
import {
  Copy,
  Cpu,
  Download,
  ExternalLink,
  FolderOpen,
  Gauge,
  History as HistoryIcon,
  Languages,
  Mic2,
  Music2,
  Piano,
  Upload,
  Link2,
  Check,
  X,
  Loader2,
  Minus,
  RefreshCw,
  Search,
  Square,
  Trash2,
  Info,
} from "lucide-react";
import { useLocale, useT } from "./lib/i18n/I18nProvider";
import {
  canEnableStemGpu,
  createStemProgressTracker,
  purposeForStemModel,
  resolveStemBackend,
  stemExecutionDeviceLabel,
  stemReadyForPurpose,
  type StemDevicePreference,
  type StemSeparationPurpose,
  type StemToolStatus,
} from "./lib/stemSeparate";
import {
  emptyStemResults,
  rememberStemPaths,
  stemPathsForPurpose,
  type StemResultsByPurpose,
} from "./lib/stemResults";
import {
  analyzeAudio,
  analyzeCancel,
  defaultOutputDir,
  getStartupCli,
  revealInFolder,
  shellMenuRegister,
  shellMenuStatus,
  shellMenuUnregister,
  parseStartupCliArgs,
  stemGpuInstall,
  stemSeparateStart,
  stemSeparateStop,
  stemToolInstall,
  stemToolStatus,
  youtubeDownload,
  youtubeCancel,
  type AnalyzeResult,
  type ShellMenuStatus,
} from "./lib/tauriApi";
import { isCurrentSessionEvent } from "./lib/sessionEvents";
import { formatKeyBpmToast, type KeyBpmToast } from "./lib/keyBpmNotification";
import { youtubeThumbnailUrl } from "./lib/youtubeThumbnail";
import {
  nextPhaseAfterYoutubeDownload,
  shouldAnalyzeYoutubeDownload,
  type YoutubeDownloadIntent,
} from "./lib/youtubeDownloadFlow";
import {
  isDochordSearchConfigured,
  searchDochord,
  type DochordSearchResult,
} from "./lib/dochordSearch";
import {
  loadHistory,
  saveHistory,
  upsertHistory,
  type TrackHistoryItem,
} from "./lib/history";
import { friendlyErrorKey, type ErrorAction } from "./lib/errors";
import {
  checkForAppUpdate,
  createIdleUpdateStatus,
  downloadAndInstallUpdate,
  loadAppVersion,
  relaunchApp,
  updateErrorKey,
  type UpdateStatus,
} from "./lib/appUpdate";
import type { Update } from "@tauri-apps/plugin-updater";
import esanoneLogo from "./assets/esanone-logo.png";
import { WaveformAudio } from "./components/WaveformAudio";
import "./App.css";

type Phase = "idle" | "fetching" | "analyzing" | "ready" | "installing" | "separating" | "done";

type StemProgressEvent = {
  sessionId: string;
  kind: string;
  text?: string;
  progress?: number;
};

const dochordSearchConfig = {
  apiKey: String(import.meta.env.VITE_GOOGLE_CUSTOM_SEARCH_API_KEY ?? "").trim(),
  searchEngineId: String(import.meta.env.VITE_GOOGLE_CUSTOM_SEARCH_CX ?? "").trim(),
};

function normalizeProgress(value: number): number {
  return Math.min(1, Math.max(0, value));
}

function newSessionId(prefix: string): string {
  return `${prefix}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

function ProgressRing({ value }: { value: number }) {
  const r = 42;
  const c = 2 * Math.PI * r;
  const offset = c * (1 - Math.min(1, Math.max(0, value)));
  return (
    <svg className="ring" viewBox="0 0 96 96" aria-hidden>
      <circle className="ringTrack" cx="48" cy="48" r={r} />
      <circle
        className="ringValue"
        cx="48"
        cy="48"
        r={r}
        strokeDasharray={c}
        strokeDashoffset={offset}
      />
    </svg>
  );
}

export default function App() {
  const t = useT();
  const appWindow = useMemo(() => getCurrentWindow(), []);
  const [locale, setLocale] = useLocale();
  const [url, setUrl] = useState("");
  const [dragOver, setDragOver] = useState(false);
  const [phase, setPhase] = useState<Phase>("idle");
  const [status, setStatus] = useState<StemToolStatus | null>(null);
  const [purpose, setPurpose] = useState<StemSeparationPurpose>("vocals");
  const [devicePref, setDevicePref] = useState<StemDevicePreference>("auto");
  const [audioPath, setAudioPath] = useState<string | null>(null);
  const [title, setTitle] = useState<string | null>(null);
  const [thumbnailUrl, setThumbnailUrl] = useState<string | null>(null);
  const [youtubeTrack, setYoutubeTrack] = useState(false);
  const [dochordResults, setDochordResults] = useState<DochordSearchResult[]>([]);
  const [dochordSearchPhase, setDochordSearchPhase] = useState<"idle" | "searching" | "done" | "error">("idle");
  const [analysis, setAnalysis] = useState<AnalyzeResult | null>(null);
  const [outputDir, setOutputDir] = useState<string>("");
  const [flattenOutput, setFlattenOutput] = useState(false);
  const [notifyAnalysis, setNotifyAnalysis] = useState(false);
  const [keyBpmToast, setKeyBpmToast] = useState<KeyBpmToast | null>(null);
  const [log, setLog] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState(0);
  const [stemResults, setStemResults] = useState<StemResultsByPurpose>(() => emptyStemResults());
  const [historyItems, setHistoryItems] = useState<TrackHistoryItem[]>(() => loadHistory());
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [shell, setShell] = useState<ShellMenuStatus | null>(null);
  const [updateStatus, setUpdateStatus] = useState<UpdateStatus>(createIdleUpdateStatus);
  const [isMaximized, setIsMaximized] = useState(false);
  const pendingUpdateRef = useRef<Update | null>(null);
  const activeSessionIdRef = useRef<string | null>(null);
  const pendingThumbnailRef = useRef<string | null>(null);
  const downloadOnlyRef = useRef(false);
  const toastTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const beginSession = useCallback((prefix: string): string => {
    const id = newSessionId(prefix);
    activeSessionIdRef.current = id;
    setSessionId(id);
    return id;
  }, []);

  const clearActiveSession = useCallback((): void => {
    activeSessionIdRef.current = null;
    setSessionId(null);
  }, []);

  const showKeyBpmToast = useCallback(
    (result: AnalyzeResult): void => {
      if (toastTimerRef.current) clearTimeout(toastTimerRef.current);
      setKeyBpmToast(formatKeyBpmToast(result, t("Key"), t("BPM"), t("Analysis complete")));
      toastTimerRef.current = setTimeout(() => {
        setKeyBpmToast(null);
        toastTimerRef.current = null;
      }, 5000);
    },
    [t]
  );

  useEffect(() => {
    return () => {
      if (toastTimerRef.current) clearTimeout(toastTimerRef.current);
    };
  }, []);

  const showError = useCallback(
    (action: ErrorAction, cause: unknown): void => {
      setError(t(friendlyErrorKey(action, cause)));
    },
    [t]
  );

  useEffect(() => {
    saveHistory(historyItems);
  }, [historyItems]);

  const historySnapshot = useMemo<TrackHistoryItem | null>(() => {
    if (!audioPath || !title) return null;
    return {
      id: audioPath,
      title,
      audioPath,
      thumbnailUrl,
      youtubeTrack,
      analysis,
      stemResults,
      purpose,
      updatedAt: Date.now(),
    };
  }, [analysis, audioPath, purpose, stemResults, thumbnailUrl, title, youtubeTrack]);

  useEffect(() => {
    if (!historySnapshot) return;
    setHistoryItems((previous) => {
      const next = upsertHistory(previous, historySnapshot);
      return JSON.stringify(previous) === JSON.stringify(next) ? previous : next;
    });
  }, [historySnapshot]);

  const searchDochordForTitle = useCallback(async (songTitle: string): Promise<void> => {
    setDochordResults([]);
    if (!isDochordSearchConfigured(dochordSearchConfig)) {
      setDochordSearchPhase("idle");
      return;
    }
    setDochordSearchPhase("searching");
    try {
      setDochordResults(await searchDochord(songTitle, dochordSearchConfig));
      setDochordSearchPhase("done");
    } catch (cause) {
      console.warn("Dochord search failed", cause);
      setDochordSearchPhase("error");
    }
  }, []);

  const onOpenExternalUrl = useCallback(
    async (target: string): Promise<void> => {
      try {
        await openUrl(target);
      } catch (cause) {
        showError("startup", cause);
      }
    },
    [showError]
  );

  const refreshWindowState = useCallback(async () => {
    try {
      setIsMaximized(await appWindow.isMaximized());
    } catch (cause) {
      console.warn("Could not read window state", cause);
    }
  }, [appWindow]);

  useEffect(() => {
    void refreshWindowState();
    let unlisten: (() => void) | undefined;
    void appWindow
      .onResized(() => {
        void refreshWindowState();
      })
      .then((off) => {
        unlisten = off;
      })
      .catch((cause) => console.warn("Could not listen for window resize", cause));
    return () => unlisten?.();
  }, [appWindow, refreshWindowState]);

  const onMinimize = useCallback(async () => {
    try {
      await appWindow.minimize();
    } catch (cause) {
      showError("startup", cause);
    }
  }, [appWindow, showError]);

  const onToggleMaximize = useCallback(async () => {
    try {
      await appWindow.toggleMaximize();
      await refreshWindowState();
    } catch (cause) {
      showError("startup", cause);
    }
  }, [appWindow, refreshWindowState, showError]);

  const onClose = useCallback(async () => {
    try {
      await appWindow.close();
    } catch (cause) {
      showError("startup", cause);
    }
  }, [appWindow, showError]);

  const onWindowBarMouseDown = useCallback(
    (event: MouseEvent<HTMLElement>) => {
      if (event.button !== 0) return;
      const target = event.target as HTMLElement;
      if (target.closest("button, input, textarea, select, a, [data-no-window-drag]")) return;
      if (event.detail === 2) {
        void onToggleMaximize();
        return;
      }
      void appWindow.startDragging().catch((cause) => showError("startup", cause));
    },
    [appWindow, onToggleMaximize, showError]
  );

  const failAction = useCallback(
    (action: ErrorAction, cause: unknown, fallbackPhase?: Phase): void => {
      showError(action, cause);
      clearActiveSession();
      setPhase(fallbackPhase ?? (audioPath ? "ready" : "idle"));
    },
    [audioPath, clearActiveSession, showError]
  );

  const backend = resolveStemBackend(purpose);
  const ready = stemReadyForPurpose(status, purpose);
  const gpuOffer = canEnableStemGpu(status, purpose);
  const busy = ["fetching", "analyzing", "installing", "separating"].includes(phase);
  const stemPaths = stemPathsForPurpose(stemResults, purpose);

  const selectPurpose = useCallback(
    (nextPurpose: StemSeparationPurpose): void => {
      if (busy) return;
      setPurpose(nextPurpose);
      setPhase(stemPathsForPurpose(stemResults, nextPurpose).length > 0 ? "done" : audioPath ? "ready" : "idle");
      setProgress(0);
      setLog("");
      setError(null);
    },
    [audioPath, busy, stemResults]
  );

  const openHistoryItem = useCallback(
    (item: TrackHistoryItem): void => {
      if (busy) return;
      clearActiveSession();
      setUrl("");
      setError(null);
      setLog("");
      setProgress(0);
      setStemResults(item.stemResults);
      setAnalysis(item.analysis);
      setAudioPath(item.audioPath);
      setTitle(item.title);
      setThumbnailUrl(item.thumbnailUrl);
      setYoutubeTrack(item.youtubeTrack);
      setPurpose(item.purpose);
      setDochordResults([]);
      setDochordSearchPhase("idle");
      setNotifyAnalysis(false);
      setPhase(stemPathsForPurpose(item.stemResults, item.purpose).length > 0 ? "done" : "ready");
      if (item.youtubeTrack) void searchDochordForTitle(item.title);
    },
    [busy, clearActiveSession, searchDochordForTitle]
  );

  const clearHistory = useCallback((): void => {
    setHistoryItems([]);
  }, []);

  const refreshStatus = useCallback(async () => {
    try {
      setStatus(await stemToolStatus());
    } catch (cause) {
      showError("startup", cause);
    }
  }, [showError]);

  const runUpdateCheck = useCallback(async (silent = false) => {
    setUpdateStatus((prev) => ({
      ...prev,
      phase: "checking",
      error: null,
      progress: null,
    }));
    try {
      const { update, currentVersion } = await checkForAppUpdate();
      pendingUpdateRef.current = update;
      if (update) {
        setUpdateStatus({
          phase: "available",
          currentVersion,
          availableVersion: update.version,
          progress: null,
          error: null,
        });
      } else {
        setUpdateStatus({
          phase: silent ? "idle" : "upToDate",
          currentVersion,
          availableVersion: null,
          progress: null,
          error: null,
        });
      }
    } catch (cause) {
      pendingUpdateRef.current = null;
      const currentVersion = await loadAppVersion();
      setUpdateStatus({
        phase: silent ? "idle" : "error",
        currentVersion,
        availableVersion: null,
        progress: null,
        error: silent ? null : t(updateErrorKey("check")),
      });
      if (!silent) {
        console.warn("Update check failed", cause);
      }
    }
  }, [t]);

  const onUpdateApp = useCallback(async () => {
    if (
      updateStatus.phase === "downloading" ||
      updateStatus.phase === "installing" ||
      updateStatus.phase === "checking"
    ) {
      return;
    }

    if (updateStatus.phase === "restartRequired") {
      try {
        await relaunchApp();
      } catch (cause) {
        console.warn("Update installed but relaunch failed", cause);
        setUpdateStatus((prev) => ({
          ...prev,
          phase: "restartRequired",
          progress: null,
          error: t(updateErrorKey("restart")),
        }));
      }
      return;
    }

    let update = pendingUpdateRef.current;
    if (!update) {
      await runUpdateCheck(false);
      update = pendingUpdateRef.current;
      if (!update) return;
    }

    setUpdateStatus((prev) => ({
      ...prev,
      phase: "downloading",
      progress: 0,
      error: null,
    }));

    let lastPhase: "downloading" | "installing" = "downloading";
    try {
      await downloadAndInstallUpdate(update, (progress, phase) => {
        lastPhase = phase;
        setUpdateStatus((prev) => ({
          ...prev,
          phase,
          progress,
          error: null,
        }));
      });
      setUpdateStatus((prev) => ({
        ...prev,
        phase: "installing",
        progress: 1,
        error: null,
      }));
      try {
        await relaunchApp();
      } catch (cause) {
        console.warn("Update installed but relaunch failed", cause);
        setUpdateStatus((prev) => ({
          ...prev,
          phase: "restartRequired",
          progress: null,
          error: t(updateErrorKey("restart")),
        }));
      }
    } catch (cause) {
      console.warn("Update install failed", cause);
      setUpdateStatus((prev) => ({
        ...prev,
        phase: "error",
        progress: null,
        error: t(updateErrorKey(lastPhase === "installing" ? "install" : "download")),
      }));
    }
  }, [runUpdateCheck, t, updateStatus.phase]);

  useEffect(() => {
    void (async () => {
      try {
        setOutputDir(await defaultOutputDir());
        const version = await loadAppVersion();
        setUpdateStatus(createIdleUpdateStatus(version));
        void runUpdateCheck(true);
        await refreshStatus();
        try {
          setShell(await shellMenuStatus());
        } catch (cause) {
          showError("startup", cause);
        }
        const cli = await getStartupCli();
        if (cli.analyze?.path) {
          pendingThumbnailRef.current = null;
          setStemResults(() => emptyStemResults());
          setYoutubeTrack(false);
          setDochordResults([]);
          setDochordSearchPhase("idle");
          setAudioPath(cli.analyze.path);
          setTitle(cli.analyze.path.split(/[/\\]/).pop() ?? "Audio");
          setThumbnailUrl(null);
          setPhase("analyzing");
          setNotifyAnalysis(true);
          const sid = beginSession("analyze-essentia");
          try {
            await analyzeAudio(sid, cli.analyze.path);
          } catch (cause) {
            failAction("analysis", cause, "ready");
          }
        } else if (cli.separate?.path) {
          pendingThumbnailRef.current = null;
          setStemResults(() => emptyStemResults());
          setYoutubeTrack(false);
          setDochordResults([]);
          setDochordSearchPhase("idle");
          setAudioPath(cli.separate.path);
          setTitle(cli.separate.path.split(/[/\\]/).pop() ?? "Audio");
          setThumbnailUrl(null);
          if (cli.separate.outputDir) setOutputDir(cli.separate.outputDir);
          setFlattenOutput(Boolean(cli.separate.flattenOutput));
          setNotifyAnalysis(false);
          const startupPurpose = purposeForStemModel(cli.separate.model);
          if (!startupPurpose) {
            setError(t("Unsupported separation model"));
            return;
          }
          setPurpose(startupPurpose);
          setPhase("analyzing");
          const sid = beginSession("analyze");
          try {
            await analyzeAudio(sid, cli.separate.path);
          } catch (cause) {
            failAction("analysis", cause, "ready");
          }
        }
      } catch (cause) {
        failAction("startup", cause);
      }
    })();
  }, [analyzeAudio, beginSession, failAction, refreshStatus, runUpdateCheck, showError, t]);

  useEffect(() => {
    let unsubscribe: (() => void) | undefined;
    void listen<{ args: string[]; cwd: string }>("single-instance", async (event) => {
      const startup = parseStartupCliArgs(event.payload.args);
      if (startup.analyze?.path) {
        pendingThumbnailRef.current = null;
        setStemResults(() => emptyStemResults());
        setYoutubeTrack(false);
        setDochordResults([]);
        setDochordSearchPhase("idle");
        setAudioPath(startup.analyze.path);
        setTitle(startup.analyze.path.split(/[/\\]/).pop() ?? "Audio");
        setThumbnailUrl(null);
        setError(null);
        setPhase("analyzing");
        setNotifyAnalysis(true);
        const sid = beginSession("analyze-essentia");
        try {
          await analyzeAudio(sid, startup.analyze.path);
        } catch (cause) {
          failAction("analysis", cause, "ready");
        }
        return;
      }
      const request = startup.separate;
      if (!request) return;
      const startupPurpose = purposeForStemModel(request.model);
      if (!startupPurpose) return;
      pendingThumbnailRef.current = null;
      setStemResults(() => emptyStemResults());
      setYoutubeTrack(false);
      setDochordResults([]);
      setDochordSearchPhase("idle");
      setAudioPath(request.path);
      setTitle(request.path.split(/[/\\]/).pop() ?? "Audio");
      setThumbnailUrl(null);
      if (request.outputDir) setOutputDir(request.outputDir);
      setFlattenOutput(Boolean(request.flattenOutput));
      setNotifyAnalysis(false);
      setError(null);
      setPhase("analyzing");
      const sid = beginSession("analyze");
      try {
        await analyzeAudio(sid, request.path);
      } catch (cause) {
        failAction("analysis", cause, "ready");
      }
    })
      .then((off) => {
        unsubscribe = off;
      })
      .catch((cause) => showError("startup", cause));
    return () => unsubscribe?.();
  }, [analyzeAudio, beginSession, failAction]);

  useEffect(() => {
    const unsubs: Array<() => void> = [];
    void (async () => {
      unsubs.push(
        await listen<{ sessionId: string; kind: string; text?: string }>(
          "youtube-download-event",
          (e) => {
            if (!isCurrentSessionEvent(activeSessionIdRef.current, e.payload)) return;
            if (e.payload.text) setLog((prev) => `${prev}${e.payload.text}\n`);
          }
        )
      );
      unsubs.push(
        await listen<{
          sessionId: string;
          ok: boolean;
          error?: string;
          path?: string;
          title?: string;
        }>("youtube-download-done", async (e) => {
          if (!isCurrentSessionEvent(activeSessionIdRef.current, e.payload)) return;
          if (!e.payload.ok || !e.payload.path) {
            pendingThumbnailRef.current = null;
            downloadOnlyRef.current = false;
            setYoutubeTrack(false);
            setDochordResults([]);
            setDochordSearchPhase("idle");
            failAction("download", e.payload.error ?? "Download failed", "idle");
            return;
          }
          const downloadIntent: YoutubeDownloadIntent = downloadOnlyRef.current ? "downloadOnly" : "analyze";
          downloadOnlyRef.current = false;
          const shouldAnalyze = shouldAnalyzeYoutubeDownload(downloadIntent);
          const nextTitle = e.payload.title ?? e.payload.path.split(/[/\\]/).pop() ?? "Audio";
          setAudioPath(e.payload.path);
          setTitle(nextTitle);
          setYoutubeTrack(shouldAnalyze);
          setThumbnailUrl(pendingThumbnailRef.current);
          pendingThumbnailRef.current = null;
          if (shouldAnalyze) void searchDochordForTitle(nextTitle);
          setPhase(nextPhaseAfterYoutubeDownload(downloadIntent));
          if (!shouldAnalyze) {
            clearActiveSession();
            return;
          }
          const sid = beginSession("analyze");
          try {
            await analyzeAudio(sid, e.payload.path);
          } catch (cause) {
            failAction("analysis", cause, "ready");
          }
        })
      );
      unsubs.push(
        await listen<{ sessionId: string; kind: string; text?: string }>(
          "analyze-audio-event",
          (e) => {
            if (!isCurrentSessionEvent(activeSessionIdRef.current, e.payload)) return;
            if (e.payload.text) setLog((prev) => `${prev}${e.payload.text}\n`);
          }
        )
      );
      unsubs.push(
        await listen<{
          sessionId: string;
          ok: boolean;
          error?: string;
          result?: AnalyzeResult;
        }>("analyze-audio-done", (e) => {
          if (!isCurrentSessionEvent(activeSessionIdRef.current, e.payload)) return;
          if (!e.payload.ok || !e.payload.result) {
            failAction("analysis", e.payload.error ?? "Analysis failed");
            return;
          }
          setAnalysis(e.payload.result);
          if (notifyAnalysis) showKeyBpmToast(e.payload.result);
          clearActiveSession();
          setPhase("ready");
        })
      );
      unsubs.push(
        await listen<StemProgressEvent>(
          "stem-tool-install-event",
          (e) => {
            if (!isCurrentSessionEvent(activeSessionIdRef.current, e.payload)) return;
            if (e.payload.text) setLog((prev) => `${prev}${e.payload.text}\n`);
            if (typeof e.payload.progress === "number") {
              setProgress(normalizeProgress(e.payload.progress));
            }
          }
        )
      );
      unsubs.push(
        await listen<{
          sessionId: string;
          ok: boolean;
          error?: string;
          status: StemToolStatus;
        }>("stem-tool-install-done", (e) => {
          if (!isCurrentSessionEvent(activeSessionIdRef.current, e.payload)) return;
          setStatus(e.payload.status);
          if (!e.payload.ok) {
            failAction("install", e.payload.error ?? "Install failed", "ready");
            return;
          }
          setProgress(1);
          clearActiveSession();
          setPhase("ready");
        })
      );
      unsubs.push(
        await listen<StemProgressEvent>(
          "stem-gpu-install-event",
          (e) => {
            if (!isCurrentSessionEvent(activeSessionIdRef.current, e.payload)) return;
            if (e.payload.text) setLog((prev) => `${prev}${e.payload.text}\n`);
            if (typeof e.payload.progress === "number") {
              setProgress(normalizeProgress(e.payload.progress));
            }
          }
        )
      );
      unsubs.push(
        await listen<{
          sessionId: string;
          ok: boolean;
          error?: string;
          status: StemToolStatus;
        }>("stem-gpu-install-done", (e) => {
          if (!isCurrentSessionEvent(activeSessionIdRef.current, e.payload)) return;
          setStatus(e.payload.status);
          if (!e.payload.ok) {
            failAction("gpu", e.payload.error ?? "GPU setup failed", "ready");
            return;
          }
          setProgress(1);
          clearActiveSession();
          setPhase("ready");
        })
      );

      const tracker = { current: createStemProgressTracker(backend.progressPasses) };
      unsubs.push(
        await listen<StemProgressEvent>(
          "stem-separate-event",
          (e) => {
            if (!isCurrentSessionEvent(activeSessionIdRef.current, e.payload)) return;
            if (e.payload.text) {
              setLog((prev) => `${prev}${e.payload.text}\n`);
              if (typeof e.payload.progress === "number") {
                setProgress(normalizeProgress(e.payload.progress));
              } else {
                const next = tracker.current.ingest(e.payload.text);
                if (next != null) setProgress(next);
              }
            }
          }
        )
      );
      unsubs.push(
        await listen<{
          sessionId: string;
          ok: boolean;
          error?: string;
          stemPaths?: string[];
        }>("stem-separate-done", (e) => {
          if (!isCurrentSessionEvent(activeSessionIdRef.current, e.payload)) return;
          if (!e.payload.ok) {
            failAction("separation", e.payload.error ?? "Separation failed", "ready");
            return;
          }
          setStemResults((previous) => rememberStemPaths(previous, purpose, e.payload.stemPaths ?? []));
          setProgress(1);
          clearActiveSession();
          setPhase("done");
        })
      );
    })().catch((cause) => showError("startup", cause));
    return () => unsubs.forEach((u) => u());
  }, [audioPath, backend.progressPasses, beginSession, clearActiveSession, failAction, notifyAnalysis, purpose, searchDochordForTitle, showError, showKeyBpmToast]);

  const onPickFile = async () => {
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: "Audio", extensions: ["wav", "mp3", "flac", "ogg", "m4a"] }],
      });
      if (typeof selected !== "string") return;
      await ingestLocal(selected);
    } catch (cause) {
      failAction("file", cause);
    }
  };

  const ingestLocal = async (path: string) => {
    setError(null);
    setLog("");
    setStemResults(() => emptyStemResults());
    setAnalysis(null);
    setAudioPath(path);
    setYoutubeTrack(false);
    setDochordResults([]);
    setDochordSearchPhase("idle");
    pendingThumbnailRef.current = null;
    setThumbnailUrl(null);
    setFlattenOutput(false);
    setNotifyAnalysis(false);
    setTitle(path.split(/[/\\]/).pop() ?? "Audio");
    setPhase("analyzing");
    const sid = beginSession("analyze");
    try {
      await analyzeAudio(sid, path);
    } catch (cause) {
      failAction("analysis", cause, "ready");
    }
  };

  const onYoutubeDownload = async (intent: YoutubeDownloadIntent) => {
    const sourceUrl = url.trim();
    if (!sourceUrl) return;
    downloadOnlyRef.current = intent === "downloadOnly";
    setError(null);
    setLog("");
    setStemResults(() => emptyStemResults());
    setAnalysis(null);
    setYoutubeTrack(false);
    setDochordResults([]);
    setDochordSearchPhase("idle");
    pendingThumbnailRef.current = youtubeThumbnailUrl(sourceUrl);
    setNotifyAnalysis(false);
    setPhase("fetching");
    const sid = beginSession("yt");
    try {
      const out = outputDir || (await defaultOutputDir());
      await youtubeDownload({ sessionId: sid, url: sourceUrl, outputDir: out });
    } catch (cause) {
      pendingThumbnailRef.current = null;
      downloadOnlyRef.current = false;
      failAction("download", cause, "idle");
    }
  };

  const onFetchYoutube = async () => {
    await onYoutubeDownload("analyze");
  };

  const onDownloadYoutube = async () => {
    await onYoutubeDownload("downloadOnly");
  };

  const onInstall = async () => {
    setError(null);
    setLog("");
    setProgress(0);
    setPhase("installing");
    const sid = beginSession("install");
    try {
      await stemToolInstall(sid, purpose);
    } catch (cause) {
      failAction("install", cause, "ready");
    }
  };

  const onEnableGpu = async () => {
    setError(null);
    setLog("");
    setProgress(0);
    setPhase("installing");
    const sid = beginSession("gpu");
    try {
      await stemGpuInstall(sid, "cuda");
    } catch (cause) {
      failAction("gpu", cause, "ready");
    }
  };

  const onSeparate = async () => {
    if (!audioPath) return;
    if (!ready) {
      await onInstall();
      return;
    }
    setError(null);
    setLog("");
    setStemResults((previous) => rememberStemPaths(previous, purpose, []));
    setProgress(0);
    setPhase("separating");
    const sid = beginSession("sep");
    try {
      const out = outputDir || (await defaultOutputDir());
      await stemSeparateStart({
        inputPath: audioPath,
        outputDir: out,
        sessionId: sid,
        model: backend.model,
        devicePreference: devicePref === "cpu" ? "cpu" : null,
        flattenOutput,
      });
    } catch (cause) {
      failAction("separation", cause, "ready");
    }
  };

  const onCancel = async () => {
    try {
      if (sessionId && phase === "fetching") {
        await youtubeCancel();
      } else if (sessionId && phase === "analyzing") {
        await analyzeCancel();
      } else if (sessionId && (phase === "separating" || phase === "installing")) {
        await stemSeparateStop(sessionId);
      }
    } catch (cause) {
      failAction("cancel", cause);
      return;
    }
    clearActiveSession();
    if (phase === "fetching") {
      pendingThumbnailRef.current = null;
      downloadOnlyRef.current = false;
    }
    setPhase(audioPath ? "ready" : "idle");
  };

  const onRevealInFolder = async (path: string) => {
    try {
      await revealInFolder(path);
    } catch (cause) {
      showError("folder", cause);
    }
  };

  const deviceLabel = useMemo(
    () => stemExecutionDeviceLabel(status, devicePref),
    [status, devicePref]
  );

  const durationLabel = analysis
    ? `${Math.floor(analysis.durationSec / 60)}:${String(Math.floor(analysis.durationSec % 60)).padStart(2, "0")}`
    : "—";

  const modeDetails = {
    vocals: {
      model: t("Vocals (MelBandRoFormer)"),
      stems: t("Vocals and instrumental"),
      advantages: t("High-quality vocal isolation for karaoke and remixing"),
      limitations: t("Does not split individual instruments"),
    },
    instruments: {
      model: t("Instruments 4 (HT-Demucs)"),
      stems: t("Vocals, drums, bass, other"),
      advantages: t("Balanced four-stem separation for general mixing"),
      limitations: t("Vocal and instrument bleed may remain"),
    },
    instruments_6s: {
      model: t("Instruments 6 (HT-Demucs 6s)"),
      stems: t("Vocals, drums, bass, other, guitar, piano"),
      advantages: t("Adds guitar and piano stems for more control"),
      limitations: t("Slower and uses more GPU, CPU, and disk space"),
    },
  }[purpose];

  const dochordSearchConfigured = isDochordSearchConfigured(dochordSearchConfig);
  const googleDochordSearchUrl = title
    ? `https://www.google.com/search?q=${encodeURIComponent(`site:dochord.com ${title}`)}`
    : null;
  const hasAnyStemResults = Object.values(stemResults).some((paths) => paths.length > 0);

  return (
    <div className="shell">
      {keyBpmToast ? (
        <div className="toastViewport">
          <div className="toast" role="status" aria-live="polite">
            <div className="toastHeader">
              <span className="toastTitle">{keyBpmToast.title}</span>
              <button
                type="button"
                className="toastClose"
                aria-label={t("Close")}
                onClick={() => setKeyBpmToast(null)}
              >
                <X size={16} />
              </button>
            </div>
            <div className="toastBody">{keyBpmToast.body}</div>
          </div>
      </div>
      ) : null}
      <header
        className="windowBar"
        data-tauri-drag-region
        onMouseDown={onWindowBarMouseDown}
        onDoubleClick={(event) => {
          if ((event.target as HTMLElement).closest("button, input, textarea, select, a")) return;
          void onToggleMaximize();
        }}
      >
        <div className="brand" data-tauri-drag-region>
          <span className="brandMark" data-tauri-drag-region>
            <img src={esanoneLogo} alt="" aria-hidden="true" />
          </span>
          <span data-tauri-drag-region>{t("EsanOne Converter")}</span>
        </div>
        <div className="topActions">
          <div className="navStatus">
            <div className="navStatusRow">
              <div className="statusRow navStatusText">
                <span
                  className={`dot ${
                    updateStatus.phase === "available" || updateStatus.phase === "restartRequired"
                      ? "dotOk"
                      : updateStatus.phase === "error"
                        ? "dotErr"
                        : updateStatus.phase === "downloading" ||
                            updateStatus.phase === "installing" ||
                            updateStatus.phase === "checking"
                          ? "dotBusy"
                          : ""
                  }`}
                />
                <span>
                  {updateStatus.phase === "checking"
                    ? t("Checking for updates…")
                    : updateStatus.phase === "downloading"
                      ? t("Downloading update…")
                      : updateStatus.phase === "installing"
                        ? t("Installing update…")
                        : updateStatus.phase === "restartRequired"
                          ? updateStatus.error ?? t("Update installed — restart required")
                          : updateStatus.phase === "available" && updateStatus.availableVersion
                            ? `${t("Update available")} · v${updateStatus.availableVersion}`
                            : updateStatus.phase === "upToDate"
                              ? t("You're up to date")
                              : updateStatus.error
                                ? updateStatus.error
                                : updateStatus.currentVersion
                                  ? `${t("Version")} ${updateStatus.currentVersion}`
                                  : t("Check for updates")}
                </span>
                {updateStatus.phase === "downloading" && updateStatus.progress != null ? (
                  <span className="updatePct">{Math.round(updateStatus.progress * 100)}%</span>
                ) : null}
              </div>
              <button
                type="button"
                className={updateStatus.phase === "available" ? "primaryBtn" : "ghostBtn"}
                disabled={
                  updateStatus.phase === "checking" ||
                  updateStatus.phase === "downloading" ||
                  updateStatus.phase === "installing" ||
                  busy
                }
                onClick={() => void onUpdateApp()}
              >
                {updateStatus.phase === "checking" ||
                updateStatus.phase === "downloading" ||
                updateStatus.phase === "installing" ? (
                  <Loader2 size={16} className="spin" />
                ) : (
                  <RefreshCw size={16} />
                )}
                {updateStatus.phase === "available"
                  ? t("Update")
                  : updateStatus.phase === "restartRequired"
                    ? t("Restart app")
                    : updateStatus.phase === "error"
                      ? t("Retry update")
                      : t("Check for updates")}
              </button>
            </div>
            <div className="navStatusRow">
              <div className="statusRow navStatusText">
                <span className={`dot ${shell?.registered ? "dotOk" : ""}`} />
                <span>{shell?.detail ?? t("Shell menus")}</span>
              </div>
              <button
                type="button"
                className="ghostBtn"
                disabled={shell?.available === false}
                onClick={async () => {
                  try {
                    const next = shell?.registered
                      ? await shellMenuUnregister()
                      : await shellMenuRegister();
                    setShell(next);
                  } catch (cause) {
                    showError("shell", cause);
                  }
                }}
              >
                {shell?.registered ? t("Unregister") : t("Enable right-click menus")}
              </button>
            </div>
          </div>
          <button
            type="button"
            className="iconBtn"
            aria-label={t("Device")}
            title={deviceLabel}
            onClick={() => setDevicePref((p) => (p === "auto" ? "cpu" : "auto"))}
          >
            {devicePref === "cpu" || !status?.gpu.cudaAvailable ? (
              <Cpu size={18} />
            ) : (
              <Gauge size={18} />
            )}
          </button>
          <button
            type="button"
            className="iconBtn"
            aria-pressed={locale === "th"}
            aria-label={t(locale === "th" ? "Switch to English" : "Switch to Thai")}
            title={t(locale === "th" ? "Switch to English" : "Switch to Thai")}
            onClick={() => setLocale(locale === "th" ? "en" : "th")}
          >
            <Languages size={18} />
          </button>
          <div className="windowControls">
            <button
              type="button"
              className="windowBtn"
              aria-label={t("Minimize window")}
              title={t("Minimize window")}
              onClick={() => void onMinimize()}
            >
              <Minus size={16} />
            </button>
            <button
              type="button"
              className="windowBtn"
              aria-label={t(isMaximized ? "Restore window" : "Maximize window")}
              title={t(isMaximized ? "Restore window" : "Maximize window")}
              onClick={() => void onToggleMaximize()}
            >
              {isMaximized ? <Copy size={14} /> : <Square size={14} />}
            </button>
            <button
              type="button"
              className="windowBtn windowBtnClose"
              aria-label={t("Close window")}
              title={t("Close window")}
              onClick={() => void onClose()}
            >
              <X size={16} />
            </button>
          </div>
        </div>
      </header>

      <div className="appWorkspace">
        <aside className="historySidebar" aria-label={t("History")}>
          <div className="historyHeader">
            <div className="historyHeading">
              <HistoryIcon size={17} aria-hidden="true" />
              <strong>{t("History")}</strong>
            </div>
            {historyItems.length > 0 ? (
              <button
                type="button"
                className="iconBtn historyClearBtn"
                aria-label={t("Clear history")}
                title={t("Clear history")}
                disabled={busy}
                onClick={clearHistory}
              >
                <Trash2 size={15} aria-hidden="true" />
              </button>
            ) : null}
          </div>
          {historyItems.length > 0 ? (
            <div className="historyList">
              {historyItems.map((item) => (
                <button
                  type="button"
                  className={`historyItem ${item.id === audioPath ? "historyItemActive" : ""}`}
                  key={item.id}
                  title={item.title}
                  disabled={busy}
                  onClick={() => openHistoryItem(item)}
                >
                  <span className="historyItemIcon" aria-hidden="true">
                    <Music2 size={16} />
                  </span>
                  <span className="historyItemText">
                    <strong>{item.title}</strong>
                    {item.analysis ? (
                      <small>
                        {item.analysis.key} · {Math.round(item.analysis.bpm)} BPM
                      </small>
                    ) : null}
                  </span>
                </button>
              ))}
            </div>
          ) : (
            <div className="historyEmpty">
              <HistoryIcon size={20} aria-hidden="true" />
              <span>{t("No history yet")}</span>
            </div>
          )}
        </aside>

      <main className="main">
        <section className="hero">
          <p className="heroSub">{t("Paste a YouTube link or drop an audio file")}</p>
        </section>

        <div className="urlRow">
          <input
            className="urlInput"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder={t("YouTube URL")}
            aria-label={t("YouTube URL")}
            disabled={busy}
            onKeyDown={(e) => {
              if (e.key === "Enter") void onFetchYoutube();
            }}
          />
          <button
            type="button"
            className="primaryBtn"
            disabled={busy || !url.trim()}
            onClick={() => void onFetchYoutube()}
          >
            <Link2 size={18} />
            {t("Separate audio")}
          </button>
          <button
            type="button"
            className="ghostBtn"
            disabled={busy || !url.trim()}
            onClick={() => void onDownloadYoutube()}
          >
            <Download size={18} />
            {t("Download song")}
          </button>
        </div>

        <div
          className={`drop ${dragOver ? "dropActive" : ""}`}
          onDragOver={(e) => {
            e.preventDefault();
            setDragOver(true);
          }}
          onDragLeave={() => setDragOver(false)}
          onDrop={(e) => {
            e.preventDefault();
            setDragOver(false);
            const file = e.dataTransfer.files?.[0];
            // Tauri drag-drop provides path via webview in production; dialog fallback for pick.
            if (file && "path" in file && typeof (file as File & { path?: string }).path === "string") {
              void ingestLocal((file as File & { path: string }).path);
            } else {
              void onPickFile();
            }
          }}
        >
          <Upload size={28} />
          <div>{t("Drop audio here")}</div>
          <button type="button" className="ghostBtn" disabled={busy} onClick={() => void onPickFile()}>
            <FolderOpen size={16} />
            {t("Choose file")}
          </button>
        </div>

        {error ? (
          <p className="error" role="alert" aria-live="assertive">
            {error}
          </p>
        ) : null}

        <AnimatePresence mode="wait">
          {(audioPath || analysis) && (
            <motion.section
              key="track"
              initial={{ opacity: 0, y: 8 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -8 }}
              transition={{ duration: 0.18, ease: [0.16, 1, 0.3, 1] }}
              style={{ display: "flex", flexDirection: "column", gap: 16 }}
            >
              <div className="trackOverview">
                {thumbnailUrl ? (
                  <img
                    className="trackArtwork"
                    src={thumbnailUrl}
                    alt={title ? `${title} — ${t("Cover art")}` : t("Cover art")}
                    onError={() => setThumbnailUrl(null)}
                  />
                ) : null}
                <h2 className="trackTitle">{title ?? t("Waiting for audio…")}</h2>
              </div>
              <div className="chips">
                <div className="chip">
                  <span className="chipLabel">{t("Key")}</span>
                  <span className="chipValue">{analysis?.key ?? "—"}</span>
                </div>
                <div className="chip">
                  <span className="chipLabel">{t("BPM")}</span>
                  <span className="chipValue">{analysis?.bpm ?? "—"}</span>
                </div>
                <div className="chip">
                  <span className="chipLabel">{t("Duration")}</span>
                  <span className="chipValue">{durationLabel}</span>
                </div>
                <div className="chip">
                  <span className="chipLabel">{t("Device")}</span>
                  <span className="chipValue">{deviceLabel}</span>
                </div>
              </div>

              {youtubeTrack && title ? (
                <section className="songSearchPanel" aria-live="polite">
                  <div className="songSearchHeader">
                    <Search className="songSearchIcon" size={20} aria-hidden="true" />
                    <div>
                      <strong className="songSearchTitle">{t("Dochord chords")}</strong>
                      <p className="songSearchHint">{t("Dochord search description")}</p>
                    </div>
                  </div>

                  {dochordSearchPhase === "searching" ? (
                    <div className="songSearchStatus">
                      <Loader2 size={16} className="spin" aria-hidden="true" />
                      {t("Searching for chords…")}
                    </div>
                  ) : null}

                  {dochordSearchPhase === "error" ? (
                    <p className="songSearchMuted">{t("Song search unavailable")}</p>
                  ) : null}

                  {dochordSearchPhase === "idle" && !dochordSearchConfigured ? (
                    <p className="songSearchMuted">{t("Song search requires API setup")}</p>
                  ) : null}

                  {dochordSearchPhase === "done" && dochordResults.length === 0 ? (
                    <p className="songSearchMuted">{t("No Dochord result")}</p>
                  ) : null}

                  {dochordResults.length > 0 ? (
                    <div className="songSearchResults">
                      {dochordResults.map((result) => (
                        <div className="songSearchResult" key={result.link}>
                          <div className="songSearchResultMeta">
                            <strong className="songSearchResultTitle">{result.title}</strong>
                            {result.snippet ? <span className="songSearchResultSnippet">{result.snippet}</span> : null}
                            <span className="songSearchResultDomain">dochord.com</span>
                          </div>
                          <button
                            type="button"
                            className="iconBtn"
                            aria-label={t("Open Dochord")}
                            title={t("Open Dochord")}
                            onClick={() => void onOpenExternalUrl(result.link)}
                          >
                            <ExternalLink size={16} aria-hidden="true" />
                          </button>
                        </div>
                      ))}
                    </div>
                  ) : null}

                  {googleDochordSearchUrl ? (
                    <button
                      type="button"
                      className="ghostBtn songSearchGoogleBtn"
                      onClick={() => void onOpenExternalUrl(googleDochordSearchUrl)}
                    >
                      <ExternalLink size={16} aria-hidden="true" />
                      {t("Search Dochord on Google")}
                    </button>
                  ) : null}
                </section>
              ) : null}

              <div className="modes" role="group" aria-label={t("Separate")}>
                <button
                  type="button"
                  className={`modeBtn ${purpose === "vocals" ? "modeBtnActive" : ""}`}
                  disabled={busy}
                  onClick={() => selectPurpose("vocals")}
                >
                  <Mic2 size={28} />
                  <span className="modeLabel">{t("Vocals (MelBandRoFormer)")}</span>
                </button>
                <button
                  type="button"
                  className={`modeBtn ${purpose === "instruments" ? "modeBtnActive" : ""}`}
                  disabled={busy}
                  onClick={() => selectPurpose("instruments")}
                >
                  <Music2 size={28} />
                  <span className="modeLabel">{t("Instruments 4 (HT-Demucs)")}</span>
                </button>
                <button
                  type="button"
                  className={`modeBtn ${purpose === "instruments_6s" ? "modeBtnActive" : ""}`}
                  disabled={busy}
                  onClick={() => selectPurpose("instruments_6s")}
                >
                  <Piano size={28} />
                  <span className="modeLabel">{t("Instruments 6 (HT-Demucs 6s)")}</span>
                </button>
              </div>

              <div className="modeInfo" role="status" aria-live="polite">
                <div className="modeInfoHeader">
                  <strong>{modeDetails.model}</strong>
                  <span>
                    <span className="modeInfoLabel">{t("Separates")}:</span> {modeDetails.stems}
                  </span>
                </div>
                <div className="modeInfoGrid">
                  <div className="modeInfoItem">
                    <span className="modeInfoLabel">{t("Advantages")}</span>
                    <p>{modeDetails.advantages}</p>
                  </div>
                  <div className="modeInfoItem">
                    <span className="modeInfoLabel">{t("Limitations")}</span>
                    <p>{modeDetails.limitations}</p>
                  </div>
                </div>
              </div>

              <div className="statusRow">
                <span
                  className={`dot ${
                    phase === "separating" || phase === "installing" || phase === "fetching"
                      ? "dotBusy"
                      : error
                        ? "dotErr"
                        : ready
                          ? "dotOk"
                          : ""
                  }`}
                />
                <span>
                  {busy
                    ? t(
                        phase === "fetching"
                          ? "Fetching from YouTube…"
                          : phase === "analyzing"
                            ? "Analyzing key & BPM…"
                            : phase === "installing"
                              ? "Setting up tools…"
                              : "Separating stems…"
                      )
                    : ready
                      ? t("Ready")
                      : t("Install this mode")}
                </span>
              </div>

              {!ready && phase !== "analyzing" && phase !== "installing" && phase !== "separating" ? (
                <div className="setupNotice" role="note">
                  <Info className="setupNoticeIcon" size={17} aria-hidden="true" />
                  <div className="setupNoticeCopy">
                    <strong className="setupNoticeTitle">{t("Stem model missing notice")}</strong>
                    <p className="setupNoticeText">{t("GPU faster notice")}</p>
                    <p className="setupNoticeText">{t("Stem setup download notice")}</p>
                    <p className="setupNoticeText">{t("Stem setup one-time notice")}</p>
                  </div>
                </div>
              ) : null}

              {(phase === "separating" || phase === "installing" || phase === "fetching" || phase === "analyzing") && (
                <div className="progressWrap">
                  <div className="progressSpinner">
                    <ProgressRing value={phase === "separating" || phase === "installing" ? progress : 0.15} />
                    <button
                      type="button"
                      className="progressCancelBtn"
                      aria-label={t("Cancel")}
                      title={t("Cancel")}
                      onClick={() => void onCancel()}
                    >
                      <X size={18} strokeWidth={2.5} aria-hidden="true" />
                    </button>
                  </div>
                  {phase === "separating" || phase === "installing" ? (
                    <span className="progressPercent" role="status" aria-live="polite">
                      {Math.round(progress * 100)}%
                    </span>
                  ) : null}
                  {log ? <div className="log">{log}</div> : null}
                </div>
              )}

              {phase === "done" && stemPaths.length > 0 && (
                <div className="results">
                  {stemPaths.map((p) => {
                    const resultName = p.split(/[/\\]/).pop() ?? p;
                    return (
                      <div className="resultRow" key={p}>
                        <div className="resultMeta">
                          <span className="resultName">{resultName}</span>
                          <WaveformAudio
                            src={convertFileSrc(p)}
                            label={resultName}
                            waveformLabel={t("Waveform")}
                            onPlaybackError={() => setError(t("Audio playback could not be started"))}
                          />
                        </div>
                        <button
                          type="button"
                          className="iconBtn"
                          aria-label={t("Reveal in folder")}
                          onClick={() => void onRevealInFolder(p)}
                        >
                          <FolderOpen size={16} />
                        </button>
                      </div>
                    );
                  })}
                </div>
              )}

              <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
                <button
                  type="button"
                  className="primaryBtn"
                  disabled={busy || !audioPath}
                  onClick={() => void onSeparate()}
                >
                  {busy ? <Loader2 size={18} className="spin" /> : <Check size={18} />}
                  {ready ? (hasAnyStemResults && stemPaths.length === 0 ? t("Separate new") : t("Separate")) : t("Install")}
                </button>
                {gpuOffer ? (
                  <button type="button" className="ghostBtn" disabled={busy} onClick={() => void onEnableGpu()}>
                    <Gauge size={16} />
                    {t("Enable GPU")}
                  </button>
                ) : null}
              </div>
            </motion.section>
          )}
        </AnimatePresence>

      </main>
      </div>
    </div>
  );
}
