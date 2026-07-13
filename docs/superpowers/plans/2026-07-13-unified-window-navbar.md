# Unified Window Navbar Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task with verification checkpoints.

**Goal:** Replace the native title bar and app Navbar with one custom draggable bar containing app status and window controls.

**Architecture:** Disable Tauri decorations for the main window. Add a small React window-control layer backed by `getCurrentWindow()` and keep the existing status/update controls inside the same bar. Use explicit drag-region attributes and responsive CSS so controls remain clickable at the existing minimum window size.

**Tech Stack:** Tauri 2, `@tauri-apps/api/window`, React/TypeScript, CSS, lucide-react.

## Global Constraints

- Keep `com.esanone.convert`, updater endpoints, and bundle identity unchanged.
- Keep the product name `EsanOne Converter`.
- Use real Tauri window APIs for minimize, maximize/restore, and close.
- Interactive controls must not initiate a drag.
- Preserve Windows, macOS, and Linux behavior.

---

### Task 1: Configure a frameless main window

**Files:**
- Modify: `src-tauri/tauri.conf.json` main window object

**Interfaces:**
- Consumes: existing window size, resize, and background configuration.
- Produces: a Tauri main window with `decorations: false` and unchanged identifier/updater metadata.

- [ ] **Step 1: Add `decorations: false`**

Add the property to the `main` window object without changing any other window values:

```json
"decorations": false,
"resizable": true
```

- [ ] **Step 2: Run Tauri config validation**

Run: `npm run tauri build -- --debug --no-bundle`

Expected: the config is accepted and the debug binary builds.

---

### Task 2: Add native window controls to the unified bar

**Files:**
- Modify: `src/App.tsx` imports, state/effects, and header JSX

**Interfaces:**
- Consumes: existing `showError`, `t`, locale, status, and update controls.
- Produces: `minimize`, `toggleMaximize`, and `close` callbacks using `getCurrentWindow()`, plus a maximized-state label/icon.

- [ ] **Step 1: Add window API import and state**

Use:

```ts
import { getCurrentWindow } from "@tauri-apps/api/window";
const appWindow = getCurrentWindow();
const [isMaximized, setIsMaximized] = useState(false);
```

Refresh state on mount with `appWindow.isMaximized()` and after toggle. If the API rejects, call `showError("startup", cause)` and leave the current state unchanged.

- [ ] **Step 2: Add the three callbacks**

Implement:

```ts
const onMinimize = () => appWindow.minimize().catch((cause) => showError("startup", cause));
const onToggleMaximize = async () => {
  try {
    await appWindow.toggleMaximize();
    setIsMaximized(await appWindow.isMaximized());
  } catch (cause) {
    showError("startup", cause);
  }
};
const onClose = () => appWindow.close().catch((cause) => showError("startup", cause));
```

- [ ] **Step 3: Render controls in the header**

Add icon buttons using `Minus`, `Square`, `Copy`, and `X`. Set `aria-label` and `title` from localized keys. Mark each control `data-tauri-drag-region="false"`.

- [ ] **Step 4: Add drag-region attributes**

Mark the header and non-interactive brand/status wrappers with `data-tauri-drag-region`. Keep all buttons and status action rows explicitly non-draggable.

---

### Task 3: Refactor layout and verify behavior

**Files:**
- Modify: `src/App.css` header and responsive rules
- Modify: `src/lib/i18n/messages.ts` window-control labels
- Test: existing frontend test suite and Tauri build

**Interfaces:**
- Consumes: header JSX from Task 2.
- Produces: one compact row with no footer title/status duplication and usable controls at 720px minimum width.

- [ ] **Step 1: Add localized labels**

Add Thai keys for `Minimize window`, `Maximize window`, `Restore window`, and `Close window`; English falls back to the source key.

- [ ] **Step 2: Update CSS**

Rename the layout to `.windowBar`, use `min-width: 0`, keep the middle status group flexible, and reserve fixed hit targets for the window buttons. Add a close hover state and a mobile wrap rule that keeps the controls visible.

- [ ] **Step 3: Run verification**

Run:

```powershell
npm test -- --run
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
npm run tauri build -- --debug --no-bundle
```

Expected: all tests and builds pass, with the debug binary created under `src-tauri\\target\\debug\\esanone-convert.exe`.
