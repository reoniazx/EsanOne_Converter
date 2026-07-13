# EsanOne Converter unified window Navbar

## Goal

Replace the separate native Windows title bar and app Navbar with one custom title bar that carries the app identity, status controls, and native window actions in a single row.

## User-visible behavior

- The native Tauri window decoration is disabled.
- The top row contains, from left to right: logo and `EsanOne Converter`, version/status information, updater and Windows cascading-menu status, device/language controls, and minimize/maximize/close buttons.
- Empty chrome in the row is draggable. Buttons, status controls, and other interactive elements do not start a window drag.
- Minimize, maximize/restore, and close call Tauri's current-window APIs and preserve normal Windows behavior.
- The same component works on Windows, macOS, and Linux. Platform-specific differences are limited to spacing and the native close-button accent where CSS can provide it.
- The content area remains below the single custom row and keeps the existing responsive layout.

## Architecture

### Tauri window configuration

Set `decorations: false` for the main window while keeping the existing size, resize, and background settings. Keep `identifier` and updater metadata unchanged.

### React window controls

Use `getCurrentWindow()` from `@tauri-apps/api/window` for `minimize()`, `toggleMaximize()`/`maximize()`/`unmaximize()`, `isMaximized()`, and `close()`. Subscribe to resize/move state or refresh the maximized state after each action so the button icon and accessible label stay correct.

### Drag regions

Mark the custom row's non-interactive chrome with `data-tauri-drag-region`. Add explicit `data-tauri-drag-region="false"` (or an equivalent event guard) to buttons and status controls so clicks never become drags.

### Layout

Refactor the existing `.topbar` into a single `.windowBar` with a left brand group, a flexible middle status group, and a right action group. Use `min-width: 0`, ellipsis, and responsive wrapping so the row remains usable at the existing 720px minimum window width.

## Error handling

- If a window API call fails, keep the app open and show the existing localized startup/action error mechanism.
- If maximized-state detection fails, default the button to the restore-safe label and allow retry on the next click.
- Closing the window must not be blocked by status or updater controls.

## Verification

- Run frontend tests and production build.
- Run Rust tests and `cargo fmt --check`.
- Run a Tauri debug build to validate the `decorations` configuration and window API permissions.
- Smoke-test drag, minimize, maximize/restore, close, update, language, and shell-menu controls on the debug app.
