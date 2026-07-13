# EsanOne Convert Rename Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rename the app-facing project identity from `EsanOne Stem` / `esanone-stem` to `EsanOne Convert` / `esanone-convert` without breaking the existing stem-separation domain contracts or installed tool data.

**Architecture:** Keep the existing `stem_separate` Rust module, `stem-*` IPC events, model IDs, output `stems` directory, and `EsanOne Stem` tools data directory as compatibility-sensitive internals. Change only public branding, package/binary/library identity, Tauri product metadata, shell-facing names, updater workflow secret reference, and documentation.

**Tech Stack:** React 19 + Vite + TypeScript, Tauri 2, Rust/Cargo, GitHub Actions, Tauri updater.

## Global Constraints

- Preserve `stem_*` Rust module names, `stem-*` event names, model IDs, and existing `stems`/`stem-tools` data paths.
- Use `EsanOne Convert` for visible product text and `esanone-convert` for machine package/binary/library names.
- Use `com.esanone.convert` as the new Tauri identifier; this is an installer identity change.
- Keep updater assets on the public `reoniazx/EsanOne-Releases` repository.
- Use the existing GitHub Actions secret name `ESANONE_CONVERT_RELEASES_TOKEN`; never read or commit `Token-releases.txt`.

---

### Task 1: Capture rename surface and protect compatibility

**Files:**
- Modify: `.gitignore` to ignore `Token-releases.txt` if it is a local credential file.
- Test: existing `npm test`, `cargo test --manifest-path src-tauri/Cargo.toml`, and `npm run build`.

**Interfaces:**
- Consumes: current source, package metadata, shell integration, updater workflow.
- Produces: a verified list of public identity strings and compatibility-sensitive `stem` strings.

- [x] **Step 1: Add the credential filename to the ignore rules**

```gitignore
Token-releases.txt
```

- [x] **Step 2: Run the baseline verification**

Run:

```powershell
npm test
npm run build
cargo test --manifest-path src-tauri\Cargo.toml
```

Expected: all current frontend/Rust tests pass and the production build exits with code 0.

- [x] **Step 3: Re-scan identity references before editing**

Run:

```powershell
rg -n -i "EsanOne[_ -]?Stem|esanone[_ -]?stem|com\.esanone\.stem" -g '!node_modules' -g '!target' -g '!dist' .
```

Expected: every hit is classified as either public identity (to rename) or compatibility-sensitive stem domain (to preserve).

### Task 2: Rename package, Cargo, Tauri, and executable identity

**Files:**
- Modify: `package.json`, `package-lock.json`.
- Modify: `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `src-tauri/src/main.rs`.
- Modify: `src-tauri/tauri.conf.json`.

**Interfaces:**
- Consumes: Tauri build metadata and existing library entry point.
- Produces: `esanone-convert` npm/Cargo package, `esanone_convert_lib` Rust library, product name `EsanOne Convert`, and identifier `com.esanone.convert`.

- [x] **Step 1: Change frontend package metadata**

In `package.json`, set:

```json
{
  "name": "esanone-convert"
}
```

Regenerate the matching root package name entries in `package-lock.json` with `npm install --package-lock-only`.

- [x] **Step 2: Change Cargo package and library names**

In `src-tauri/Cargo.toml`, set:

```toml
name = "esanone-convert"
description = "EsanOne Convert — local stem separation"

[lib]
name = "esanone_convert_lib"
```

Update `src-tauri/src/main.rs` to call `esanone_convert_lib::run()` and regenerate the root package entry in `src-tauri/Cargo.lock` with `cargo check --manifest-path src-tauri/Cargo.toml`.

- [x] **Step 3: Change Tauri product identity**

In `src-tauri/tauri.conf.json`, set:

```json
{
  "productName": "EsanOne Convert",
  "identifier": "com.esanone.convert",
  "app": { "windows": [{ "title": "EsanOne Convert" }] }
}
```

- [x] **Step 4: Run the targeted metadata verification**

Run:

```powershell
npm run build
cargo test --manifest-path src-tauri\Cargo.toml
```

Expected: TypeScript/Vite build and all Rust tests pass with the renamed Rust library.

### Task 3: Rename visible branding and shell-facing labels while preserving stem internals

**Files:**
- Modify: `index.html`, `src/App.tsx`, `src/lib/i18n/messages.ts`.
- Modify: `src-tauri/src/lib.rs`, `src-tauri/src/shell_integration.rs`.
- Modify: `README.md`, `PRODUCT.md`, `DESIGN.md`.

**Interfaces:**
- Consumes: existing UI strings, shell integration commands, and updater copy.
- Produces: consistent `EsanOne Convert` display name and new shell-facing executable/menu names; old `stem-*` events and tool paths remain unchanged.

- [x] **Step 1: Rename visible UI/documentation strings**

Replace only public product strings:

```text
EsanOne Stem -> EsanOne Convert
EsanOne Stem.exe -> EsanOne Convert.exe
```

Keep domain strings such as `stem separation`, `Separate stems`, model IDs, and `stem-*` event names.

- [x] **Step 2: Update shell integration with compatibility fallback**

Use `EsanOne Convert` for newly registered menu/workflow/desktop display names and `esanone-convert` for the new executable/desktop filenames. Keep the old `EsanOne Stem.exe` and `esanone-stem.exe` lookup/unregister paths as fallbacks so existing installs can still be removed or repaired.

- [x] **Step 3: Run the branding scan**

Run:

```powershell
rg -n "EsanOne Stem|esanone-stem|com\.esanone\.stem" -g '!node_modules' -g '!target' -g '!dist' .
```

Expected: no public identity hits remain except explicitly documented backward-compatibility fallback strings.

### Task 4: Align updater naming and release documentation

**Files:**
- Modify: `.github/workflows/release.yml`, `README.md`, `src-tauri/tauri.conf.json` only where updater copy/URL naming is present.

**Interfaces:**
- Consumes: public `reoniazx/EsanOne-Releases` release channel and the existing signed updater artifacts.
- Produces: workflow references `secrets.ESANONE_CONVERT_RELEASES_TOKEN`, while the public endpoint remains unchanged.

- [x] **Step 1: Change the workflow secret reference**

Replace both workflow references with:

```yaml
GITHUB_TOKEN: ${{ secrets.ESANONE_CONVERT_RELEASES_TOKEN }}
GH_TOKEN: ${{ secrets.ESANONE_CONVERT_RELEASES_TOKEN }}
```

- [x] **Step 2: Update the secret name in README**

Document `ESANONE_CONVERT_RELEASES_TOKEN` with the least-privilege `Contents: Read and write` scope on `reoniazx/EsanOne-Releases`.

- [x] **Step 3: Validate updater endpoint and workflow syntax**

Run:

```powershell
python -c "import yaml; yaml.safe_load(open('.github/workflows/release.yml', encoding='utf-8')); print('workflow yaml: ok')"
$config = Get-Content -Raw 'src-tauri\tauri.conf.json' | ConvertFrom-Json
$config.plugins.updater.endpoints
```

Expected: YAML parses, and the endpoint remains `https://github.com/reoniazx/EsanOne-Releases/releases/latest/download/latest.json`.

### Task 5: Final verification and compatibility report

**Files:**
- Test: all existing frontend and Rust test suites plus the final identity scan.

**Interfaces:**
- Consumes: all renamed metadata, UI, shell, docs, and workflow changes.
- Produces: evidence that the new identity builds and that compatibility-sensitive `stem` contracts remain present.

- [x] **Step 1: Run the full verification**

```powershell
npm test
npm run build
cargo test --manifest-path src-tauri\Cargo.toml
```

Expected: all tests pass, build exits 0, and no Rust test regressions occur.

- [x] **Step 2: Verify public identity and preserved domain contracts**

```powershell
rg -n "EsanOne Convert|esanone-convert|com\.esanone\.convert" package.json src-tauri src index.html README.md .github
rg -n "stem-separate-event|stem-tool-install-event|stem-tools|stems" src src-tauri
```

Expected: new identity appears in public surfaces and old stem contracts remain available.

- [x] **Step 3: Report external release prerequisites**

Confirm that `ESANONE_CONVERT_RELEASES_TOKEN` exists by name only, never print its value, and report whether the source repository has received the workflow commit and whether a public release exists.
