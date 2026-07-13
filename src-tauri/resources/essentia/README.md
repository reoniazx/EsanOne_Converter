# Essentia native helper

The platform folders contain the static `streaming_extractor_music` executable
from the official AcousticBrainz/Essentia distribution (Essentia 2.1 beta2
builds). The Tauri code embeds the platform-specific executable and extracts it
to the user's temporary folder before analysis, so Python and Essentia Python
bindings are not required.

To refresh the binaries from the pinned upstream archives, run:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/fetch-essentia-extractors.ps1
```

The helper uses Essentia's standard `RhythmExtractor2013`-based BPM descriptor
and tonal key profiles exposed by `streaming_extractor_music`. It writes JSON;
EsanOne Converter reads `rhythm.bpm`, `tonal.key_key`, `tonal.key_scale`, and
`metadata.audio_properties.length`.

Essentia is licensed under AGPL-3.0. Keep the upstream license and source
availability notices with every distributed build.

The pinned upstream artifacts are Windows 32-bit (runs on supported 64-bit
Windows), Linux x86_64, and macOS Intel. Apple Silicon builds should either
run the Intel helper through Rosetta or replace the macOS asset with a native
arm64/universal build before release.
