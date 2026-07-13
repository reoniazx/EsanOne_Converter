# Essentia Key/BPM Analysis

## Goal

Use MTG Essentia for Key and BPM detection without requiring Python bindings,
so the same analysis engine can run on Windows, macOS, and Linux.

## Runtime design

- Use Essentia's native `streaming_extractor_music` static executable.
- Embed the platform executable in the Tauri binary and extract it to the
  user's temporary directory on demand.
- Invoke it with `input-audio-path -` and read JSON from stdout. This avoids
  failures when a user's temp folder contains non-Latin characters.
- Read `rhythm.bpm`, `tonal.key_key`, `tonal.key_scale`, and
  `metadata.audio_properties.length`.
- Keep the existing in-app completion Toast; no Key/BPM output file is created.

## Error behavior

- Missing or invalid Essentia output is reported as an analysis failure.
- Unsupported targets report that Essentia analysis is unavailable on the
  device instead of attempting a hidden Python fallback.
- The analysis child process remains cancellable through the existing Tauri
  command and event flow.

## Distribution note

The checked-in helper archives are official Essentia/AcousticBrainz static
extractor builds. Essentia is AGPL-3.0; keep the license and source notices in
distributed packages and refresh binaries only through the pinned download
script.
