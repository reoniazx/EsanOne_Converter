# Song history sidebar

## Goal

Give the app a chatbot-like history column on the left. Each entry is named with the song title and can restore the song's last analysis, selected separation mode, cover art, and per-mode stem files.

## Design

- Persist up to 30 entries in local storage; history is best-effort and never blocks conversion if storage is unavailable.
- Deduplicate entries by the audio path and move the most recently changed song to the top.
- Store only local metadata and output paths; audio files are not copied into the history store.
- Clicking an entry restores its current state. Existing output files remain playable when their paths still exist; missing files surface through the existing playback error.
- Desktop uses a fixed-width left sidebar. On narrow windows it becomes a horizontal history strip so the main workflow remains usable.
- A clear-history control removes the local list without touching audio files.

## Verification

`src/lib/history.test.ts` covers newest-first ordering, deduplication, and malformed persisted data. The full test suite and production build are required before handoff.
