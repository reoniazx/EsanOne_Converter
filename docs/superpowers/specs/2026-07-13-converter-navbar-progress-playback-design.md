# EsanOne Converter: Navbar, setup progress, and result playback

## Goal

Improve the first-run and separation workflow so users can see what the app is doing, understand the GPU/CPU choice, and play separated files immediately. Consolidate updater and Windows shell status in the top navigation while preserving the existing updater identity.

## User-visible behavior

- The product name shown in the window, tray, brand, and UI is **EsanOne Converter**.
- The updater keeps the existing identifier `com.esanone.convert` so existing installations and update metadata continue to match.
- The YouTube row button label changes from **Paste / วาง** to **Separate / แยกเสียง**. Its behavior remains unchanged: it accepts a YouTube link and downloads the audio before separation.
- When the selected separation mode is not installed, the notice says that the user does not yet have the separation model and should click Install to continue. It also explains that GPU separation is faster than CPU.
- Update controls and the Windows cascading-menu registration status move from the footer to a compact, responsive group in the navbar.
- Tool installation, GPU setup, and separation show a numeric percentage, a progress ring, and live log output. The percentage is supplied by backend events rather than inferred from arbitrary log text.
- When separation completes, every result row has an inline audio player and keeps the reveal-in-folder action.

## Architecture

### Backend progress events

Extend stem installation and separation event payloads with an optional normalized `progress` value (`0..1`). Rust emits stage progress for Python/dependency/model setup and GPU setup, and forwards the actual progress reported by the separation process. Existing text/session fields remain intact so current listeners stay compatible.

### Frontend state

The React app consumes `progress` from tool-install, GPU-install, and separation events. It resets progress at the start of each operation, updates it as events arrive, sets it to `1` on successful completion, and preserves the last log when an operation fails. The progress panel renders the percentage for installation and separation rather than a static placeholder.

### Local playback

Use Tauri's `convertFileSrc` to turn each returned absolute path into an asset URL for a native `<audio controls>` element. Keep the existing folder-reveal action and ensure the media CSP allows the asset scheme.

### Navbar layout

Move updater and shell-menu controls into the existing topbar. Use compact labeled rows with responsive wrapping so the controls remain usable at the existing minimum window width. Remove the duplicate footer status rows.

## Error handling

- Installation and GPU failures keep the latest log visible and show a localized, actionable error.
- GPU setup failures explicitly explain that CPU mode remains available.
- Playback errors are isolated to the affected audio element; the reveal action remains available.
- Existing session-id filtering remains the guard against stale worker events.

## Verification

- Run the focused frontend test suite.
- Run the production frontend build.
- Run a Tauri debug build to verify configuration, event payload typing, asset playback wiring, and renamed user-facing metadata.
- Confirm no updater identifier or endpoint is changed.
