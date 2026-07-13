# Per-mode stem result memory

## Goal

Keep completed stem files associated with the separation mode that produced them. Switching to a mode without results must hide other-mode files and leave the user with a clear action to run a new separation. Switching back to a completed mode must restore its files without running separation again.

## Design

- Store output paths in a small immutable map keyed by `vocals`, `instruments`, and `instruments_6s`.
- On a new local file, YouTube download, or shell/CLI input, clear the whole map.
- On mode selection, show cached paths for that mode; otherwise clear the visible list and set the screen ready for a new separation. If another mode already has output, label the action `แยกใหม่`.
- On separation completion, replace only the selected mode's cached paths.
- Keep mode buttons disabled during active work so a running session cannot be reassigned to another mode.

## Verification

The cache behavior is covered by `src/lib/stemResults.test.ts`; the full Vitest suite and production build must pass before handoff.
