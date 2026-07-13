# EsanOne Convert Windows Tray Icon Design

## Goal

Show EsanOne Convert in the Windows notification-area tray (including the hidden-icons panel shown by the user) while the desktop app is running.

## Approved behavior

- Use Tauri's native tray-icon support and the existing app icon asset.
- Left-clicking the tray icon restores and focuses the main window.
- Right-clicking opens a small menu with `Open` and `Exit`.
- Closing the main window continues to exit normally; this first version does not silently minimize to the tray.
- Tray support is desktop-only and must not affect shell-menu registration, stem separation, or updater behavior.

## Technical design

- Add `app.trayIcon` to `src-tauri/tauri.conf.json` using the small `icons/32x32.png` asset and tooltip `EsanOne Convert`.
- Enable Tauri's tray-icon feature through the config-driven build setup already used by the project.
- In `src-tauri/src/lib.rs`, create the tray menu during desktop setup with `MenuItemBuilder` and attach it to the configured tray icon.
- Handle tray menu events by showing/focusing the `main` window for `Open` and calling `app.exit(0)` for `Exit`.
- Handle a left-click tray event by showing/focusing the `main` window. Avoid changing close behavior in this slice.
- Keep all tray code behind `#[cfg(desktop)]` so mobile builds do not compile desktop-only APIs.

## Error handling

- If the tray icon cannot be created or the menu cannot be attached, fail application setup with the native Tauri error rather than silently reporting success.
- The app should still use the existing window icon and bundled icon assets when tray support is disabled on unsupported targets.

## Verification

- Parse `tauri.conf.json` and confirm the tray icon path exists.
- Run `cargo fmt --check`.
- Run the full Rust test suite and frontend test/build suite.
- Run `npm run tauri dev` and confirm the EsanOne Convert icon appears in Windows' tray/hidden-icons panel, left click restores the window, and right-click exposes `Open` and `Exit`.
