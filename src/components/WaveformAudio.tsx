import { useCallback, useEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import { downsampleWaveform } from "../lib/waveform";

type WaveformData = {
  samples: number[];
  duration: number;
};

type WaveformAudioProps = {
  src: string;
  label: string;
  waveformLabel: string;
  onPlaybackError: () => void;
};

const WAVEFORM_BARS = 160;
const WAVEFORM_HEIGHT = 48;
const waveformCache = new Map<string, Promise<WaveformData>>();
let waveformDecodeQueue = Promise.resolve();

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function formatTime(value: number): string {
  if (!Number.isFinite(value) || value < 0) return "0:00";
  const seconds = Math.floor(value);
  const minutes = Math.floor(seconds / 60);
  return `${minutes}:${String(seconds % 60).padStart(2, "0")}`;
}

async function decodeWaveform(src: string): Promise<WaveformData> {
  const response = await fetch(src);
  if (!response.ok) throw new Error(`Waveform request failed with ${response.status}`);

  const context = new AudioContext();
  try {
    const decoded = await context.decodeAudioData(await response.arrayBuffer());
    return {
      samples: downsampleWaveform(decoded.getChannelData(0), WAVEFORM_BARS),
      duration: decoded.duration,
    };
  } finally {
    await context.close().catch(() => undefined);
  }
}

async function loadWaveform(src: string): Promise<WaveformData> {
  const cached = waveformCache.get(src);
  if (cached) return cached;

  const promise = waveformDecodeQueue.then(() => decodeWaveform(src));
  waveformDecodeQueue = promise.then(
    () => undefined,
    () => undefined
  );

  waveformCache.set(src, promise);
  try {
    return await promise;
  } catch (cause) {
    waveformCache.delete(src);
    throw cause;
  }
}

export function WaveformAudio({ src, label, waveformLabel, onPlaybackError }: WaveformAudioProps) {
  const audioRef = useRef<HTMLAudioElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [samples, setSamples] = useState<number[] | null>(null);
  const [waveformFailed, setWaveformFailed] = useState(false);
  const [duration, setDuration] = useState(0);
  const [currentTime, setCurrentTime] = useState(0);

  const syncAudioState = useCallback(() => {
    const audio = audioRef.current;
    if (!audio) return;

    setCurrentTime(audio.currentTime);
    if (Number.isFinite(audio.duration)) setDuration(audio.duration);
  }, []);

  useEffect(() => {
    let cancelled = false;
    setSamples(null);
    setWaveformFailed(false);

    void loadWaveform(src)
      .then((data) => {
        if (cancelled) return;
        setSamples(data.samples);
        setDuration((value) => value || data.duration);
      })
      .catch(() => {
        if (!cancelled) setWaveformFailed(true);
      });

    return () => {
      cancelled = true;
    };
  }, [src]);

  const drawWaveform = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const width = Math.max(1, canvas.clientWidth);
    const pixelRatio = Math.min(2, window.devicePixelRatio || 1);
    canvas.width = Math.max(1, Math.round(width * pixelRatio));
    canvas.height = Math.round(WAVEFORM_HEIGHT * pixelRatio);

    const context = canvas.getContext("2d");
    if (!context) return;

    context.setTransform(pixelRatio, 0, 0, pixelRatio, 0, 0);
    context.clearRect(0, 0, width, WAVEFORM_HEIGHT);

    const styles = getComputedStyle(canvas.parentElement ?? canvas);
    const trackColor = styles.getPropertyValue("--border-strong").trim() || "rgba(255,255,255,.18)";
    const valueColor = styles.getPropertyValue("--ink").trim() || "#fff";
    const progress = duration > 0 ? clamp(currentTime / duration, 0, 1) : 0;
    const values = samples ?? [];

    if (values.length === 0) {
      context.fillStyle = trackColor;
      context.fillRect(0, Math.floor(WAVEFORM_HEIGHT / 2), width, 1);
      return;
    }

    const gap = values.length > width / 2 ? 0.5 : 1;
    const barWidth = Math.max(1, width / values.length - gap);
    values.forEach((sample, index) => {
      const x = index * (barWidth + gap);
      const amplitude = Math.max(2, sample * (WAVEFORM_HEIGHT * 0.88));
      const y = (WAVEFORM_HEIGHT - amplitude) / 2;
      context.fillStyle = x / width <= progress ? valueColor : trackColor;
      context.fillRect(x, y, barWidth, amplitude);
    });
  }, [currentTime, duration, samples]);

  useEffect(() => {
    drawWaveform();
    const canvas = canvasRef.current;
    if (!canvas) return;

    const redraw = () => drawWaveform();
    if (typeof ResizeObserver === "undefined") {
      window.addEventListener("resize", redraw);
      return () => window.removeEventListener("resize", redraw);
    }

    const observer = new ResizeObserver(redraw);
    observer.observe(canvas);
    return () => observer.disconnect();
  }, [drawWaveform]);

  const seekToRatio = useCallback(
    (ratio: number) => {
      const audio = audioRef.current;
      if (!audio) return;

      const total = Number.isFinite(audio.duration) ? audio.duration : duration;
      if (!(total > 0)) return;

      const nextTime = clamp(ratio, 0, 1) * total;
      audio.currentTime = nextTime;
      setCurrentTime(nextTime);
    },
    [duration]
  );

  const handleWaveformPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    const bounds = event.currentTarget.getBoundingClientRect();
    seekToRatio((event.clientX - bounds.left) / bounds.width);
  };

  const handleWaveformKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const audio = audioRef.current;
    if (!audio) return;

    const total = Number.isFinite(audio.duration) ? audio.duration : duration;
    if (!(total > 0)) return;

    const step = event.shiftKey ? 5 : 1;
    if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
      event.preventDefault();
      const direction = event.key === "ArrowLeft" ? -1 : 1;
      seekToRatio((audio.currentTime + direction * step) / total);
    } else if (event.key === "Home") {
      event.preventDefault();
      seekToRatio(0);
    } else if (event.key === "End") {
      event.preventDefault();
      seekToRatio(1);
    }
  };

  const sliderDuration = Number.isFinite(duration) ? duration : 0;
  const sliderTime = clamp(currentTime, 0, sliderDuration || 0);

  return (
    <div className="waveformAudio">
      <div
        className={`waveformViewport${waveformFailed ? " waveformViewportFailed" : ""}`}
        role="slider"
        tabIndex={0}
        aria-label={`${label} — ${waveformLabel}`}
        aria-valuemin={0}
        aria-valuemax={sliderDuration}
        aria-valuenow={sliderTime}
        aria-valuetext={`${formatTime(sliderTime)} / ${formatTime(sliderDuration)}`}
        aria-busy={!samples && !waveformFailed}
        onPointerDown={handleWaveformPointerDown}
        onKeyDown={handleWaveformKeyDown}
      >
        <canvas ref={canvasRef} className="waveformCanvas" aria-hidden="true" />
      </div>
      <audio
        ref={audioRef}
        className="resultAudio"
        controls
        preload="metadata"
        src={src}
        onLoadedMetadata={syncAudioState}
        onDurationChange={syncAudioState}
        onTimeUpdate={syncAudioState}
        onError={onPlaybackError}
      />
    </div>
  );
}
