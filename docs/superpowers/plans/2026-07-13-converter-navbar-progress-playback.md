# EsanOne Converter Navbar, Progress, and Playback Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task with verification checkpoints.

**Goal:** Rename the user-facing app to EsanOne Converter, expose truthful install/GPU/separation progress, move updater and shell status into the navbar, and make separated files playable immediately.

**Architecture:** Keep the existing Tauri command and session-id architecture. Add an optional normalized `progress` field to worker event payloads, emit explicit backend stage progress for setup work, and let the React app render the same value in the ring and percentage label. Use Tauri asset URLs for inline audio playback while preserving the existing reveal-in-folder action and update identifier.

**Tech Stack:** React + TypeScript, Vite, Tauri 2, Rust, lucide-react, Tauri asset protocol, existing i18n and event helpers.

## Global Constraints

- Keep updater identifier `com.esanone.convert` unchanged.
- Display **EsanOne Converter** in all user-facing title, tray, brand, and UI strings.
- The YouTube button text changes to **แยกเสียง** / **Separate** but keeps its current download behavior.
- Progress values are normalized from `0` to `1` and must be emitted by backend events, not guessed from arbitrary log text.
- GPU failure must leave CPU mode available and explain that GPU is faster when supported.
- Support Windows, macOS, and Linux paths through Tauri's `convertFileSrc` and the existing asset media CSP.
- Do not change the updater endpoint, signing key, or bundle identifier.

---

### Task 1: Update product naming and localized copy

**Files:**
- Modify: `src-tauri/tauri.conf.json` (`productName`, window title, tray tooltip)
- Modify: `src-tauri/src/lib.rs` (tray error text)
- Modify: `src-tauri/src/stem_separate.rs` (user-facing app-folder/model error strings)
- Modify: `src/App.tsx` (brand and hero keys, setup copy, YouTube button key)
- Modify: `src/lib/i18n/messages.ts` (Thai and English source keys)
- Test: `src/lib/i18n/locale.test.ts` and existing frontend tests

**Interfaces:**
- Consumes: existing `useT()` message lookup and Tauri product metadata.
- Produces: stable source keys `EsanOne Converter`, `Separate audio`, `Stem model missing notice`, and `GPU faster notice` used by the UI.

- [ ] **Step 1: Add failing message assertions**

Add tests to the existing i18n test file for the default Thai translations and English fallback:

```ts
expect(messages.th["Separate audio"]).toBe("แยกเสียง");
expect(messages.th["Stem model missing notice"]).toContain("ตอนนี้คุณยังไม่มี Model แยกเสียง");
expect(messages.th["GPU faster notice"]).toContain("GPU");
```

- [ ] **Step 2: Run the focused test and verify it fails**

Run: `npm test -- --run src/lib/i18n/locale.test.ts`

Expected: FAIL because the new keys do not yet exist.

- [ ] **Step 3: Add the new Thai/English source keys**

In `src/lib/i18n/messages.ts`, add the keys with these values:

```ts
"EsanOne Converter": "EsanOne Converter",
"Separate audio": "แยกเสียง",
"Stem model missing notice": "ตอนนี้คุณยังไม่มี Model แยกเสียง คลิกปุ่มติดตั้งเพื่อใช้งานต่อ",
"GPU faster notice": "GPU แยกเสียงได้เร็วกว่า CPU",
```

Keep English source keys as English so the existing fallback behavior remains intact.

- [ ] **Step 4: Switch user-facing UI references**

In `src/App.tsx`, replace `t("EsanOne Convert")` with `t("EsanOne Converter")`, replace the YouTube button's `t("Paste")` with `t("Separate audio")`, and render the new setup notice keys. Do not change the `onFetchYoutube` callback.

- [ ] **Step 5: Rename Tauri metadata without changing identity**

Update `src-tauri/tauri.conf.json` values:

```json
"productName": "EsanOne Converter",
"tooltip": "EsanOne Converter",
"title": "EsanOne Converter"
```

Leave `"identifier": "com.esanone.convert"` unchanged. Update Rust error strings that mention the old product name where they are shown to users.

- [ ] **Step 6: Run tests and build**

Run: `npm test -- --run`

Expected: all frontend tests pass, including the new translation assertions.

---

### Task 2: Add structured setup and GPU progress events in Rust

**Files:**
- Modify: `src-tauri/src/stem_separate.rs` event payload definitions and install functions around `stem_tool_install`, `stem_gpu_install`, and `stem_separate_start`
- Test: `src-tauri/src/stem_separate.rs` unit tests (progress clamping/monotonic helper)

**Interfaces:**
- Consumes: existing `emit_log`, `STEM_TOOL_INSTALL_EVENT`, `STEM_GPU_INSTALL_EVENT`, `STEM_SEPARATE_EVENT`, and session id filtering.
- Produces: event payload `{ sessionId, kind, text, progress }` where `progress: Option<f32>` is always between `0.0` and `1.0` when present.

- [ ] **Step 1: Add a failing progress helper test**

Add a unit test for a small helper that clamps stage values:

```rust
#[test]
fn progress_value_is_clamped() {
    assert_eq!(clamp_progress(-0.2), 0.0);
    assert_eq!(clamp_progress(0.45), 0.45);
    assert_eq!(clamp_progress(1.4), 1.0);
}
```

- [ ] **Step 2: Run the Rust test and verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml progress_value_is_clamped`

Expected: FAIL because `clamp_progress` is not defined.

- [ ] **Step 3: Add the optional progress field and helper**

Extend the shared log event payload (or the equivalent serde event struct) with `progress: Option<f32>`, add `clamp_progress(value: f32) -> f32`, and keep existing callers source-compatible by defaulting the field to `None` in text-only events.

- [ ] **Step 4: Emit explicit setup stages**

In `stem_tool_install`, emit progress events in this order using the existing event name and session id:

```text
0.05  Checking Python and the managed environment
0.25  Installing separation dependencies
0.55  Installing/validating the selected model
0.80  Checking FFmpeg and runtime files
0.95  Verifying the separation backend
1.00  Stem tools ready
```

For already-installed stages, emit the same stage message with the corresponding value so the UI still advances visibly.

- [ ] **Step 5: Emit explicit GPU stages and visible logs**

In `stem_gpu_install`, emit `STEM_GPU_INSTALL_EVENT` messages and progress values:

```text
0.05  Checking GPU support
0.20  Preparing the Python GPU environment
0.65  Installing the CUDA-enabled PyTorch packages
0.85  Testing CUDA and the selected device
0.95  Saving GPU preference
1.00  GPU is ready; separation will be faster than CPU
```

Keep stdout/stderr streaming through the existing `run_tracked_child` path so pip output is appended to the same GPU log event stream.

- [ ] **Step 6: Preserve real separation progress**

When forwarding separation process output, attach the parsed `N%|` value as `Some(N as f32 / 100.0)` on `STEM_SEPARATE_EVENT`; retain the raw text so the existing backend-specific tracker remains a fallback for old output.

- [ ] **Step 7: Run Rust tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`

Expected: existing Rust tests plus `progress_value_is_clamped` pass.

---

### Task 3: Render progress, navbar controls, and playable results in React

**Files:**
- Modify: `src/App.tsx` (event listeners, navbar, progress panel, result rows)
- Modify: `src/App.css` (navbar status layout, progress percentage, result audio controls, responsive rules)
- Modify: `src-tauri/capabilities/default.json` only if asset protocol permission is required by the installed Tauri version
- Modify: `src-tauri/tauri.conf.json` only if the existing media CSP needs an asset-scheme adjustment

**Interfaces:**
- Consumes: `progress?: number` from Task 2 events, existing `UpdateStatus`, `ShellMenuStatus`, `convertFileSrc` from `@tauri-apps/api/core`.
- Produces: visible percentages for install/GPU/separation, topbar updater/shell controls, and `<audio controls>` for each result path.

- [ ] **Step 1: Add a failing UI behavior test or type-level fixture**

Extend the existing frontend test harness with a fixture asserting that an install event updates the displayed percentage to 65% and that a result path is converted to an asset URL. If no component harness exists, add a focused pure helper test for:

```ts
expect(formatProgressPercent(0.65)).toBe("65%");
expect(formatProgressPercent(1.2)).toBe("100%");
```

- [ ] **Step 2: Run the focused test and verify it fails**

Run: `npm test -- --run`

Expected: FAIL until the progress formatter/listener behavior is implemented.

- [ ] **Step 3: Consume progress from all stem events**

Import `convertFileSrc` and update listeners in `src/App.tsx`:

```ts
type StemProgressEvent = { sessionId: string; kind: string; text?: string; progress?: number };

if (typeof e.payload.progress === "number") {
  setProgress(Math.min(1, Math.max(0, e.payload.progress)));
}
```

Register listeners for both `stem-tool-install-event` and `stem-gpu-install-event`. Reset `setProgress(0)` in `onInstall` and `onEnableGpu`; set `setProgress(1)` in both successful done handlers.

- [ ] **Step 4: Display numeric percentages and live logs**

Use the actual `progress` value for the ring during install and GPU setup, and add a visible label:

```tsx
<span className="progressPercent">{Math.round(progress * 100)}%</span>
```

Keep the cancel button and log area. Do not use the previous static `0.15` ring value.

- [ ] **Step 5: Move updater and shell status into the navbar**

Create a compact `.navStatus` group inside `.topActions` containing the existing update status text/button and shell registration status/button. Remove the duplicate `.updateRow` and `.shellRow` footer controls, leaving the footer only if another footer element is still needed.

- [ ] **Step 6: Add inline playback to result rows**

Render each result path as:

```tsx
<div className="resultRow" key={p}>
  <div className="resultMeta">
    <span className="resultName">{basename}</span>
    <audio className="resultAudio" controls preload="metadata" src={convertFileSrc(p)} />
  </div>
  <button type="button" className="iconBtn" aria-label={t("Reveal in folder")}>
    <FolderOpen size={16} />
  </button>
</div>
```

Keep the reveal callback unchanged. Add `onError` only to set a localized playback warning; do not hide the row or the folder button.

- [ ] **Step 7: Add responsive CSS**

Add styles for `.navStatus`, `.navStatusRow`, `.navStatusLabel`, `.progressPercent`, `.resultMeta`, and `.resultAudio`. At the existing 720px minimum width, allow `.topActions` to wrap and keep buttons at a minimum 32px hit target. Set the audio element to `width: 100%` within the result row without overflowing the card.

- [ ] **Step 8: Run frontend tests and build**

Run: `npm test -- --run` and `npm run build`.

Expected: all tests pass and Vite produces `dist` without TypeScript or CSP-related errors.

---

### Task 4: Verify Tauri packaging and runtime wiring

**Files:**
- Inspect/modify: `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`, and generated Tauri permissions only if the build reports a required asset permission
- Inspect: `src-tauri/src/lib.rs`, `src-tauri/src/stem_separate.rs`

**Interfaces:**
- Consumes: completed frontend event listeners, Tauri metadata, and asset URL playback.
- Produces: a debug executable that starts with the new product name and has no permission/configuration errors.

- [ ] **Step 1: Run the Tauri debug build**

Run: `npm run tauri build -- --debug --no-bundle`

Expected: successful build ending with `src-tauri\\target\\debug\\esanone-convert.exe` (or the platform-equivalent binary).

- [ ] **Step 2: Check generated metadata and permission output**

Search the generated bundle/configuration for `EsanOne Convert`. The only allowed remaining occurrences are internal identifiers or historical error text that is not user-facing; `com.esanone.convert` must remain unchanged.

- [ ] **Step 3: Smoke-test the visible workflow**

Launch the debug app and verify:

1. Navbar contains update status/action and Windows cascading-menu status/action.
2. Missing-model notice contains the requested Thai wording and GPU speed note.
3. Clicking Install shows a changing percentage and log output.
4. Clicking Enable GPU shows GPU-specific log output and percentage.
5. Separation shows real progress and result rows contain playable audio controls.
6. YouTube button reads `แยกเสียง` while still downloading the URL audio.

- [ ] **Step 4: Run the final regression checks**

Run:

```powershell
npm test -- --run
npm run build
npm run tauri build -- --debug --no-bundle
```

Expected: all commands exit successfully. If a process lock blocks the Tauri build, close the running debug app and rerun the same command; do not delete build artifacts or change updater identity.
