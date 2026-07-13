# Right-Click Output Naming Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Put Windows context-menu stem outputs beside the source audio with descriptive prefixes, and show Key/BPM results in an in-app Toast.

**Architecture:** Extend the startup CLI with a dedicated `analyze` operation and explicit context-menu output behavior. Add small filesystem/name helpers in the Rust backend for collision-safe destination names and flattening backend output paths. Route the existing analysis result to a frontend Toast; Key/BPM never copies audio.

**Tech Stack:** Rust/Tauri 2, PowerShell registry integration, Python/librosa analyzer, Vitest, Cargo tests.

## Global Constraints

- Context-menu separation outputs go beside the source file; normal app conversions keep the existing app-data `exports` directory.
- Never overwrite an existing file; allocate `(1)`, `(2)`, etc. before the extension.
- Prefixes are `[Vocal]`, `[Drum]`, `[Bass]`, `[Other]`, `[Guitar]`, `[Piano]`, and `[INST]`.
- Key/BPM context actions show `Key: {key} · BPM: {bpm}` in an in-app Toast and do not create files.
- Production code must be preceded by a failing test.

---

### Task 1: CLI contract and filename helpers

**Files:**
- Modify: `src-tauri/src/cli.rs`
- Modify: `src-tauri/src/stem_separate.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: Rust unit tests in the same modules

**Interfaces:**
- `parse_args` recognizes `analyze --path <file>` and returns a typed startup request.
- `stem_separate::prefixed_output_path(dir, prefix, source_name)` returns the first unused path.
- `stem_separate::stem_output_prefix(model, stem_name)` maps backend stem names to approved prefixes.

- [x] Write failing tests for `analyze` parsing, approved prefix mapping, and collision-safe `(1)` naming.
- [x] Run the focused Rust tests and confirm the new assertions fail because the helpers/contract did not exist.
- [x] Implement the typed CLI operation and pure filename helpers without changing normal separation execution.
- [x] Re-run the focused tests and confirm they pass.

### Task 2: Flatten context-menu separation results

**Files:**
- Modify: `src-tauri/src/stem_separate.rs`
- Test: Rust tests in `src-tauri/src/stem_separate.rs`

**Interfaces:**
- The worker receives the source parent as the context-menu output directory.
- After a successful backend run, each discovered stem is renamed/copied to the parent using the approved prefix and collision allocator; emitted `stemPaths` contain final paths.

- [x] Write a regression test that creates a model/input subdirectory with stem files and expects flattened prefixed destinations beside the source.
- [x] Run the focused Rust test and verify flattened paths and cleanup.
- [x] Implement flattening with safe rename behavior and cleanup of only the worker-created temporary tree.
- [x] Re-run focused tests and confirm the final paths and collision behavior pass.

### Task 3: Route Key/BPM to an in-app Toast

**Files:**
- Modify: `src-tauri/src/analyze.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/cli.rs`
- Modify: `src/lib/tauriApi.ts`
- Modify: `src/App.tsx`
- Create: `src/lib/keyBpmNotification.ts`
- Test: Rust tests in `src-tauri/src/cli.rs`; Vitest tests for CLI parsing and Toast formatting

**Interfaces:**
- `StartupCli` and `parseStartupCliArgs` expose an `analyze` request. App startup and single-instance handling invoke the existing display-only analysis command for that request.
- The frontend shows a Toast when an analyze request completes successfully, with Thai/English labels supplied by the current locale.

- [x] Write a failing test for Key/BPM Toast formatting.
- [x] Run the focused Vitest test and confirm it fails before implementation.
- [x] Route the existing analyzer command without copying audio and implement the Toast formatter.
- [x] Re-run focused tests and confirm success and failure paths are covered.

### Task 4: Register Windows context-menu commands

**Files:**
- Modify: `src-tauri/src/shell_integration.rs`
- Test: Rust shell integration tests

**Interfaces:**
- Windows registry commands launch `separate --model ... --path "%1"` and `analyze --path "%1"`.
- Existing three separation commands remain available; the new analysis command is a sibling action.

- [x] Write a failing test for the command argument builders/labels.
- [x] Run the focused shell test and confirm it fails before implementation.
- [x] Implement the `Analyze Key & BPM` registry command and keep unregister compatibility for old keys.
- [x] Re-run focused tests and verify the generated registry script contains all four actions.

### Task 5: Verification and regression checks

**Files:**
- Modify: `src/lib/tauriApi.ts` only if frontend CLI parsing types need the new operation.
- Modify: i18n messages only if a visible label is surfaced in the app.

- [x] Run `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`.
- [x] Run `cargo test --manifest-path src-tauri/Cargo.toml` (37 passed).
- [x] Run `npm test` (18 passed).
- [x] Run `npm run build` successfully.
- [x] Run `npm run tauri dev` far enough to confirm startup succeeds, then stop the dev process cleanly.
- [x] Mark the plan complete with the exact verification results (Rust 35 passed, frontend 19 passed, build and dev startup passed).
