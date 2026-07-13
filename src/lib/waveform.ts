export function downsampleWaveform(samples: ArrayLike<number>, targetBars = 160): number[] {
  const sampleCount = samples.length;
  const barCount = Math.min(sampleCount, Math.max(1, Math.floor(targetBars)));

  if (sampleCount === 0 || !Number.isFinite(targetBars) || targetBars <= 0) return [];

  const result: number[] = [];
  for (let bar = 0; bar < barCount; bar += 1) {
    const start = Math.floor((bar * sampleCount) / barCount);
    const end = Math.max(start + 1, Math.floor(((bar + 1) * sampleCount) / barCount));
    let peak = 0;

    for (let index = start; index < end; index += 1) {
      const sample = Number(samples[index]);
      if (Number.isFinite(sample)) peak = Math.max(peak, Math.min(1, Math.abs(sample)));
    }

    result.push(peak);
  }

  return result;
}
