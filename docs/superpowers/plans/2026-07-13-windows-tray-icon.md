# EsanOne Convert Windows Tray Icon Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a native Windows notification-area icon for EsanOne Convert with Open and Exit actions.

**Architecture:** Let Tauri create the tray icon from `tauri.conf.json`, then attach a small native menu and global event handlers in the existing desktop setup. The main window remains the normal application lifecycle owner; this slice does not change close-to-tray behavior.

**Tech Stack:** Tauri 2.11, Rust, Windows tray-icon, Tauri menu APIs, existing bundled PNG/ICO assets.

## Global Constraints

- Use the existing `src-tauri/icons/32x32.png` asset for the tray icon.
- Show tooltip `EsanOne Convert` and use tray id `main`.
- Set `showMenuOnLeftClick` to `false`; left click restores the main window and right click opens the native menu.
- Menu ids are exactly `open` and `exit`, with labels `Open` and `Exit`.
- Keep the feature behind `#[cfg(desktop)]`; do not alter shell-menu, updater, or close-window behavior.
- If the configured tray icon is unavailable during setup, return a setup error instead of silently continuing.

---

### Task 1: Configure the native tray icon

**Files:**
- Modify: `src-tauri/tauri.conf.json` inside `app`.
- Verify: `src-tauri/icons/32x32.png` exists.

**Interfaces:**
- Consumes: the existing bundled icon asset.
- Produces: Tauri config-driven tray icon id `main`, tooltip, and left-click behavior.

- [x] **Step 1: Add the tray configuration**

Add this object under `app`:

```json
"trayIcon": {
  "id": "main",
  "iconPath": "icons/32x32.png",
  "showMenuOnLeftClick": false,
  "tooltip": "EsanOne Convert"
}
```

- [x] **Step 2: Validate the config and asset**

Run:

```powershell
$config = Get-Content -Raw 'src-tauri\tauri.conf.json' | ConvertFrom-Json
if ($config.app.trayIcon.iconPath -ne 'icons/32x32.png') { throw 'Unexpected tray icon path' }
if (-not (Test-Path -LiteralPath ('src-tauri\' + $config.app.trayIcon.iconPath))) { throw 'Tray icon asset missing' }
Write-Output 'tray config: ok'
```

Expected: `tray config: ok`.

### Task 2: Attach menu and tray interactions

**Files:**
- Modify: `src-tauri/src/lib.rs`.

**Interfaces:**
- Consumes: configured tray id `main`, menu ids `open`/`exit`, and existing window label `main`.
- Produces: `Open` restores/focuses the main window, `Exit` exits with code 0, and left-click restores/focuses the main window.

- [x] **Step 1: Add desktop tray imports and the window helper**

Use `tauri::menu::{MenuBuilder, MenuItemBuilder}` and `tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent}` behind desktop cfg, and add:

```rust
#[cfg(desktop)]
fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}
```

- [x] **Step 2: Add setup-time menu attachment**

Inside the existing `setup` closure, after the updater plugin setup, create the menu and attach it to the configured tray:

```rust
#[cfg(desktop)]
{
    let open = MenuItemBuilder::with_id("open", "Open").build(app)?;
    let exit = MenuItemBuilder::with_id("exit", "Exit").build(app)?;
    let menu = MenuBuilder::new(app)
        .items(&[&open, &exit])
        .build()?;
    let tray = app.tray_by_id("main").ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "configured tray icon `main` not found")
    })?;
    tray.set_menu(Some(menu))?;
}
```

- [x] **Step 3: Register menu and tray event handlers**

Build the existing `tauri::Builder` chain into a variable, then add the desktop-only handlers before calling `.setup(...)` so mobile compilation never references tray APIs:

```rust
// After the current plugin chain has been assigned to `builder`:
#[cfg(desktop)]
let builder = builder
    .on_menu_event(|app, event| match event.id() {
        id if id == "open" => show_main_window(app),
        id if id == "exit" => app.exit(0),
        _ => {}
    })
    .on_tray_icon_event(|app, event| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            show_main_window(app);
        }
    });

builder
```

After this block, continue the existing chain with `.setup(...)`, `.manage(...)`, `.invoke_handler(...)`, `.run(...)`, and `.expect(...)` exactly as it exists in `src-tauri/src/lib.rs`. The desktop-only shadowed `builder` is the same builder type on mobile, without the two tray handlers.

- [x] **Step 4: Format and compile the Rust application**

Run:

```powershell
cargo fmt --manifest-path src-tauri\Cargo.toml -- --check
cargo check --manifest-path src-tauri\Cargo.toml
```

Expected: both commands exit 0 with no warnings introduced by the tray code.

### Task 3: Full verification and runtime check

**Files:**
- Test: `src-tauri/src/lib.rs`, `src-tauri/tauri.conf.json`, and the existing frontend/Rust suites.

**Interfaces:**
- Consumes: the configured tray icon and event handlers.
- Produces: verified build/test results and a manual Windows tray check.

- [x] **Step 1: Run the full automated suite**

Run:

```powershell
npm test
npm run build
cargo test --manifest-path src-tauri\Cargo.toml
```

Expected: 16 frontend tests pass, all Rust tests pass, and Vite build exits 0.

- [x] **Step 2: Run the desktop app from the renamed workspace**

Run:

```powershell
npm run tauri dev
```

Expected: the app starts without tray setup errors, `EsanOne Convert` appears in the Windows hidden-icons panel, left click restores/focuses the app, and right click shows `Open`/`Exit`.

- [x] **Step 3: Confirm no unrelated regressions**

Verify the existing shell-menu setup and update controls remain present in the main window, and stop the dev process cleanly after the tray check.
