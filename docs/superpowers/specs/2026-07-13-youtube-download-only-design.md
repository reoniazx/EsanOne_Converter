# YouTube download-only action

## Goal

Offer a separate action for users who only want the downloaded audio file and do not want Key/BPM analysis to start automatically.

## Design

- Keep the existing `ค้นหา` action unchanged: download, analyze, and continue the normal workflow.
- Add a secondary `ดาวน์โหลดเพลง` button beside it.
- Both actions share the same YouTube downloader and progress/cancel behavior.
- The download-only intent is held only for the active download session. On completion it loads the file, cover art, and title, clears the active session, and moves to ready without starting analysis or Dochord search.
- A failed or cancelled download clears the intent so the next request cannot inherit download-only behavior.

## Verification

`src/lib/youtubeDownloadFlow.test.ts` covers both intents. The production build and full Vitest suite are required before handoff.
