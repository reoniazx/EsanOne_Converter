//! Local HT-Demucs stem separation + stem-tool install (Python → demucs).
//!
//! Mirrors [`crate::ai_agent`]: spawn external processes, stream log lines as
//! Tauri events, expose start/stop/status commands to the UI.

use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, State};
use zip::ZipArchive;

pub const STEM_TOOL_INSTALL_EVENT: &str = "stem-tool-install-event";
pub const STEM_TOOL_INSTALL_DONE: &str = "stem-tool-install-done";
pub const STEM_GPU_INSTALL_EVENT: &str = "stem-gpu-install-event";
pub const STEM_GPU_INSTALL_DONE: &str = "stem-gpu-install-done";
pub const STEM_SEPARATE_EVENT: &str = "stem-separate-event";
pub const STEM_SEPARATE_DONE: &str = "stem-separate-done";

const MIN_PYTHON_MINOR: u32 = 10;
const PYTHON_WINGET_ID: &str = "Python.Python.3.12";
/// Official Windows amd64 installer (3.12.10). Fallback when winget is missing.
const PYTHON_INSTALLER_URL: &str =
    "https://www.python.org/ftp/python/3.12.10/python-3.12.10-amd64.exe";
const PYTHON_INSTALLER_SHA256: &str =
    "67b5635e80ea51072b87941312d00ec8927c4db9ba18938f7ad2d27b328b95fb";
const FFMPEG_RELEASE_ZIP_URL: &str =
    "https://www.gyan.dev/ffmpeg/builds/packages/ffmpeg-8.1.2-essentials_build.zip";
const FFMPEG_RELEASE_SHA256: &str =
    "db580001caa24ac104c8cb856cd113a87b0a443f7bdf47d8c12b1d740584a2ec";
const MAX_PYTHON_INSTALLER_BYTES: u64 = 64 * 1024 * 1024;
const MAX_FFMPEG_ARCHIVE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_FFMPEG_ARCHIVE_ENTRIES: usize = 64;
const MAX_FFMPEG_EXTRACTED_BYTES: u64 = 512 * 1024 * 1024;
pub const STEM_TOOLS_DIR: &str = "EsanOne Stem";
const PYTORCH_CUDA_INDEX_URL: &str = "https://download.pytorch.org/whl/cu128";
const PYTORCH_TORCH_VERSION: &str = "2.7.1";
const PYTORCH_TORCHAUDIO_VERSION: &str = "2.7.1";
const CUDA_MIN_FULL_QUALITY_VRAM_MB: u64 = 3072;
const NVIDIA_VENDOR_ID: u32 = 0x10DE;

// --- MelBandRoFormer (local vocal separation) ---
//
// Distinct backend from HT-Demucs: writes its own runtime Python wrapper and
// does not require Demucs to be installed. See
// docs/superpowers/specs/2026-07-11-vocal-and-instrument-stem-separation-design.md.

const MELBANDROFORMER_MODEL: &str = "melbandroformer_vocals";
const MELBANDROFORMER_WRAPPER_SRC: &str = include_str!("../resources/melbandroformer_wrapper.py");
const MELBANDROFORMER_WEIGHTS_FILENAME: &str = "MelBandRoformer_fp16.safetensors";
/// Pinned commit (not `main`) so the download is reproducible.
const MELBANDROFORMER_WEIGHTS_REVISION: &str = "7dc5fa7824f1f3089a5c4b130d767004ccc1ed12";
/// Verified against the Hugging Face LFS object's sha256 for this revision.
const MELBANDROFORMER_WEIGHTS_SHA256: &str =
    "6119aef379a6c7264e0b37db65ae1e6488b8ca4a00baf56d6d244737b8488226";
const MELBANDROFORMER_WEIGHTS_BYTES: u64 = 456_479_072;
const MAX_MELBANDROFORMER_WEIGHTS_BYTES: u64 = 512 * 1024 * 1024;

fn melbandroformer_weights_url() -> String {
    format!(
        "https://huggingface.co/Kijai/MelBandRoFormer_comfy/resolve/{MELBANDROFORMER_WEIGHTS_REVISION}/{MELBANDROFORMER_WEIGHTS_FILENAME}"
    )
}

fn is_melbandroformer(model: &str) -> bool {
    model == MELBANDROFORMER_MODEL
}

fn validate_stem_model(model: &str) -> Result<(), String> {
    match model {
        MELBANDROFORMER_MODEL | "htdemucs_ft" | "htdemucs_6s" => Ok(()),
        other => Err(format!("Unknown separation model: {other}")),
    }
}

fn write_melbandroformer_wrapper() -> Result<PathBuf, String> {
    let path = std::env::temp_dir().join("esan_melbandroformer_wrapper.py");
    std::fs::write(&path, MELBANDROFORMER_WRAPPER_SRC)
        .map_err(|error| format!("Cannot write MelBandRoFormer wrapper: {error}"))?;
    Ok(path)
}

fn stem_app_data_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let local = std::env::var("LOCALAPPDATA").ok()?;
        return Some(PathBuf::from(local).join(STEM_TOOLS_DIR));
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").ok()?;
        return Some(
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join(STEM_TOOLS_DIR),
        );
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
            return Some(PathBuf::from(xdg).join(STEM_TOOLS_DIR));
        }
        let home = std::env::var("HOME").ok()?;
        return Some(
            PathBuf::from(home)
                .join(".local")
                .join("share")
                .join(STEM_TOOLS_DIR),
        );
    }
    #[cfg(not(any(windows, unix)))]
    {
        None
    }
}

fn stem_tools_root() -> Option<PathBuf> {
    Some(stem_app_data_dir()?.join("stem-tools"))
}

fn managed_python_path(root: &Path) -> PathBuf {
    let venv = root.join("python").join("venv");
    #[cfg(windows)]
    {
        return venv.join("Scripts").join("python.exe");
    }
    #[cfg(not(windows))]
    {
        venv.join("bin").join("python")
    }
}

fn managed_python() -> Option<(PathBuf, String)> {
    let root = stem_tools_root()?;
    let python = managed_python_path(&root);
    if !python.is_file() {
        return None;
    }
    probe_python_at(&python)
}

fn stem_tools_models_dir() -> Option<PathBuf> {
    Some(stem_tools_root()?.join("models"))
}

fn melbandroformer_weights_path() -> Option<PathBuf> {
    Some(stem_tools_models_dir()?.join(MELBANDROFORMER_WEIGHTS_FILENAME))
}

/// Pure readiness gate: vocal separation needs the narrow Python deps, ffmpeg
/// (librosa's audioread fallback decodes compressed inputs through it), and
/// the downloaded+verified weights file.
fn melbandroformer_ready(deps_ok: bool, ffmpeg_ok: bool, weights_ok: bool) -> bool {
    deps_ok && ffmpeg_ok && weights_ok
}

/// Validates a `purpose` string before any install work starts.
fn stem_install_backend(purpose: &str) -> Result<(), String> {
    match purpose {
        "vocals" | "instruments" | "instruments_6s" => Ok(()),
        other => Err(format!("Unknown separation purpose: {other}")),
    }
}

/// Lightweight readiness probe: `find_spec` only, no full model load (that
/// only happens once, at install time, before the weights file is renamed
/// into place).
fn probe_melbandroformer_deps(python: &Path) -> bool {
    run_capture(
        python,
        &[
            "-c",
            "import importlib.util as u; import sys; \
             mods = ['torch', 'safetensors', 'einops', 'rotary_embedding_torch', 'librosa', 'soundfile']; \
             sys.exit(0 if all(u.find_spec(m) is not None for m in mods) else 1)",
        ],
    )
    .map(|(code, _, _)| code == 0)
    .unwrap_or(false)
}

fn build_melbandroformer_args(
    wrapper: &str,
    weights: &str,
    output_dir: &str,
    input_path: &str,
    attempt: StemAttempt,
    chunk_seconds: &str,
) -> Vec<String> {
    let mut args = vec![
        wrapper.to_string(),
        "--weights".into(),
        weights.to_string(),
        "--input".into(),
        input_path.to_string(),
        "--output-dir".into(),
        output_dir.to_string(),
    ];
    match attempt {
        StemAttempt::CudaFull => {
            args.extend(["--device".into(), "cuda".into()]);
        }
        StemAttempt::CudaSegmented => {
            args.extend([
                "--device".into(),
                "cuda".into(),
                "--chunk-seconds".into(),
                chunk_seconds.into(),
            ]);
        }
        StemAttempt::Cpu => {
            args.extend(["--device".into(), "cpu".into()]);
        }
    }
    args
}

#[derive(Default, Clone)]
pub struct StemSeparateState {
    children: Arc<Mutex<HashMap<String, Child>>>,
    cancelled: Arc<Mutex<HashSet<String>>>,
    /// Only one install pipeline at a time (dialog + System Setup share this).
    install_session: Arc<Mutex<Option<String>>>,
    status_cache: Arc<Mutex<Option<StemToolStatus>>>,
}

#[derive(Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StemGpuStatus {
    pub torch_ok: bool,
    /// The torch package exists on disk even if importing it fails — this is
    /// what separates "repair the runtime" from "install stem tools first".
    pub torch_installed: bool,
    pub cuda_available: bool,
    pub has_nvidia_adapter: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub torch_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_mb: Option<u64>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StemToolStatus {
    pub demucs: bool,
    pub mel_band_ro_former: bool,
    pub python: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub python_version: Option<String>,
    pub gpu: StemGpuStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StemAttempt {
    CudaFull,
    CudaSegmented,
    Cpu,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AttemptOutcome {
    Success,
    Cancelled,
    Failed { oom: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StemNextAction {
    Retry(StemAttempt),
    FinishSuccess,
    FinishCancelled,
    FinishFailed,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LogEvent {
    session_id: String,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    progress: Option<f32>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct InstallDoneEvent {
    session_id: String,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    status: StemToolStatus,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SeparateDoneEvent {
    session_id: String,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    exit_code: Option<i32>,
    stem_paths: Vec<String>,
}

fn emit_log(
    app: &AppHandle,
    event_name: &str,
    session_id: &str,
    kind: &str,
    text: impl Into<String>,
) {
    let _ = app.emit(
        event_name,
        LogEvent {
            session_id: session_id.to_string(),
            kind: kind.to_string(),
            text: Some(text.into()),
            progress: None,
        },
    );
}

fn clamp_progress(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

fn emit_progress(
    app: &AppHandle,
    event_name: &str,
    session_id: &str,
    kind: &str,
    progress: f32,
    text: impl Into<String>,
) {
    let _ = app.emit(
        event_name,
        LogEvent {
            session_id: session_id.to_string(),
            kind: kind.to_string(),
            text: Some(text.into()),
            progress: Some(clamp_progress(progress)),
        },
    );
}

fn parse_progress_percent(text: &str) -> Option<f32> {
    let marker = text.find("%|")?;
    let before = &text[..marker];
    let start = before
        .rfind(|ch: char| !ch.is_ascii_digit())
        .map(|index| index + 1)
        .unwrap_or(0);
    before[start..]
        .parse::<f32>()
        .ok()
        .map(|value| clamp_progress(value / 100.0))
}

/// Find `name` on `PATH`, honoring Windows `PATHEXT`.
fn resolve_program(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    #[cfg(windows)]
    let extensions: Vec<String> = std::env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
        .split(';')
        .filter(|ext| !ext.is_empty())
        .map(|ext| ext.to_string())
        .collect();
    for dir in std::env::split_paths(&path) {
        let direct = dir.join(name);
        if direct.is_file() {
            return Some(direct);
        }
        #[cfg(windows)]
        for ext in &extensions {
            let candidate = dir.join(format!("{name}{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Append Machine+User PATH dirs from the registry without dropping session PATH.
#[cfg(windows)]
fn refresh_path_from_registry() {
    use std::os::windows::process::CommandExt;
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "[Environment]::GetEnvironmentVariable('Path','Machine') + ';' + [Environment]::GetEnvironmentVariable('Path','User')",
        ])
        .creation_flags(0x0800_0000)
        .output();
    let Ok(out) = output else {
        return;
    };
    if !out.status.success() {
        return;
    }
    let registry = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if registry.is_empty() {
        return;
    }
    let current = std::env::var("PATH").unwrap_or_default();
    let mut merged = current;
    for dir in registry.split(';') {
        let dir = dir.trim();
        if dir.is_empty() {
            continue;
        }
        let already =
            std::env::split_paths(&merged).any(|p| p.as_os_str() == std::ffi::OsStr::new(dir));
        if already {
            continue;
        }
        if !merged.is_empty() && !merged.ends_with(';') {
            merged.push(';');
        }
        merged.push_str(dir);
    }
    std::env::set_var("PATH", merged);
}

#[cfg(not(windows))]
fn refresh_path_from_registry() {}

/// Prepend dirs onto PATH if missing (case-insensitive on Windows).
fn prepend_path_dirs(dirs: &[PathBuf]) {
    if dirs.is_empty() {
        return;
    }
    let current = std::env::var("PATH").unwrap_or_default();
    let mut prefix = String::new();
    for dir in dirs {
        let s = dir.to_string_lossy();
        if s.is_empty() {
            continue;
        }
        let already = std::env::split_paths(&current)
            .chain(std::env::split_paths(&prefix))
            .any(|p| {
                #[cfg(windows)]
                {
                    p.to_string_lossy().eq_ignore_ascii_case(&s)
                }
                #[cfg(not(windows))]
                {
                    p.as_os_str() == dir.as_os_str()
                }
            });
        if already {
            continue;
        }
        #[cfg(windows)]
        let sep = ';';
        #[cfg(not(windows))]
        let sep = ':';
        if !prefix.is_empty() && !prefix.ends_with(sep) {
            prefix.push(sep);
        }
        prefix.push_str(&s);
    }
    if prefix.is_empty() {
        return;
    }
    #[cfg(windows)]
    let sep = ';';
    #[cfg(not(windows))]
    let sep = ':';
    if current.is_empty() {
        std::env::set_var("PATH", prefix);
    } else {
        std::env::set_var("PATH", format!("{prefix}{sep}{current}"));
    }
}

/// Production GUI apps often inherit a stale Explorer PATH. Refresh registry PATH
/// and discover common winget/Python install locations so status/separate work
/// without requiring a full OS logoff.
fn ensure_stem_env() {
    refresh_path_from_registry();
    prepend_path_dirs(&discover_stem_bin_dirs());
}

fn discover_stem_bin_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    #[cfg(windows)]
    {
        let Ok(local) = std::env::var("LOCALAPPDATA") else {
            return dirs;
        };
        let local = PathBuf::from(local);

        let python_root = local.join("Programs").join("Python");
        if let Ok(entries) = std::fs::read_dir(&python_root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                if path.join("python.exe").is_file() {
                    dirs.push(path.clone());
                }
                let scripts = path.join("Scripts");
                if scripts.is_dir() {
                    dirs.push(scripts);
                }
            }
        }

        let winget = local.join("Microsoft").join("WinGet").join("Packages");
        if let Ok(packages) = std::fs::read_dir(&winget) {
            for package in packages.flatten() {
                let name = package.file_name().to_string_lossy().to_ascii_lowercase();
                if !name.starts_with("gyan.ffmpeg") {
                    continue;
                }
                if let Ok(inners) = std::fs::read_dir(package.path()) {
                    for inner in inners.flatten() {
                        let bin = inner.path().join("bin");
                        if bin.join("ffmpeg.exe").is_file() {
                            dirs.push(bin);
                        }
                    }
                }
            }
        }

        if let Some(bin) = stem_tools_ffmpeg_bin_dir() {
            if bin.join("ffmpeg.exe").is_file() || bin.join("ffmpeg").is_file() {
                dirs.push(bin);
            }
        }
    }

    #[cfg(not(windows))]
    {
        if let Some(root) = stem_tools_root() {
            let bin = root.join("bin");
            if bin.is_dir() {
                dirs.push(bin);
            }
            let ffmpeg_bin = root.join("ffmpeg").join("bin");
            if ffmpeg_bin.is_dir() {
                dirs.push(ffmpeg_bin);
            }
            let py = root.join("python");
            if py.is_dir() {
                dirs.push(py.clone());
                let scripts = py.join("bin");
                if scripts.is_dir() {
                    dirs.push(scripts);
                }
            }
        }
        for extra in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"] {
            let p = PathBuf::from(extra);
            if p.is_dir() {
                dirs.push(p);
            }
        }
    }

    dirs
}

fn stem_tools_ffmpeg_bin_dir() -> Option<PathBuf> {
    Some(stem_tools_root()?.join("ffmpeg").join("bin"))
}

fn run_capture(program: &Path, args: &[&str]) -> Result<(i32, String, String), String> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let output = command
        .output()
        .map_err(|error| format!("Failed to run {}: {error}", program.display()))?;
    let code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Ok((code, stdout, stderr))
}

fn ensure_size_within_limit(size: u64, limit: u64, artifact: &str) -> Result<(), String> {
    if size <= limit {
        Ok(())
    } else {
        Err(format!("{artifact} exceeded its {limit}-byte safety limit"))
    }
}

fn download_bytes(url: &str, artifact: &str, limit: u64) -> Result<Vec<u8>, String> {
    tauri::async_runtime::block_on(async {
        let mut response = reqwest::get(url)
            .await
            .map_err(|error| format!("{artifact} download failed: {error}"))?;
        if !response.status().is_success() {
            return Err(format!(
                "{artifact} download returned HTTP {}",
                response.status()
            ));
        }
        if let Some(content_length) = response.content_length() {
            ensure_size_within_limit(content_length, limit, artifact)?;
        }

        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| format!("{artifact} download body failed: {error}"))?
        {
            let next_len = bytes
                .len()
                .checked_add(chunk.len())
                .ok_or_else(|| format!("{artifact} exceeded its safety limit"))?;
            ensure_size_within_limit(next_len as u64, limit, artifact)?;
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    })
}

fn verify_download_sha256(bytes: &[u8], expected: &str, artifact: &str) -> Result<(), String> {
    let expected = expected
        .split_whitespace()
        .next()
        .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| format!("{artifact} checksum was invalid"))?;
    let actual = format!("{:x}", Sha256::digest(bytes));
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(format!(
            "{artifact} checksum did not match; download was not used"
        ))
    }
}

/// Extract only the release archive's top-level `bin` files. Keeping the install
/// root fixed makes it discoverable on every later app launch and avoids Zip Slip.
fn extract_ffmpeg_bin_from_zip(archive_bytes: &[u8], bin_dir: &Path) -> Result<(), String> {
    let mut archive = ZipArchive::new(Cursor::new(archive_bytes))
        .map_err(|error| format!("Could not open FFmpeg archive: {error}"))?;
    if archive.len() > MAX_FFMPEG_ARCHIVE_ENTRIES {
        return Err(format!(
            "FFmpeg archive has too many entries (maximum {MAX_FFMPEG_ARCHIVE_ENTRIES})"
        ));
    }
    std::fs::create_dir_all(bin_dir)
        .map_err(|error| format!("Could not create FFmpeg install folder: {error}"))?;

    let mut found_ffmpeg = false;
    let mut extracted_bytes = 0u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("Could not read FFmpeg archive entry: {error}"))?;
        if entry.is_dir() {
            continue;
        }
        let Some(path) = entry.enclosed_name() else {
            continue;
        };
        let is_bin_file = path.components().count() == 3
            && path
                .parent()
                .and_then(Path::file_name)
                .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("bin"));
        if !is_bin_file {
            continue;
        }
        let Some(file_name) = path.file_name() else {
            continue;
        };
        extracted_bytes = extracted_bytes
            .checked_add(entry.size())
            .ok_or_else(|| "FFmpeg archive extraction size overflowed".to_string())?;
        ensure_size_within_limit(
            extracted_bytes,
            MAX_FFMPEG_EXTRACTED_BYTES,
            "FFmpeg archive extraction",
        )?;
        let destination = bin_dir.join(file_name);
        let mut file = std::fs::File::create(&destination)
            .map_err(|error| format!("Could not write {}: {error}", destination.display()))?;
        std::io::copy(&mut entry, &mut file)
            .map_err(|error| format!("Could not unpack {}: {error}", destination.display()))?;
        if file_name
            .to_string_lossy()
            .eq_ignore_ascii_case("ffmpeg.exe")
        {
            found_ffmpeg = true;
        }
    }

    if found_ffmpeg {
        Ok(())
    } else {
        Err("FFmpeg archive did not contain bin\\ffmpeg.exe".into())
    }
}

fn parse_python_version(version_line: &str) -> Option<(u32, u32)> {
    let rest = version_line.trim().strip_prefix("Python ")?;
    let mut parts = rest.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
}

fn python_version_ok(version_line: &str) -> bool {
    matches!(parse_python_version(version_line), Some((3, minor)) if minor >= MIN_PYTHON_MINOR)
}

fn probe_python_at(program: &Path) -> Option<(PathBuf, String)> {
    let (code, stdout, stderr) = run_capture(program, &["--version"]).ok()?;
    if code != 0 {
        return None;
    }
    let version_line = if !stdout.is_empty() { stdout } else { stderr };
    if !python_version_ok(&version_line) {
        return None;
    }
    Some((program.to_path_buf(), version_line))
}

/// True Python 3.10+ from the host, used only to bootstrap the app-owned venv.
fn probe_system_python() -> Option<(PathBuf, String)> {
    let candidates: Vec<(PathBuf, Vec<&str>)> = {
        let mut list = Vec::new();
        if let Some(py) = resolve_program("py") {
            list.push((py, vec!["-3", "--version"]));
        }
        if let Some(python) = resolve_program("python") {
            list.push((python, vec!["--version"]));
        }
        if let Some(python3) = resolve_program("python3") {
            list.push((python3, vec!["--version"]));
        }
        list
    };

    for (program, args) in candidates {
        if let Ok((code, stdout, stderr)) = run_capture(&program, &args) {
            if code != 0 {
                continue;
            }
            let version_line = if !stdout.is_empty() { stdout } else { stderr };
            if !python_version_ok(&version_line) {
                continue;
            }
            if program
                .file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.eq_ignore_ascii_case("py"))
            {
                if let Ok((c, out, _)) =
                    run_capture(&program, &["-3", "-c", "import sys; print(sys.executable)"])
                {
                    if c == 0 {
                        let exe = out.trim();
                        if !exe.is_empty() {
                            return Some((PathBuf::from(exe), version_line));
                        }
                    }
                }
            }
            return Some((program, version_line));
        }
    }
    None
}

fn parse_stem_gpu_probe(raw: &str) -> StemGpuStatus {
    #[derive(serde::Deserialize)]
    struct Probe {
        #[serde(default)]
        torch_ok: Option<bool>,
        #[serde(default)]
        torch_installed: Option<bool>,
        cuda_available: bool,
        torch_version: Option<String>,
        device_name: Option<String>,
        memory_mb: Option<u64>,
    }
    let Ok(probe) = serde_json::from_str::<Probe>(raw.trim()) else {
        return StemGpuStatus::default();
    };
    let torch_ok = probe.torch_ok.unwrap_or(true);
    StemGpuStatus {
        torch_ok,
        torch_installed: probe.torch_installed.unwrap_or(torch_ok),
        cuda_available: probe.cuda_available,
        has_nvidia_adapter: false,
        torch_version: probe.torch_version,
        device_name: probe.device_name,
        memory_mb: probe.memory_mb,
    }
}

fn has_nvidia_adapter() -> bool {
    static CACHE: OnceLock<bool> = OnceLock::new();
    *CACHE.get_or_init(|| {
        std::panic::catch_unwind(|| {
            let instance = wgpu::Instance::default();
            instance
                .enumerate_adapters(wgpu::Backends::all())
                .into_iter()
                .any(|adapter| adapter.get_info().vendor == NVIDIA_VENDOR_ID)
        })
        .unwrap_or(false)
    })
}

fn probe_stem_gpu(python: Option<&Path>) -> StemGpuStatus {
    let nvidia = has_nvidia_adapter();
    let Some(python) = python else {
        return StemGpuStatus {
            has_nvidia_adapter: nvidia,
            ..StemGpuStatus::default()
        };
    };
    const SCRIPT: &str = concat!(
        "import json\n",
        "try:\n",
        "    import torch\n",
        "    ok = bool(torch.cuda.is_available())\n",
        "    payload = {\"torch_ok\": True, \"torch_installed\": True, \"cuda_available\": ok, \"torch_version\": torch.__version__}\n",
        "    if ok:\n",
        "        props = torch.cuda.get_device_properties(0)\n",
        "        payload.update({\"device_name\": props.name, \"memory_mb\": props.total_memory // (1024 * 1024)})\n",
        "except Exception as error:\n",
        "    try:\n",
        "        import importlib.util\n",
        "        installed = importlib.util.find_spec(\"torch\") is not None\n",
        "    except Exception:\n",
        "        installed = False\n",
        "    payload = {\"torch_ok\": False, \"torch_installed\": installed, \"cuda_available\": False, \"error\": str(error)[:200]}\n",
        "print(json.dumps(payload))\n",
    );
    let mut status = match run_capture(python, &["-c", SCRIPT]) {
        Ok((0, stdout, _)) => parse_stem_gpu_probe(&stdout),
        _ => StemGpuStatus::default(),
    };
    status.has_nvidia_adapter = nvidia;
    status
}

fn initial_stem_attempt(gpu: &StemGpuStatus, force_cpu: bool) -> StemAttempt {
    if force_cpu || !gpu.cuda_available {
        return StemAttempt::Cpu;
    }
    match gpu.memory_mb {
        Some(mb) if mb < CUDA_MIN_FULL_QUALITY_VRAM_MB => StemAttempt::CudaSegmented,
        _ => StemAttempt::CudaFull,
    }
}

fn next_stem_attempt(current: StemAttempt, was_oom: bool) -> Option<StemAttempt> {
    if !was_oom {
        return None;
    }
    match current {
        StemAttempt::CudaFull => Some(StemAttempt::CudaSegmented),
        StemAttempt::CudaSegmented => Some(StemAttempt::Cpu),
        StemAttempt::Cpu => None,
    }
}

fn after_stem_attempt(
    current: StemAttempt,
    outcome: AttemptOutcome,
    cancel_pending: bool,
) -> StemNextAction {
    if cancel_pending || matches!(outcome, AttemptOutcome::Cancelled) {
        return StemNextAction::FinishCancelled;
    }
    match outcome {
        AttemptOutcome::Success => StemNextAction::FinishSuccess,
        AttemptOutcome::Failed { oom } => match next_stem_attempt(current, oom) {
            Some(next) => StemNextAction::Retry(next),
            None => StemNextAction::FinishFailed,
        },
        AttemptOutcome::Cancelled => StemNextAction::FinishCancelled,
    }
}

fn is_cuda_out_of_memory(stderr_lines: &[String]) -> bool {
    stderr_lines.iter().rev().any(|line| {
        let lower = line.to_ascii_lowercase();
        lower.contains("out of memory")
            || lower.contains("cuda error: out of memory")
            || lower.contains("cudnn_status_alloc_failed")
            || lower.contains("cublas_status_alloc_failed")
    })
}

fn cuda_retry_segment_seconds(memory_mb: Option<u64>) -> &'static str {
    match memory_mb {
        Some(mb) if mb >= 6144 => "6",
        _ => "4",
    }
}

fn build_demucs_args(
    wrapper: &str,
    model: &str,
    output_dir: &str,
    input_path: &str,
    attempt: StemAttempt,
    segment_seconds: &str,
) -> Vec<String> {
    let mut args = vec![
        wrapper.to_string(),
        "-n".into(),
        model.to_string(),
        "-o".into(),
        output_dir.to_string(),
    ];
    match attempt {
        StemAttempt::CudaFull => {
            args.extend(["-d".into(), "cuda".into()]);
        }
        StemAttempt::CudaSegmented => {
            args.extend([
                "-d".into(),
                "cuda".into(),
                "--segment".into(),
                segment_seconds.into(),
            ]);
        }
        StemAttempt::Cpu => {
            args.extend(["-d".into(), "cpu".into()]);
        }
    }
    args.push(input_path.to_string());
    args
}

fn torch_pair_args(target: &str) -> Result<Vec<String>, String> {
    let mut args = vec![
        "-m".into(),
        "pip".into(),
        "install".into(),
        "--upgrade".into(),
        "--force-reinstall".into(),
        "--no-cache-dir".into(),
        format!("torch=={PYTORCH_TORCH_VERSION}"),
        format!("torchaudio=={PYTORCH_TORCHAUDIO_VERSION}"),
    ];
    match target {
        "cuda" => {
            args.push("--index-url".into());
            args.push(PYTORCH_CUDA_INDEX_URL.into());
            Ok(args)
        }
        "cpu" => Ok(args),
        other => Err(format!("Unknown torch install target: {other}")),
    }
}

fn current_status(state: &StemSeparateState, force: bool) -> StemToolStatus {
    if !force {
        if let Some(cached) = state.status_cache.lock().unwrap().clone() {
            return cached;
        }
    }
    ensure_stem_env();
    let python = managed_python();
    let demucs_ok = python
        .as_ref()
        .map(|(path, _)| probe_demucs_module(path))
        .unwrap_or(false);
    let ffmpeg_ok = probe_ffmpeg();
    let melbandroformer_deps_ok = python
        .as_ref()
        .map(|(path, _)| probe_melbandroformer_deps(path))
        .unwrap_or(false);
    let melbandroformer_weights_ok = melbandroformer_weights_path()
        .map(|path| path.is_file())
        .unwrap_or(false);
    let gpu = probe_stem_gpu(python.as_ref().map(|(path, _)| path.as_path()));
    let status = StemToolStatus {
        // Ready when Python can import demucs and ffmpeg CLI can decode audio.
        demucs: demucs_ok && ffmpeg_ok,
        mel_band_ro_former: melbandroformer_ready(
            melbandroformer_deps_ok,
            ffmpeg_ok,
            melbandroformer_weights_ok,
        ),
        python: python.is_some(),
        python_version: python.map(|(_, v)| v),
        gpu,
    };
    *state.status_cache.lock().unwrap() = Some(status.clone());
    status
}

fn invalidate_status_cache(state: &StemSeparateState) {
    *state.status_cache.lock().unwrap() = None;
}

fn probe_ffmpeg() -> bool {
    let Some(ffmpeg) = resolve_program("ffmpeg") else {
        return false;
    };
    run_capture(&ffmpeg, &["-version"])
        .map(|(code, _, _)| code == 0)
        .unwrap_or(false)
}

fn probe_demucs_module(python: &Path) -> bool {
    run_capture(python, &["-c", "import demucs.separate, soundfile"])
        .map(|(code, _, _)| code == 0)
        .unwrap_or(false)
}

/// torchaudio 2.11+ routes save/load through torchcodec, which needs shared
/// FFmpeg DLLs on Windows. Patch `torchaudio.save` to soundfile so demucs can
/// finish writing stems after ffmpeg-based decode.
const DEMUCS_WRAPPER: &str = r#"
import soundfile as sf
import torchaudio

def _esan_save(uri, src, sample_rate, **kwargs):
    path = str(uri)
    data = src.detach().cpu().numpy()
    if data.ndim == 2:
        data = data.T
    bits = kwargs.get("bits_per_sample", 16)
    encoding = kwargs.get("encoding")
    subtype = "FLOAT" if encoding == "PCM_F" or bits == 32 else "PCM_16"
    sf.write(path, data, int(sample_rate), subtype=subtype)

torchaudio.save = _esan_save

from demucs.separate import main

if __name__ == "__main__":
    main()
"#;

fn write_demucs_wrapper() -> Result<PathBuf, String> {
    let path = std::env::temp_dir().join("esan_demucs_wrapper.py");
    std::fs::write(&path, DEMUCS_WRAPPER)
        .map_err(|error| format!("Cannot write demucs wrapper: {error}"))?;
    Ok(path)
}

pub(crate) fn configure_python_utf8(command: &mut Command) {
    command
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONUTF8", "1");
}

#[tauri::command]
pub async fn stem_tool_status(
    state: State<'_, StemSeparateState>,
    force: Option<bool>,
) -> Result<StemToolStatus, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || current_status(&state, force.unwrap_or(false)))
        .await
        .map_err(|e| e.to_string())
}

/// Read a pipe and emit on `\n` or `\r` so tqdm/demucs progress is visible.
fn stream_pipe_progress(
    app: AppHandle,
    event_name: &'static str,
    session_id: String,
    kind: &'static str,
    mut reader: impl Read + Send + 'static,
) {
    std::thread::spawn(move || {
        let mut buf = [0u8; 1024];
        let mut acc = String::new();
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let chunk = String::from_utf8_lossy(&buf[..n]);
                    for ch in chunk.chars() {
                        if ch == '\r' || ch == '\n' {
                            let line = acc.trim();
                            // pip's "[notice] A new release of pip…" is noise, not failure.
                            if !line.is_empty() && !line.starts_with("[notice]") {
                                if let Some(progress) = parse_progress_percent(line) {
                                    emit_progress(
                                        &app,
                                        event_name,
                                        &session_id,
                                        kind,
                                        progress,
                                        line,
                                    );
                                } else {
                                    emit_log(&app, event_name, &session_id, kind, line);
                                }
                            }
                            acc.clear();
                        } else if !ch.is_control() {
                            acc.push(ch);
                        }
                    }
                }
                Err(_) => break,
            }
        }
        let line = acc.trim();
        if !line.is_empty() {
            if let Some(progress) = parse_progress_percent(line) {
                emit_progress(&app, event_name, &session_id, kind, progress, line);
            } else {
                emit_log(&app, event_name, &session_id, kind, line);
            }
        }
    });
}

fn stream_child_logs(
    app: AppHandle,
    event_name: &'static str,
    session_id: String,
    mut child: Child,
    state: StemSeparateState,
    on_exit: impl FnOnce(Option<i32>, Vec<String>) + Send + 'static,
) {
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    // Do not clear cancelled here — a cancel set between attempts must survive
    // until the retry loop reads it via after_stem_attempt.
    state
        .children
        .lock()
        .unwrap()
        .insert(session_id.clone(), child);

    let recent_stderr: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    if let Some(stderr) = stderr {
        let app = app.clone();
        let session_id = session_id.clone();
        let recent = recent_stderr.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 1024];
            let mut acc = String::new();
            let mut reader = stderr;
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let chunk = String::from_utf8_lossy(&buf[..n]);
                        for ch in chunk.chars() {
                            if ch == '\r' || ch == '\n' {
                                let line = acc.trim().to_string();
                                if !line.is_empty() {
                                    emit_log(&app, event_name, &session_id, "stderr", &line);
                                    let mut guard = recent.lock().unwrap();
                                    guard.push(line);
                                    if guard.len() > 40 {
                                        let skip = guard.len() - 40;
                                        guard.drain(0..skip);
                                    }
                                }
                                acc.clear();
                            } else if !ch.is_control() {
                                acc.push(ch);
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
            let line = acc.trim().to_string();
            if !line.is_empty() {
                emit_log(&app, event_name, &session_id, "stderr", &line);
                recent.lock().unwrap().push(line);
            }
        });
    }
    if let Some(stdout) = stdout {
        stream_pipe_progress(
            app.clone(),
            event_name,
            session_id.clone(),
            "stdout",
            stdout,
        );
    }

    let children = state.children.clone();
    let cancelled = state.cancelled.clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(150));
            let was_cancelled = cancelled.lock().unwrap().contains(&session_id);
            let mut guard = children.lock().unwrap();
            let Some(child) = guard.get_mut(&session_id) else {
                drop(guard);
                on_exit(None, recent_stderr.lock().unwrap().clone());
                return;
            };
            match child.try_wait() {
                Ok(Some(status)) => {
                    guard.remove(&session_id);
                    drop(guard);
                    let cancel_at_exit = cancelled.lock().unwrap().remove(&session_id);
                    // Give stderr reader a moment to flush.
                    std::thread::sleep(Duration::from_millis(50));
                    let lines = recent_stderr.lock().unwrap().clone();
                    if was_cancelled || cancel_at_exit {
                        on_exit(None, lines);
                    } else {
                        on_exit(status.code(), lines);
                    }
                    return;
                }
                Ok(None) => {
                    if was_cancelled {
                        let _ = child.kill();
                    }
                }
                Err(_) => {
                    guard.remove(&session_id);
                    drop(guard);
                    on_exit(None, recent_stderr.lock().unwrap().clone());
                    return;
                }
            }
        }
    });
}

fn spawn_logged(program: &Path, args: &[&str]) -> Result<Child, String> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_python_utf8(&mut command);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
        .spawn()
        .map_err(|error| format!("Failed to start {}: {error}", program.display()))
}

/// Run a child while registered in `state` so [`stem_separate_stop`] can kill it.
fn run_tracked_child(
    state: &StemSeparateState,
    session_id: &str,
    mut child: Child,
    app: &AppHandle,
    event_name: &'static str,
) -> Result<i32, String> {
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    state.cancelled.lock().unwrap().remove(session_id);
    state
        .children
        .lock()
        .unwrap()
        .insert(session_id.to_string(), child);

    if let Some(stdout) = stdout {
        stream_pipe_progress(
            app.clone(),
            event_name,
            session_id.to_string(),
            "stdout",
            stdout,
        );
    }
    if let Some(stderr) = stderr {
        stream_pipe_progress(
            app.clone(),
            event_name,
            session_id.to_string(),
            "stderr",
            stderr,
        );
    }

    loop {
        std::thread::sleep(Duration::from_millis(150));
        if state.cancelled.lock().unwrap().contains(session_id) {
            if let Some(mut child) = state.children.lock().unwrap().remove(session_id) {
                let _ = child.kill();
                let _ = child.wait();
            }
            state.cancelled.lock().unwrap().remove(session_id);
            return Err("Cancelled".into());
        }
        let mut guard = state.children.lock().unwrap();
        let Some(child) = guard.get_mut(session_id) else {
            return Err("Cancelled".into());
        };
        match child.try_wait() {
            Ok(Some(status)) => {
                guard.remove(session_id);
                drop(guard);
                if state.cancelled.lock().unwrap().remove(session_id) {
                    return Err("Cancelled".into());
                }
                return Ok(status.code().unwrap_or(-1));
            }
            Ok(None) => {}
            Err(error) => {
                guard.remove(session_id);
                return Err(format!("Process wait failed: {error}"));
            }
        }
    }
}

fn ensure_managed_python(
    app: &AppHandle,
    state: &StemSeparateState,
    session_id: &str,
    bootstrap: &Path,
) -> Result<(PathBuf, String), String> {
    let root = stem_tools_root()
        .ok_or_else(|| "Could not resolve the EsanOne Converter tools folder.".to_string())?;
    let venv_dir = root.join("python").join("venv");
    let managed = managed_python_path(&root);
    if let Some(found) = probe_python_at(&managed) {
        return Ok(found);
    }

    if venv_dir.exists() {
        std::fs::remove_dir_all(&venv_dir)
            .map_err(|error| format!("Cannot repair managed Python environment: {error}"))?;
    }
    if let Some(parent) = venv_dir.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Cannot create managed Python folder: {error}"))?;
    }
    emit_log(
        app,
        STEM_TOOL_INSTALL_EVENT,
        session_id,
        "status",
        "Creating app-owned Python environment…",
    );
    let target = venv_dir.to_string_lossy().to_string();
    let args = ["-m", "venv", target.as_str()];
    let child = spawn_logged(bootstrap, &args)?;
    let code = run_tracked_child(state, session_id, child, app, STEM_TOOL_INSTALL_EVENT)?;
    if code != 0 {
        return Err(format!("Python venv creation failed (exit {code})"));
    }
    probe_python_at(&managed).ok_or_else(|| {
        "Python venv was created but its interpreter could not be started.".to_string()
    })
}

#[cfg(windows)]
fn install_python_windows(
    app: &AppHandle,
    state: &StemSeparateState,
    session_id: &str,
) -> Result<(), String> {
    emit_log(
        app,
        STEM_TOOL_INSTALL_EVENT,
        session_id,
        "status",
        "Installing Python 3.12…",
    );

    if let Some(winget) = resolve_program("winget") {
        emit_log(
            app,
            STEM_TOOL_INSTALL_EVENT,
            session_id,
            "status",
            "Using winget…",
        );
        let mut command = Command::new(&winget);
        command
            .args([
                "install",
                "-e",
                "--id",
                PYTHON_WINGET_ID,
                "--accept-package-agreements",
                "--accept-source-agreements",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let child = command
            .spawn()
            .map_err(|error| format!("Failed to start winget: {error}"))?;
        let code = run_tracked_child(state, session_id, child, app, STEM_TOOL_INSTALL_EVENT)?;
        ensure_stem_env();
        if probe_system_python().is_some() {
            return Ok(());
        }
        emit_log(
            app,
            STEM_TOOL_INSTALL_EVENT,
            session_id,
            "status",
            format!("winget finished (code {code}); trying official installer…"),
        );
    }

    emit_log(
        app,
        STEM_TOOL_INSTALL_EVENT,
        session_id,
        "status",
        "Downloading Python installer…",
    );
    let tmp = std::env::temp_dir().join("esan-python-3.12.10-amd64.exe");
    let bytes = download_bytes(
        PYTHON_INSTALLER_URL,
        "Python installer",
        MAX_PYTHON_INSTALLER_BYTES,
    )?;
    verify_download_sha256(&bytes, PYTHON_INSTALLER_SHA256, "Python installer")?;
    {
        let mut file = std::fs::File::create(&tmp)
            .map_err(|error| format!("Cannot write installer: {error}"))?;
        file.write_all(&bytes)
            .map_err(|error| format!("Cannot write installer: {error}"))?;
    }

    emit_log(
        app,
        STEM_TOOL_INSTALL_EVENT,
        session_id,
        "status",
        "Running Python installer (silent)…",
    );
    let mut command = Command::new(&tmp);
    command
        .args([
            "/quiet",
            "InstallAllUsers=0",
            "PrependPath=1",
            "Include_test=0",
            "Include_pip=1",
            "Include_launcher=1",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let child = command
        .spawn()
        .map_err(|error| format!("Python installer failed to start: {error}"))?;
    let code = run_tracked_child(state, session_id, child, app, STEM_TOOL_INSTALL_EVENT)?;
    ensure_stem_env();
    if probe_system_python().is_none() {
        return Err(format!(
            "Python installer finished (code {code}) but Python was not found. Try again or install Python manually."
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn install_python_windows(
    app: &AppHandle,
    state: &StemSeparateState,
    session_id: &str,
) -> Result<(), String> {
    emit_log(
        app,
        STEM_TOOL_INSTALL_EVENT,
        session_id,
        "status",
        "Looking for Python 3.10+…",
    );
    if probe_system_python().is_some() {
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(brew) = resolve_program("brew") {
            emit_log(
                app,
                STEM_TOOL_INSTALL_EVENT,
                session_id,
                "status",
                "Installing Python via Homebrew…",
            );
            let child = spawn_logged(&brew, &["install", "python@3.12"])?;
            let _ = run_tracked_child(state, session_id, child, app, STEM_TOOL_INSTALL_EVENT)?;
            ensure_stem_env();
            if probe_system_python().is_some() {
                return Ok(());
            }
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // Prefer a user-local uv-managed Python when available.
        if let Some(uv) = resolve_program("uv") {
            emit_log(
                app,
                STEM_TOOL_INSTALL_EVENT,
                session_id,
                "status",
                "Installing Python 3.12 via uv…",
            );
            let child = spawn_logged(&uv, &["python", "install", "3.12"])?;
            let _ = run_tracked_child(state, session_id, child, app, STEM_TOOL_INSTALL_EVENT)?;
            ensure_stem_env();
            if probe_system_python().is_some() {
                return Ok(());
            }
        }
        for manager in ["apt-get", "dnf", "pacman"] {
            if let Some(cmd) = resolve_program(manager) {
                emit_log(
                    app,
                    STEM_TOOL_INSTALL_EVENT,
                    session_id,
                    "status",
                    format!("Trying {manager} for python3…"),
                );
                let args: Vec<&str> = match manager {
                    "apt-get" => vec!["install", "-y", "python3", "python3-pip", "python3-venv"],
                    "dnf" => vec!["install", "-y", "python3", "python3-pip"],
                    "pacman" => vec!["-S", "--noconfirm", "python", "python-pip"],
                    _ => vec![],
                };
                if args.is_empty() {
                    continue;
                }
                let child = spawn_logged(&cmd, &args)?;
                let _ = run_tracked_child(state, session_id, child, app, STEM_TOOL_INSTALL_EVENT)?;
                ensure_stem_env();
                if probe_system_python().is_some() {
                    return Ok(());
                }
            }
        }
    }

    Err("Python 3.10+ was not found. Install Python (Homebrew / apt / uv), then try again.".into())
}

#[cfg(not(windows))]
fn install_ffmpeg_windows(
    app: &AppHandle,
    state: &StemSeparateState,
    session_id: &str,
) -> Result<(), String> {
    if probe_ffmpeg() {
        return Ok(());
    }
    emit_log(
        app,
        STEM_TOOL_INSTALL_EVENT,
        session_id,
        "status",
        "Installing ffmpeg…",
    );

    #[cfg(target_os = "macos")]
    {
        if let Some(brew) = resolve_program("brew") {
            let child = spawn_logged(&brew, &["install", "ffmpeg"])?;
            let _ = run_tracked_child(state, session_id, child, app, STEM_TOOL_INSTALL_EVENT)?;
            ensure_stem_env();
            if probe_ffmpeg() {
                return Ok(());
            }
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        for manager in ["apt-get", "dnf", "pacman"] {
            if let Some(cmd) = resolve_program(manager) {
                let args: Vec<&str> = match manager {
                    "apt-get" => vec!["install", "-y", "ffmpeg"],
                    "dnf" => vec!["install", "-y", "ffmpeg"],
                    "pacman" => vec!["-S", "--noconfirm", "ffmpeg"],
                    _ => vec![],
                };
                if args.is_empty() {
                    continue;
                }
                let child = spawn_logged(&cmd, &args)?;
                let _ = run_tracked_child(state, session_id, child, app, STEM_TOOL_INSTALL_EVENT)?;
                ensure_stem_env();
                if probe_ffmpeg() {
                    return Ok(());
                }
            }
        }
    }

    Err("ffmpeg is required. Install ffmpeg (Homebrew / apt), then try again.".into())
}

fn install_demucs(
    app: &AppHandle,
    state: &StemSeparateState,
    session_id: &str,
    python: &Path,
) -> Result<(), String> {
    emit_log(
        app,
        STEM_TOOL_INSTALL_EVENT,
        session_id,
        "status",
        "Installing demucs + soundfile (pip)… first run may download large packages",
    );
    let mut command = Command::new(python);
    // Demucs prefers ffmpeg CLI for decoding. Avoid depending on torchcodec on
    // Windows (needs shared FFmpeg DLLs that winget static builds don't provide).
    command
        .args(["-m", "pip", "install", "-U", "demucs", "soundfile"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let child = command
        .spawn()
        .map_err(|error| format!("Failed to start pip: {error}"))?;
    let code = run_tracked_child(state, session_id, child, app, STEM_TOOL_INSTALL_EVENT)?;
    refresh_path_from_registry();
    // Drop broken torchcodec wheels so torchaudio doesn't prefer them at save-time.
    let _ = run_capture(python, &["-m", "pip", "uninstall", "-y", "torchcodec"]);
    if code != 0 && !probe_demucs_module(python) {
        return Err(format!("pip install demucs failed (exit {code})"));
    }
    if !probe_demucs_module(python) {
        return Err("demucs installed but Python cannot import it. Try the install again.".into());
    }
    Ok(())
}

#[cfg(windows)]
fn install_ffmpeg_direct(app: &AppHandle, session_id: &str) -> Result<(), String> {
    emit_log(
        app,
        STEM_TOOL_INSTALL_EVENT,
        session_id,
        "status",
        "Downloading FFmpeg essentials build…",
    );
    let archive = download_bytes(
        FFMPEG_RELEASE_ZIP_URL,
        "FFmpeg archive",
        MAX_FFMPEG_ARCHIVE_BYTES,
    )?;
    verify_download_sha256(&archive, FFMPEG_RELEASE_SHA256, "FFmpeg archive")?;

    let bin_dir = stem_tools_ffmpeg_bin_dir()
        .ok_or_else(|| "Could not resolve the EsanOne Converter tools folder.".to_string())?;
    emit_log(
        app,
        STEM_TOOL_INSTALL_EVENT,
        session_id,
        "status",
        "Installing FFmpeg…",
    );
    extract_ffmpeg_bin_from_zip(&archive, &bin_dir)?;
    ensure_stem_env();
    if probe_ffmpeg() {
        Ok(())
    } else {
        Err(
            "FFmpeg was unpacked but could not run. Install FFmpeg manually, then try again."
                .into(),
        )
    }
}

#[cfg(windows)]
fn install_ffmpeg_windows(
    app: &AppHandle,
    state: &StemSeparateState,
    session_id: &str,
) -> Result<(), String> {
    ensure_stem_env();
    if probe_ffmpeg() {
        emit_log(
            app,
            STEM_TOOL_INSTALL_EVENT,
            session_id,
            "status",
            "ffmpeg already available",
        );
        return Ok(());
    }
    if let Some(winget) = resolve_program("winget") {
        emit_log(
            app,
            STEM_TOOL_INSTALL_EVENT,
            session_id,
            "status",
            "Installing FFmpeg (winget Gyan.FFmpeg)…",
        );
        let mut command = Command::new(&winget);
        command
            .args([
                "install",
                "-e",
                "--id",
                "Gyan.FFmpeg",
                "--accept-package-agreements",
                "--accept-source-agreements",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                emit_log(
                    app,
                    STEM_TOOL_INSTALL_EVENT,
                    session_id,
                    "status",
                    format!("winget could not start ({error}); using direct FFmpeg download…"),
                );
                return install_ffmpeg_direct(app, session_id);
            }
        };
        let code = run_tracked_child(state, session_id, child, app, STEM_TOOL_INSTALL_EVENT)?;
        ensure_stem_env();
        if probe_ffmpeg() {
            return Ok(());
        }
        emit_log(
            app,
            STEM_TOOL_INSTALL_EVENT,
            session_id,
            "status",
            format!("winget finished (code {code}); using direct FFmpeg download…"),
        );
    } else {
        emit_log(
            app,
            STEM_TOOL_INSTALL_EVENT,
            session_id,
            "status",
            "winget was not found; using direct FFmpeg download…",
        );
    }

    install_ffmpeg_direct(app, session_id)
}

fn install_melbandroformer_deps(
    app: &AppHandle,
    state: &StemSeparateState,
    session_id: &str,
    python: &Path,
) -> Result<(), String> {
    emit_log(
        app,
        STEM_TOOL_INSTALL_EVENT,
        session_id,
        "status",
        "Installing MelBandRoFormer dependencies (pip)…",
    );
    let mut command = Command::new(python);
    command
        .args([
            "-m",
            "pip",
            "install",
            "-U",
            "safetensors",
            "einops",
            "rotary-embedding-torch",
            "librosa",
            "soundfile",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let child = command
        .spawn()
        .map_err(|error| format!("Failed to start pip: {error}"))?;
    let code = run_tracked_child(state, session_id, child, app, STEM_TOOL_INSTALL_EVENT)?;
    if code != 0 && !probe_melbandroformer_deps(python) {
        return Err(format!(
            "pip install MelBandRoFormer dependencies failed (exit {code})"
        ));
    }
    if !probe_melbandroformer_deps(python) {
        return Err(
            "MelBandRoFormer dependencies installed but Python cannot import them. Try the install again."
                .into(),
        );
    }
    Ok(())
}

/// Downloads and verifies the pinned FP16 weights, writing to a `.part` file
/// first so an interrupted download never leaves a file at the final,
/// ready-marker path (the next install retries cleanly).
fn download_melbandroformer_weights(
    app: &AppHandle,
    session_id: &str,
    python: &Path,
) -> Result<(), String> {
    let final_path = melbandroformer_weights_path().ok_or_else(|| {
        "Could not resolve the local model folder for EsanOne Converter.".to_string()
    })?;
    if final_path.is_file() {
        return Ok(());
    }
    let dir = final_path
        .parent()
        .ok_or_else(|| "Invalid MelBandRoFormer model path".to_string())?;
    std::fs::create_dir_all(dir).map_err(|error| format!("Cannot create model folder: {error}"))?;

    emit_log(
        app,
        STEM_TOOL_INSTALL_EVENT,
        session_id,
        "status",
        format!(
            "Downloading MelBandRoFormer vocal model (~{} MB)…",
            MELBANDROFORMER_WEIGHTS_BYTES / (1024 * 1024)
        ),
    );
    let bytes = download_bytes(
        &melbandroformer_weights_url(),
        "MelBandRoFormer weights",
        MAX_MELBANDROFORMER_WEIGHTS_BYTES,
    )?;
    verify_download_sha256(
        &bytes,
        MELBANDROFORMER_WEIGHTS_SHA256,
        "MelBandRoFormer weights",
    )?;

    let tmp_path = dir.join(format!("{MELBANDROFORMER_WEIGHTS_FILENAME}.part"));
    {
        let mut file = std::fs::File::create(&tmp_path)
            .map_err(|error| format!("Cannot write model weights: {error}"))?;
        file.write_all(&bytes)
            .map_err(|error| format!("Cannot write model weights: {error}"))?;
    }

    emit_log(
        app,
        STEM_TOOL_INSTALL_EVENT,
        session_id,
        "status",
        "Verifying MelBandRoFormer weights load…",
    );
    let load_script = format!(
        "from safetensors.torch import load_file; load_file(r'{}')",
        tmp_path.to_string_lossy()
    );
    match run_capture(python, &["-c", &load_script]) {
        Ok((0, _, _)) => {}
        Ok((_, _, stderr)) => {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(format!("Downloaded weights failed to load: {stderr}"));
        }
        Err(error) => {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(format!("Could not verify downloaded weights: {error}"));
        }
    }

    std::fs::rename(&tmp_path, &final_path)
        .map_err(|error| format!("Cannot finalize model weights: {error}"))?;
    Ok(())
}

/// Install Python (if needed) then either `pip install -U demucs` or the
/// MelBandRoFormer vocal dependencies + weights, depending on `purpose`.
/// Runs on a worker thread; progress via `stem-tool-install-event` /
/// `stem-tool-install-done`.
#[tauri::command]
pub fn stem_tool_install(
    app: AppHandle,
    state: State<'_, StemSeparateState>,
    session_id: String,
    purpose: String,
) -> Result<(), String> {
    stem_install_backend(&purpose)?;
    {
        let mut slot = state.install_session.lock().unwrap();
        if slot.is_some() {
            return Err("Stem tool install is already running.".into());
        }
        *slot = Some(session_id.clone());
    }

    let state = state.inner().clone();
    std::thread::spawn(move || {
        let finish = |ok: bool, error: Option<String>| {
            *state.install_session.lock().unwrap() = None;
            invalidate_status_cache(&state);
            let _ = app.emit(
                STEM_TOOL_INSTALL_DONE,
                InstallDoneEvent {
                    session_id: session_id.clone(),
                    ok,
                    error,
                    status: current_status(&state, true),
                },
            );
        };

        let result = (|| {
            ensure_stem_env();
            emit_progress(
                &app,
                STEM_TOOL_INSTALL_EVENT,
                &session_id,
                "status",
                0.05,
                "Checking Python and the managed environment…",
            );
            let mut python = probe_system_python();
            if python.is_none() {
                install_python_windows(&app, &state, &session_id)?;
                ensure_stem_env();
                python = probe_system_python();
            }
            let (bootstrap_python, version) = python.ok_or_else(|| {
                "Python 3.10+ was not found after install. Try again or install Python manually."
                    .to_string()
            })?;
            let (python_path, managed_version) =
                ensure_managed_python(&app, &state, &session_id, &bootstrap_python)?;
            emit_progress(
                &app,
                STEM_TOOL_INSTALL_EVENT,
                &session_id,
                "status",
                0.25,
                format!("Using app-owned Python {managed_version} (bootstrap {version})"),
            );

            if purpose == "vocals" {
                // Both backends share this Python env: only add torch if it
                // isn't already importable (e.g. Instruments installed first).
                emit_progress(
                    &app,
                    STEM_TOOL_INSTALL_EVENT,
                    &session_id,
                    "status",
                    0.35,
                    "Installing separation dependencies…",
                );
                install_melbandroformer_deps(&app, &state, &session_id, &python_path)?;
                emit_progress(
                    &app,
                    STEM_TOOL_INSTALL_EVENT,
                    &session_id,
                    "status",
                    0.52,
                    "Checking the CPU/GPU PyTorch runtime…",
                );
                if !probe_stem_gpu(Some(&python_path)).torch_ok {
                    install_torch_pair(
                        &app,
                        &state,
                        &session_id,
                        &python_path,
                        "cpu",
                        STEM_TOOL_INSTALL_EVENT,
                    )?;
                }
                install_ffmpeg_windows(&app, &state, &session_id)?;
                emit_progress(
                    &app,
                    STEM_TOOL_INSTALL_EVENT,
                    &session_id,
                    "status",
                    0.78,
                    "FFmpeg is ready; downloading the vocal model…",
                );
                download_melbandroformer_weights(&app, &session_id, &python_path)?;
                emit_progress(
                    &app,
                    STEM_TOOL_INSTALL_EVENT,
                    &session_id,
                    "status",
                    0.95,
                    "Verifying the vocal separation backend…",
                );
                if !current_status(&state, true).mel_band_ro_former {
                    return Err(
                        "Install finished but the vocal model is not ready (dependencies, ffmpeg, or weights missing). Recheck and try again."
                            .into(),
                    );
                }
            } else {
                // Always (re)install demucs + soundfile, then ensure ffmpeg for decoding.
                emit_progress(
                    &app,
                    STEM_TOOL_INSTALL_EVENT,
                    &session_id,
                    "status",
                    0.38,
                    "Installing Demucs and audio dependencies…",
                );
                install_demucs(&app, &state, &session_id, &python_path)?;
                emit_progress(
                    &app,
                    STEM_TOOL_INSTALL_EVENT,
                    &session_id,
                    "status",
                    0.78,
                    "Checking FFmpeg and the Demucs runtime…",
                );
                install_ffmpeg_windows(&app, &state, &session_id)?;
                if !current_status(&state, true).demucs {
                    return Err(
                        "Install finished but stem tools are not ready (need demucs + ffmpeg). Recheck the setup and try again."
                            .into(),
                    );
                }
            }
            emit_progress(
                &app,
                STEM_TOOL_INSTALL_EVENT,
                &session_id,
                "status",
                1.0,
                "Stem tools ready",
            );
            Ok(())
        })();

        match result {
            Ok(()) => finish(true, None),
            Err(error) => {
                emit_log(&app, STEM_TOOL_INSTALL_EVENT, &session_id, "error", &error);
                finish(false, Some(error));
            }
        }
    });
    Ok(())
}

fn install_torch_pair(
    app: &AppHandle,
    state: &StemSeparateState,
    session_id: &str,
    python: &Path,
    target: &str,
    event_name: &'static str,
) -> Result<(), String> {
    let args = torch_pair_args(target)?;
    let label = if target == "cuda" { "CUDA" } else { "CPU" };
    emit_log(
        app,
        event_name,
        session_id,
        "status",
        format!(
            "Installing torch=={PYTORCH_TORCH_VERSION} + torchaudio=={PYTORCH_TORCHAUDIO_VERSION} ({label})…"
        ),
    );
    let mut command = Command::new(python);
    command
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let child = command
        .spawn()
        .map_err(|error| format!("Failed to start pip: {error}"))?;
    let code = run_tracked_child(state, session_id, child, app, event_name)?;
    if code != 0 {
        return Err(format!("{label} PyTorch install failed (exit {code})"));
    }
    Ok(())
}

/// Install or repair the pinned PyTorch wheel pair (`target`: `"cuda"` | `"cpu"`).
#[tauri::command]
pub fn stem_gpu_install(
    app: AppHandle,
    state: State<'_, StemSeparateState>,
    session_id: String,
    target: String,
) -> Result<(), String> {
    if target != "cuda" && target != "cpu" {
        return Err(format!("Unknown torch install target: {target}"));
    }
    {
        let mut slot = state.install_session.lock().unwrap();
        if slot.is_some() {
            return Err("Another install is already running.".into());
        }
        *slot = Some(session_id.clone());
    }

    let state = state.inner().clone();
    std::thread::spawn(move || {
        let finish = |ok: bool, error: Option<String>| {
            *state.install_session.lock().unwrap() = None;
            invalidate_status_cache(&state);
            let _ = app.emit(
                STEM_GPU_INSTALL_DONE,
                InstallDoneEvent {
                    session_id: session_id.clone(),
                    ok,
                    error,
                    status: current_status(&state, true),
                },
            );
        };

        let result = (|| {
            ensure_stem_env();
            emit_progress(
                &app,
                STEM_GPU_INSTALL_EVENT,
                &session_id,
                "status",
                0.05,
                "Checking GPU support…",
            );
            let (python, version) = managed_python().ok_or_else(|| {
                "Python 3.10+ is required. Install stem tools first, then enable GPU acceleration."
                    .to_string()
            })?;
            emit_progress(
                &app,
                STEM_GPU_INSTALL_EVENT,
                &session_id,
                "status",
                0.20,
                format!("Using {version}"),
            );

            emit_progress(
                &app,
                STEM_GPU_INSTALL_EVENT,
                &session_id,
                "status",
                0.28,
                "Installing the CUDA-enabled PyTorch packages…",
            );
            let install_result = install_torch_pair(
                &app,
                &state,
                &session_id,
                &python,
                &target,
                STEM_GPU_INSTALL_EVENT,
            );

            if target == "cuda" {
                // One roll-back path for every wrecked outcome (pip failure,
                // user cancel mid-uninstall, or a "successful" install that
                // left torch unimportable). Skipped while torch still imports
                // — a CUDA-build torch without a driver still runs CPU ops.
                let restore_cpu = |message: String| -> String {
                    let cancelled = message.contains("Cancelled");
                    emit_log(
                        &app,
                        STEM_GPU_INSTALL_EVENT,
                        &session_id,
                        "status",
                        if cancelled {
                            "Install cancelled mid-way and torch is unusable; restoring the CPU PyTorch runtime…"
                        } else {
                            "CUDA install failed; restoring the CPU PyTorch runtime…"
                        },
                    );
                    emit_progress(
                        &app,
                        STEM_GPU_INSTALL_EVENT,
                        &session_id,
                        "status",
                        0.75,
                        "Restoring the CPU PyTorch runtime…",
                    );
                    match install_torch_pair(
                        &app,
                        &state,
                        &session_id,
                        &python,
                        "cpu",
                        STEM_GPU_INSTALL_EVENT,
                    ) {
                        Ok(()) if probe_stem_gpu(Some(&python)).torch_ok => {
                            format!("{message} CPU runtime restored.")
                        }
                        _ => format!(
                            "{message} CPU restore also failed — click Repair PyTorch in System Setup."
                        ),
                    }
                };

                if let Err(cuda_error) = install_result {
                    let gpu = probe_stem_gpu(Some(&python));
                    if !gpu.torch_ok {
                        return Err(restore_cpu(cuda_error));
                    }
                    return Err(cuda_error);
                }
                let gpu = probe_stem_gpu(Some(&python));
                if !gpu.torch_ok {
                    return Err(restore_cpu(
                        "CUDA PyTorch install left torch unimportable.".to_string(),
                    ));
                }
                emit_progress(
                    &app,
                    STEM_GPU_INSTALL_EVENT,
                    &session_id,
                    "status",
                    0.90,
                    "Testing CUDA and the selected device…",
                );
                if !gpu.cuda_available {
                    // CUDA-build torch without a working driver still runs CPU ops — not a rollback.
                    return Err(
                        "CUDA PyTorch installed but torch.cuda.is_available() is false. Check the NVIDIA driver, then Recheck."
                            .into(),
                    );
                }
                emit_progress(
                    &app,
                    STEM_GPU_INSTALL_EVENT,
                    &session_id,
                    "status",
                    1.0,
                    format!(
                        "CUDA ready: {} — separation will be faster than CPU",
                        gpu.device_name.as_deref().unwrap_or("NVIDIA GPU")
                    ),
                );
                return Ok(());
            }

            // CPU repair target
            install_result?;
            let gpu = probe_stem_gpu(Some(&python));
            if !gpu.torch_ok {
                return Err(
                    "CPU PyTorch install finished but torch still cannot be imported.".into(),
                );
            }
            emit_progress(
                &app,
                STEM_GPU_INSTALL_EVENT,
                &session_id,
                "status",
                0.95,
                "CPU PyTorch runtime ready",
            );
            Ok(())
        })();

        match result {
            Ok(()) => finish(true, None),
            Err(error) => {
                emit_log(&app, STEM_GPU_INSTALL_EVENT, &session_id, "error", &error);
                finish(false, Some(error));
            }
        }
    });
    Ok(())
}

fn audio_stem_extensions(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("wav") | Some("mp3") | Some("flac") | Some("ogg") | Some("m4a")
    )
}

/// Demucs writes `{out}/{model}/{input_stem}/*.wav`. Collect every audio file.
pub fn collect_stem_paths(output_dir: &Path, model: &str, input_path: &Path) -> Vec<String> {
    let stem_name = input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("track");
    let track_dir = output_dir.join(model).join(stem_name);
    let mut paths = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&track_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && audio_stem_extensions(&path) {
                paths.push(path.to_string_lossy().to_string());
            }
        }
    }
    paths.sort();
    paths
}

/// Allocate a descriptive output name without overwriting an existing file.
pub fn prefixed_output_path(output_dir: &Path, prefix: &str, source_name: &str) -> PathBuf {
    let source = Path::new(source_name);
    let stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("track");
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{value}"))
        .unwrap_or_default();
    let base = format!("{prefix} {stem}");
    let mut candidate = output_dir.join(format!("{base}{extension}"));
    let mut suffix = 1;
    while candidate.exists() {
        candidate = output_dir.join(format!("{base} ({suffix}){extension}"));
        suffix += 1;
    }
    candidate
}

pub fn stem_output_prefix(_model: &str, stem_name: &str) -> Option<&'static str> {
    match stem_name.to_ascii_lowercase().as_str() {
        "vocal" | "vocals" => Some("[Vocal]"),
        "drum" | "drums" => Some("[Drum]"),
        "bass" => Some("[Bass]"),
        "other" => Some("[Other]"),
        "guitar" => Some("[Guitar]"),
        "piano" => Some("[Piano]"),
        "inst" | "instrumental" | "instrumentals" => Some("[INST]"),
        _ => None,
    }
}

pub fn flatten_stem_paths(
    output_dir: &Path,
    model: &str,
    input_path: &Path,
) -> Result<Vec<String>, String> {
    let source_name = input_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("track.wav");
    let nested_paths = collect_stem_paths(output_dir, model, input_path);
    let mut final_paths = Vec::with_capacity(nested_paths.len());
    for nested in nested_paths {
        let source = PathBuf::from(&nested);
        let stem_name = source
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("other");
        let prefix = stem_output_prefix(model, stem_name)
            .ok_or_else(|| format!("Unsupported output stem: {stem_name}"))?;
        let destination = prefixed_output_path(output_dir, prefix, source_name);
        std::fs::rename(&source, &destination)
            .map_err(|error| format!("Cannot move stem output: {error}"))?;
        final_paths.push(destination.to_string_lossy().to_string());
    }

    let stem_name = input_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("track");
    let track_dir = output_dir.join(model).join(stem_name);
    let _ = std::fs::remove_dir(&track_dir);
    let _ = std::fs::remove_dir(output_dir.join(model));
    final_paths.sort();
    Ok(final_paths)
}

#[tauri::command]
pub fn stem_separate_start(
    app: AppHandle,
    state: State<'_, StemSeparateState>,
    input_path: String,
    output_dir: String,
    session_id: String,
    model: String,
    device_preference: Option<String>,
    flatten_output: Option<bool>,
) -> Result<(), String> {
    validate_stem_model(&model)?;
    ensure_stem_env();
    let force_cpu = device_preference.as_deref() == Some("cpu");
    let flatten_output = flatten_output.unwrap_or(false);
    let (python, _) = managed_python()
        .ok_or_else(|| "Python 3.10+ not found. Use Install stem tools first.".to_string())?;
    if !probe_ffmpeg() {
        return Err(
            "ffmpeg is missing (needed to load audio). Open Separate stems and click Install stem tools again."
                .into(),
        );
    }

    // The vocal backend does not use Demucs at all: it must not be gated on
    // a Demucs install, and it needs its own weights file instead.
    let weights_path_owned = if is_melbandroformer(&model) {
        if !probe_melbandroformer_deps(&python) {
            return Err(
                "MelBandRoFormer dependencies are missing. Open Separate stems and click Install (Vocals)."
                    .into(),
            );
        }
        let weights = melbandroformer_weights_path().filter(|p| p.is_file()).ok_or_else(|| {
            "MelBandRoFormer weights are missing. Open Separate stems and click Install (Vocals)."
                .to_string()
        })?;
        weights.to_string_lossy().to_string()
    } else {
        if !probe_demucs_module(&python) {
            return Err(
                "demucs is not installed. Open Separate stems and click Install stem tools.".into(),
            );
        }
        String::new()
    };

    std::fs::create_dir_all(&output_dir)
        .map_err(|error| format!("Cannot create output dir: {error}"))?;

    let wrapper = if is_melbandroformer(&model) {
        write_melbandroformer_wrapper()?
    } else {
        write_demucs_wrapper()?
    };
    let state_owned = state.inner().clone();
    // Clear stale cancel once per session before any spawn.
    state_owned.cancelled.lock().unwrap().remove(&session_id);

    emit_log(
        &app,
        STEM_SEPARATE_EVENT,
        &session_id,
        "status",
        if is_melbandroformer(&model) {
            "Running MelBandRoFormer vocal separation…".to_string()
        } else {
            format!(
                "Running demucs ({model})… first song may download model weights and take a while"
            )
        },
    );

    let out_dir = output_dir.clone();
    let in_path = input_path.clone();
    let model_owned = model.clone();
    let session_for_done = session_id.clone();
    let wrapper_s = wrapper.to_string_lossy().to_string();
    let weights_s = weights_path_owned;
    let python_path = python.clone();

    std::thread::spawn(move || {
        // Probe here, not in the command body: a cold status cache imports
        // torch (seconds) and sync commands run on the main thread.
        let status = current_status(&state_owned, false);
        let device_name = status
            .gpu
            .device_name
            .clone()
            .unwrap_or_else(|| "NVIDIA GPU".into());
        let segment_seconds = cuda_retry_segment_seconds(status.gpu.memory_mb);
        let mut attempt = initial_stem_attempt(&status.gpu, force_cpu);
        let mut retried_after_oom = false;

        let emit_done =
            |ok: bool, error: Option<String>, exit_code: Option<i32>, stem_paths: Vec<String>| {
                let _ = app.emit(
                    STEM_SEPARATE_DONE,
                    SeparateDoneEvent {
                        session_id: session_for_done.clone(),
                        ok,
                        error,
                        exit_code,
                        stem_paths,
                    },
                );
            };

        let format_fatal = |exit_code: Option<i32>, stderr_lines: &[String]| -> String {
            let fatal = stderr_lines
                .iter()
                .rev()
                .find(|line| {
                    let lower = line.to_ascii_lowercase();
                    lower.contains("fatal")
                        || lower.contains("importerror")
                        || lower.contains("modulenotfounderror")
                        || lower.contains("error:")
                        || lower.contains("exception")
                })
                .cloned();
            match fatal {
                Some(detail) => format!(
                    "separation exited with code {}: {detail}",
                    exit_code.unwrap_or(-1)
                ),
                None => format!(
                    "separation exited with code {}",
                    exit_code
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "unknown".into())
                ),
            }
        };

        loop {
            match attempt {
                StemAttempt::CudaFull if !force_cpu => {
                    emit_log(
                        &app,
                        STEM_SEPARATE_EVENT,
                        &session_id,
                        "status",
                        format!("Using GPU: {device_name}"),
                    );
                }
                StemAttempt::CudaSegmented if !force_cpu => {
                    let msg = if retried_after_oom {
                        format!(
                            "GPU memory exhausted; retrying with {segment_seconds} second segments"
                        )
                    } else {
                        format!("Using GPU: {device_name} ({segment_seconds}s segments)")
                    };
                    emit_log(&app, STEM_SEPARATE_EVENT, &session_id, "status", msg);
                }
                StemAttempt::Cpu if status.gpu.cuda_available && !force_cpu => {
                    emit_log(
                        &app,
                        STEM_SEPARATE_EVENT,
                        &session_id,
                        "status",
                        "GPU memory remains insufficient; continuing on CPU",
                    );
                }
                _ => {}
            }

            let args = if is_melbandroformer(&model_owned) {
                build_melbandroformer_args(
                    &wrapper_s,
                    &weights_s,
                    &out_dir,
                    &in_path,
                    attempt,
                    segment_seconds,
                )
            } else {
                build_demucs_args(
                    &wrapper_s,
                    &model_owned,
                    &out_dir,
                    &in_path,
                    attempt,
                    segment_seconds,
                )
            };
            let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            let child = match spawn_logged(&python_path, &arg_refs) {
                Ok(child) => child,
                Err(error) => {
                    emit_log(&app, STEM_SEPARATE_EVENT, &session_id, "error", &error);
                    emit_done(false, Some(error), None, Vec::new());
                    return;
                }
            };

            let (tx, rx) = std::sync::mpsc::channel();
            stream_child_logs(
                app.clone(),
                STEM_SEPARATE_EVENT,
                session_id.clone(),
                child,
                state_owned.clone(),
                move |exit_code, stderr_lines| {
                    let _ = tx.send((exit_code, stderr_lines));
                },
            );

            let Ok((exit_code, stderr_lines)) = rx.recv() else {
                emit_done(
                    false,
                    Some("Separation process ended unexpectedly.".into()),
                    None,
                    Vec::new(),
                );
                return;
            };

            let cancel_pending = state_owned.cancelled.lock().unwrap().remove(&session_id);
            let outcome = if exit_code.is_none() {
                AttemptOutcome::Cancelled
            } else if exit_code == Some(0) {
                AttemptOutcome::Success
            } else {
                let oom = matches!(attempt, StemAttempt::CudaFull | StemAttempt::CudaSegmented)
                    && is_cuda_out_of_memory(&stderr_lines);
                AttemptOutcome::Failed { oom }
            };

            match after_stem_attempt(attempt, outcome, cancel_pending) {
                StemNextAction::Retry(next) => {
                    retried_after_oom = true;
                    attempt = next;
                    continue;
                }
                StemNextAction::FinishCancelled => {
                    emit_done(false, Some("Cancelled".into()), None, Vec::new());
                    return;
                }
                StemNextAction::FinishSuccess => {
                    let paths = if flatten_output {
                        flatten_stem_paths(Path::new(&out_dir), &model_owned, Path::new(&in_path))
                    } else {
                        Ok(collect_stem_paths(
                            Path::new(&out_dir),
                            &model_owned,
                            Path::new(&in_path),
                        ))
                    };
                    match paths {
                        Ok(stem_paths) if stem_paths.is_empty() => emit_done(
                            false,
                            Some("Separation finished but no stem files were found.".into()),
                            exit_code,
                            Vec::new(),
                        ),
                        Ok(stem_paths) => emit_done(true, None, exit_code, stem_paths),
                        Err(error) => emit_done(false, Some(error), exit_code, Vec::new()),
                    }
                    return;
                }
                StemNextAction::FinishFailed => {
                    emit_done(
                        false,
                        Some(format_fatal(exit_code, &stderr_lines)),
                        exit_code,
                        Vec::new(),
                    );
                    return;
                }
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub fn stem_separate_stop(state: State<'_, StemSeparateState>, session_id: String) {
    state.cancelled.lock().unwrap().insert(session_id.clone());
    if let Some(child) = state.children.lock().unwrap().get_mut(&session_id) {
        let _ = child.kill();
    }
}

/// Public helpers for sibling modules (youtube).
pub fn stem_app_data_dir_pub() -> Option<PathBuf> {
    stem_app_data_dir()
}

pub fn download_bytes_pub(url: &str, artifact: &str, limit: u64) -> Result<Vec<u8>, String> {
    download_bytes(url, artifact, limit)
}

pub fn verify_download_sha256_pub(
    bytes: &[u8],
    expected: &str,
    artifact: &str,
) -> Result<(), String> {
    verify_download_sha256(bytes, expected, artifact)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;

    #[test]
    fn progress_value_is_clamped() {
        assert_eq!(clamp_progress(-0.2), 0.0);
        assert_eq!(clamp_progress(0.45), 0.45);
        assert_eq!(clamp_progress(1.4), 1.0);
    }

    #[test]
    fn collect_stem_paths_finds_wavs() {
        let root = std::env::temp_dir().join(format!(
            "esan-stem-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        ));
        let track_dir = root.join("htdemucs_ft").join("song");
        fs::create_dir_all(&track_dir).unwrap();
        fs::write(track_dir.join("vocals.wav"), b"x").unwrap();
        fs::write(track_dir.join("drums.wav"), b"x").unwrap();
        fs::write(track_dir.join("readme.txt"), b"x").unwrap();

        let paths = collect_stem_paths(&root, "htdemucs_ft", Path::new("/tmp/song.mp3"));
        assert_eq!(paths.len(), 2);
        assert!(paths.iter().any(|p| p.ends_with("drums.wav")));
        assert!(paths.iter().any(|p| p.ends_with("vocals.wav")));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn python_version_requires_3_10() {
        assert!(python_version_ok("Python 3.10.0"));
        assert!(python_version_ok("Python 3.12.10"));
        assert!(!python_version_ok("Python 3.9.18"));
        assert!(!python_version_ok("Python 2.7.18"));
    }

    #[test]
    fn model_validation_rejects_unknown_ids() {
        assert!(validate_stem_model("melbandroformer_vocals").is_ok());
        assert!(validate_stem_model("htdemucs_ft").is_ok());
        assert!(validate_stem_model("htdemucs_6s").is_ok());
        assert!(validate_stem_model("htdemucs").is_err());
    }

    #[test]
    fn managed_python_lives_inside_stem_tools() {
        let root = Path::new(r"C:\Users\tester\EsanOne Stem\stem-tools");
        let path = managed_python_path(root);
        assert!(path.starts_with(root));
        assert!(path.to_string_lossy().contains("python"));
        assert!(path.to_string_lossy().contains("venv"));
    }

    #[test]
    fn downloaded_artifact_sha256_accepts_sha256sum_format() {
        let expected =
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  artifact.zip";

        assert!(verify_download_sha256(b"abc", expected, "artifact").is_ok());
        assert!(verify_download_sha256(b"abc", "00", "artifact").is_err());
    }

    #[test]
    fn ffmpeg_fallback_uses_a_pinned_release_checksum() {
        assert_eq!(
            FFMPEG_RELEASE_ZIP_URL,
            "https://www.gyan.dev/ffmpeg/builds/packages/ffmpeg-8.1.2-essentials_build.zip"
        );
        assert_eq!(
            FFMPEG_RELEASE_SHA256,
            "db580001caa24ac104c8cb856cd113a87b0a443f7bdf47d8c12b1d740584a2ec"
        );
    }

    #[test]
    fn download_size_limit_rejects_oversized_artifacts() {
        assert!(ensure_size_within_limit(8, 8, "artifact").is_ok());
        assert!(ensure_size_within_limit(9, 8, "artifact").is_err());
    }

    #[test]
    fn extracts_only_ffmpeg_bin_files_from_release_archive() {
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        archive
            .start_file(
                "ffmpeg-8.0-essentials_build/bin/ffmpeg.exe",
                SimpleFileOptions::default(),
            )
            .unwrap();
        archive.write_all(b"ffmpeg").unwrap();
        archive
            .start_file(
                "ffmpeg-8.0-essentials_build/bin/ffprobe.exe",
                SimpleFileOptions::default(),
            )
            .unwrap();
        archive.write_all(b"ffprobe").unwrap();
        archive
            .start_file(
                "ffmpeg-8.0-essentials_build/doc/readme.txt",
                SimpleFileOptions::default(),
            )
            .unwrap();
        archive.write_all(b"not a binary").unwrap();
        let bytes = archive.finish().unwrap().into_inner();

        let install_dir = tempfile::tempdir().unwrap();
        extract_ffmpeg_bin_from_zip(&bytes, install_dir.path()).unwrap();

        assert_eq!(
            fs::read(install_dir.path().join("ffmpeg.exe")).unwrap(),
            b"ffmpeg"
        );
        assert_eq!(
            fs::read(install_dir.path().join("ffprobe.exe")).unwrap(),
            b"ffprobe"
        );
        assert!(!install_dir.path().join("readme.txt").exists());
    }

    #[test]
    fn cuda_probe_parses_a_ready_runtime() {
        let status = parse_stem_gpu_probe(
            r#"{"torch_ok":true,"cuda_available":true,"torch_version":"2.7.1+cu128","device_name":"RTX 3050 Ti","memory_mb":4096}"#,
        );
        assert!(status.torch_ok);
        assert!(status.cuda_available);
        assert_eq!(status.device_name.as_deref(), Some("RTX 3050 Ti"));
        assert_eq!(status.memory_mb, Some(4096));
    }

    #[test]
    fn invalid_cuda_probe_is_a_safe_cpu_fallback() {
        assert!(!parse_stem_gpu_probe("not-json").cuda_available);
        assert!(!parse_stem_gpu_probe("not-json").torch_ok);
    }

    #[test]
    fn probe_distinguishes_broken_torch_from_missing_cuda() {
        let broken = parse_stem_gpu_probe(
            r#"{"torch_ok":false,"torch_installed":true,"cuda_available":false,"error":"DLL load failed"}"#,
        );
        assert!(!broken.torch_ok && broken.torch_installed && !broken.cuda_available);

        let absent = parse_stem_gpu_probe(
            r#"{"torch_ok":false,"torch_installed":false,"cuda_available":false,"error":"No module named 'torch'"}"#,
        );
        assert!(!absent.torch_ok && !absent.torch_installed);

        let cpu_torch = parse_stem_gpu_probe(
            r#"{"torch_ok":true,"cuda_available":false,"torch_version":"2.7.1"}"#,
        );
        // Missing torch_installed falls back to torch_ok (older payloads).
        assert!(cpu_torch.torch_ok && cpu_torch.torch_installed && !cpu_torch.cuda_available);

        assert!(!parse_stem_gpu_probe("not-json").torch_ok);
        assert!(!parse_stem_gpu_probe("not-json").torch_installed);
    }

    #[test]
    fn cancel_pending_after_oom_never_retries() {
        assert_eq!(
            after_stem_attempt(
                StemAttempt::CudaFull,
                AttemptOutcome::Failed { oom: true },
                true
            ),
            StemNextAction::FinishCancelled
        );
        assert_eq!(
            after_stem_attempt(
                StemAttempt::CudaFull,
                AttemptOutcome::Failed { oom: true },
                false
            ),
            StemNextAction::Retry(StemAttempt::CudaSegmented)
        );
        assert_eq!(
            after_stem_attempt(StemAttempt::Cpu, AttemptOutcome::Success, true),
            StemNextAction::FinishCancelled
        );
        assert_eq!(
            after_stem_attempt(StemAttempt::CudaSegmented, AttemptOutcome::Cancelled, false),
            StemNextAction::FinishCancelled
        );
    }

    #[test]
    fn retry_policy_preserves_quality_then_falls_back_after_two_cuda_ooms() {
        let ready = StemGpuStatus {
            torch_ok: true,
            cuda_available: true,
            memory_mb: Some(8192),
            ..Default::default()
        };
        assert_eq!(initial_stem_attempt(&ready, false), StemAttempt::CudaFull);
        assert_eq!(
            next_stem_attempt(StemAttempt::CudaFull, true),
            Some(StemAttempt::CudaSegmented)
        );
        assert_eq!(
            next_stem_attempt(StemAttempt::CudaSegmented, true),
            Some(StemAttempt::Cpu)
        );
        assert_eq!(next_stem_attempt(StemAttempt::Cpu, true), None);
    }

    #[test]
    fn initial_attempt_follows_vram_and_preference() {
        let gpu = |mem: Option<u64>| StemGpuStatus {
            torch_ok: true,
            cuda_available: true,
            memory_mb: mem,
            ..Default::default()
        };
        assert_eq!(
            initial_stem_attempt(&gpu(Some(8192)), false),
            StemAttempt::CudaFull
        );
        assert_eq!(
            initial_stem_attempt(&gpu(Some(2048)), false),
            StemAttempt::CudaSegmented
        );
        assert_eq!(
            initial_stem_attempt(&gpu(None), false),
            StemAttempt::CudaFull
        );
        assert_eq!(
            initial_stem_attempt(&gpu(Some(8192)), true),
            StemAttempt::Cpu
        );
        assert_eq!(
            initial_stem_attempt(&StemGpuStatus::default(), false),
            StemAttempt::Cpu
        );
    }

    #[test]
    fn retry_segment_scales_with_vram() {
        assert_eq!(cuda_retry_segment_seconds(Some(4096)), "4");
        assert_eq!(cuda_retry_segment_seconds(Some(8192)), "6");
        assert_eq!(cuda_retry_segment_seconds(None), "4");
    }

    #[test]
    fn oom_markers_cover_cublas() {
        assert!(is_cuda_out_of_memory(&[
            "CUBLAS_STATUS_ALLOC_FAILED when calling cublasCreate".into()
        ]));
    }

    #[test]
    fn torch_pair_args_pin_versions_and_cuda_index() {
        let cuda = torch_pair_args("cuda").unwrap();
        assert!(cuda.iter().any(|a| a == "torch==2.7.1"));
        assert!(cuda.iter().any(|a| a == "torchaudio==2.7.1"));
        assert!(cuda.iter().any(|a| a.contains("cu128")));
        assert!(!cuda.iter().any(|a| a.contains("torchvision")));

        let cpu = torch_pair_args("cpu").unwrap();
        assert!(cpu.iter().any(|a| a == "torch==2.7.1"));
        assert!(cpu.iter().any(|a| a == "torchaudio==2.7.1"));
        assert!(!cpu.iter().any(|a| a.contains("download.pytorch.org")));
        assert!(!cpu.iter().any(|a| a.contains("torchvision")));
    }

    #[test]
    fn demucs_args_pin_the_selected_device_and_segment_only_when_requested() {
        assert!(build_demucs_args(
            "wrapper.py",
            "htdemucs_ft",
            "out",
            "song.wav",
            StemAttempt::CudaFull,
            "4",
        )
        .windows(2)
        .any(|pair| pair == ["-d", "cuda"]));
        assert!(build_demucs_args(
            "wrapper.py",
            "htdemucs_ft",
            "out",
            "song.wav",
            StemAttempt::CudaSegmented,
            "4",
        )
        .windows(2)
        .any(|pair| pair == ["--segment", "4"]));
        assert!(build_demucs_args(
            "wrapper.py",
            "htdemucs_ft",
            "out",
            "song.wav",
            StemAttempt::Cpu,
            "4",
        )
        .windows(2)
        .any(|pair| pair == ["-d", "cpu"]));
    }

    #[test]
    fn melbandroformer_args_pin_device_weight_and_chunk() {
        let full = build_melbandroformer_args(
            "wrapper.py",
            "model.safetensors",
            "out",
            "song.wav",
            StemAttempt::CudaFull,
            "10",
        );
        assert!(full.windows(2).any(|pair| pair == ["--device", "cuda"]));
        assert!(full
            .windows(2)
            .any(|pair| pair == ["--weights", "model.safetensors"]));
        assert!(!full.iter().any(|a| a == "--chunk-seconds"));

        let seg = build_melbandroformer_args(
            "wrapper.py",
            "model.safetensors",
            "out",
            "song.wav",
            StemAttempt::CudaSegmented,
            "6",
        );
        assert!(seg.windows(2).any(|pair| pair == ["--device", "cuda"]));
        assert!(seg.windows(2).any(|pair| pair == ["--chunk-seconds", "6"]));

        let cpu = build_melbandroformer_args(
            "wrapper.py",
            "model.safetensors",
            "out",
            "song.wav",
            StemAttempt::Cpu,
            "10",
        );
        assert!(cpu.windows(2).any(|pair| pair == ["--device", "cpu"]));
    }

    #[test]
    fn melbandroformer_outputs_are_collected_by_the_shared_collector() {
        let root = std::env::temp_dir().join(format!(
            "esan-mbrf-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        ));
        let dir = root.join(MELBANDROFORMER_MODEL).join("song");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("vocals.wav"), b"x").unwrap();
        fs::write(dir.join("instrumental.wav"), b"x").unwrap();

        let paths = collect_stem_paths(&root, MELBANDROFORMER_MODEL, Path::new("song.wav"));
        assert_eq!(paths.len(), 2);
        assert!(paths.iter().any(|p| p.ends_with("vocals.wav")));
        assert!(paths.iter().any(|p| p.ends_with("instrumental.wav")));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn is_melbandroformer_matches_only_the_vocal_model_id() {
        assert!(is_melbandroformer(MELBANDROFORMER_MODEL));
        assert!(!is_melbandroformer("htdemucs_ft"));
        assert!(!is_melbandroformer("htdemucs_6s"));
    }

    #[test]
    fn vocal_backend_requires_dependencies_ffmpeg_and_weights() {
        assert!(melbandroformer_ready(true, true, true));
        assert!(!melbandroformer_ready(true, true, false));
        assert!(!melbandroformer_ready(true, false, true));
        assert!(!melbandroformer_ready(false, true, true));
    }

    #[test]
    fn installer_rejects_unknown_purpose() {
        assert!(stem_install_backend("unknown").is_err());
        assert!(stem_install_backend("vocals").is_ok());
        assert!(stem_install_backend("instruments").is_ok());
        assert!(stem_install_backend("instruments_6s").is_ok());
    }

    #[test]
    fn melbandroformer_weights_sha256_is_a_valid_pinned_hash() {
        assert_eq!(MELBANDROFORMER_WEIGHTS_SHA256.len(), 64);
        assert!(MELBANDROFORMER_WEIGHTS_SHA256
            .bytes()
            .all(|b| b.is_ascii_hexdigit()));
        let url = melbandroformer_weights_url();
        assert!(url.contains(MELBANDROFORMER_WEIGHTS_REVISION));
        assert!(!url.contains("/resolve/main/"));
    }

    #[test]
    fn python_separation_processes_force_utf8_output() {
        let mut command = Command::new("python");
        configure_python_utf8(&mut command);
        let envs = command
            .get_envs()
            .filter_map(|(key, value)| {
                Some((
                    key.to_string_lossy().into_owned(),
                    value?.to_string_lossy().into_owned(),
                ))
            })
            .collect::<Vec<_>>();

        assert!(envs
            .iter()
            .any(|(key, value)| { key == "PYTHONIOENCODING" && value == "utf-8" }));
        assert!(envs
            .iter()
            .any(|(key, value)| key == "PYTHONUTF8" && value == "1"));
    }

    #[test]
    fn maps_backend_stems_to_filename_prefixes() {
        assert_eq!(stem_output_prefix("htdemucs_ft", "vocals"), Some("[Vocal]"));
        assert_eq!(stem_output_prefix("htdemucs_ft", "drums"), Some("[Drum]"));
        assert_eq!(stem_output_prefix("htdemucs_ft", "bass"), Some("[Bass]"));
        assert_eq!(
            stem_output_prefix("melbandroformer_vocals", "instrumental"),
            Some("[INST]")
        );
    }

    #[test]
    fn allocates_prefixed_path_without_overwriting_existing_file() {
        let root = std::env::temp_dir().join(format!(
            "esan-prefix-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("[Vocal] song.wav"), b"existing").unwrap();

        let path = prefixed_output_path(&root, "[Vocal]", "song.wav");
        assert_eq!(path.file_name().unwrap(), "[Vocal] song (1).wav");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn flattens_backend_outputs_next_to_source_with_prefixes() {
        let root = std::env::temp_dir().join(format!(
            "esan-flatten-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        ));
        let nested = root.join("htdemucs_ft").join("song");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("vocals.wav"), b"vocal").unwrap();
        fs::write(nested.join("drums.wav"), b"drum").unwrap();

        let paths = flatten_stem_paths(&root, "htdemucs_ft", &root.join("song.wav")).unwrap();
        assert_eq!(paths.len(), 2);
        assert!(root.join("[Vocal] song.wav").is_file());
        assert!(root.join("[Drum] song.wav").is_file());
        assert!(!nested.exists());
        let _ = fs::remove_dir_all(&root);
    }
}
