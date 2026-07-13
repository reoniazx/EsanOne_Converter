# Output Waveform Design

## Goal

Show a lightweight waveform preview for every separated audio file and let the user seek by clicking the waveform.

## Design

- Each result row renders a reusable `WaveformAudio` component.
- The component decodes the local audio asset through the Web Audio API, reduces it to roughly 160 amplitude samples, and draws the samples as a compact canvas waveform.
- Clicking or pressing Left/Right on the waveform seeks the existing native audio element. Playback continues when it was already playing; a paused track remains paused.
- The native audio controls remain available below the waveform for familiar play, volume, and download behavior.
- Waveform loading or decoding failures are non-blocking: the native audio element remains usable and the preview falls back to an empty state.
- Samples are cached by asset URL during the page lifetime to avoid decoding the same output repeatedly.

## Accessibility and performance

- The waveform exposes a slider role with the current time and duration, supports keyboard seeking, and includes a localized label.
- The canvas is capped at a small sample count and uses one animation loop only while audio is playing.
- Audio and animation resources are released when a result row unmounts.

## Verification

- Unit tests cover amplitude downsampling and empty/invalid input behavior.
- The frontend build and complete Vitest suite must pass.
