//! YouTube audio download via managed yt-dlp.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::stem_separate;

pub const YOUTUBE_EVENT: &str = "youtube-download-event";
pub const YOUTUBE_DONE: &str = "youtube-download-done";
const YT_DLP_VERSION: &str = "2026.07.04";
const YT_DLP_SHA256_WINDOWS: &str =
    "52fe3c26dcf71fbdc85b528589020bb0b8e383155cfa81b64dd447bbe35e24b8";
#[cfg(not(windows))]
const YT_DLP_SHA256_UNIX: &str = "495be29ff4d9d4e9be7eabdfef225221e5d5282e77f2f505abc6dca80349f3fd";

#[derive(Default, Clone)]
pub struct YoutubeState {
    pub cancelled: Arc<Mutex<bool>>,
    pub active_child: Arc<Mutex<Option<Child>>>,
    running: Arc<Mutex<bool>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LogEvent {
    session_id: String,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DoneEvent {
    session_id: String,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
}

fn emit_log(app: &AppHandle, session_id: &str, kind: &str, text: impl Into<String>) {
    let _ = app.emit(
        YOUTUBE_EVENT,
        LogEvent {
            session_id: session_id.to_string(),
            kind: kind.to_string(),
            text: Some(text.into()),
        },
    );
}

fn app_data_dir() -> Option<PathBuf> {
    stem_separate::stem_app_data_dir_pub()
}

fn tools_bin_dir() -> Option<PathBuf> {
    Some(app_data_dir()?.join("stem-tools").join("bin"))
}

fn yt_dlp_path() -> Option<PathBuf> {
    if let Some(bin) = tools_bin_dir() {
        #[cfg(windows)]
        let candidate = bin.join("yt-dlp.exe");
        #[cfg(not(windows))]
        let candidate = bin.join("yt-dlp");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn download_bytes(url: &str, limit: u64) -> Result<Vec<u8>, String> {
    stem_separate::download_bytes_pub(url, "yt-dlp", limit)
}

fn ensure_yt_dlp(app: &AppHandle, session_id: &str) -> Result<PathBuf, String> {
    if let Some(path) = yt_dlp_path() {
        return Ok(path);
    }
    emit_log(app, session_id, "status", "Installing yt-dlp…");
    let bin = tools_bin_dir().ok_or_else(|| "Cannot resolve stem-tools bin folder".to_string())?;
    std::fs::create_dir_all(&bin).map_err(|e| format!("Cannot create bin folder: {e}"))?;

    #[cfg(windows)]
    let url =
        format!("https://github.com/yt-dlp/yt-dlp/releases/download/{YT_DLP_VERSION}/yt-dlp.exe");
    #[cfg(not(windows))]
    let url = format!("https://github.com/yt-dlp/yt-dlp/releases/download/{YT_DLP_VERSION}/yt-dlp");

    #[cfg(windows)]
    let dest = bin.join("yt-dlp.exe");
    #[cfg(not(windows))]
    let dest = bin.join("yt-dlp");

    let bytes = download_bytes(&url, 80 * 1024 * 1024)?;
    #[cfg(windows)]
    let expected_sha = YT_DLP_SHA256_WINDOWS;
    #[cfg(not(windows))]
    let expected_sha = YT_DLP_SHA256_UNIX;
    stem_separate::verify_download_sha256_pub(&bytes, expected_sha, "yt-dlp")?;
    let partial = dest.with_extension("part");
    std::fs::write(&partial, &bytes).map_err(|e| format!("Cannot write yt-dlp: {e}"))?;
    std::fs::rename(&partial, &dest).map_err(|e| format!("Cannot finalize yt-dlp: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&dest)
            .map_err(|e| format!("Cannot read yt-dlp perms: {e}"))?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&dest, perms)
            .map_err(|e| format!("Cannot set yt-dlp executable: {e}"))?;
    }
    Ok(dest)
}

fn is_youtube_url(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url.trim()) else {
        return false;
    };
    if parsed.scheme() != "https" {
        return false;
    }
    matches!(
        parsed
            .host_str()
            .map(|host| host.to_ascii_lowercase())
            .as_deref(),
        Some(
            "youtube.com"
                | "www.youtube.com"
                | "m.youtube.com"
                | "music.youtube.com"
                | "youtu.be"
                | "www.youtu.be"
        )
    )
}

fn build_youtube_args(output_template: &str, url: &str) -> Vec<String> {
    vec![
        "-f".into(),
        "bestaudio/best".into(),
        "--extract-audio".into(),
        "--audio-format".into(),
        "wav".into(),
        "--audio-quality".into(),
        "0".into(),
        "-o".into(),
        output_template.into(),
        "--no-playlist".into(),
        "--print".into(),
        "after_move:filepath".into(),
        "--print".into(),
        "before_dl:title".into(),
        "--".into(),
        url.into(),
    ]
}

fn youtube_title_from_path(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?.trim();
    if stem.is_empty() {
        return None;
    }

    let title = stem
        .rsplit_once(" [")
        .and_then(|(title, suffix)| {
            let id = suffix.strip_suffix(']')?;
            (id.len() == 11
                && id.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
                }))
            .then_some(title)
        })
        .unwrap_or(stem);
    Some(title.to_string())
}

fn resolve_youtube_title(path: &Path, printed_title: Option<String>) -> Option<String> {
    youtube_title_from_path(path).or_else(|| {
        printed_title
            .map(|title| title.trim().to_string())
            .filter(|title| !title.is_empty() && !title.contains('\u{FFFD}'))
    })
}

fn wait_for_youtube_child(
    state: &YoutubeState,
    cancelled: &Arc<Mutex<bool>>,
) -> Result<ExitStatus, String> {
    loop {
        let mut guard = state.active_child.lock().unwrap();
        if *cancelled.lock().unwrap() {
            if let Some(child) = guard.as_mut() {
                let _ = child.kill();
            }
        }
        let status = guard
            .as_mut()
            .ok_or_else(|| "yt-dlp process was not started".to_string())?
            .try_wait()
            .map_err(|e| format!("Failed waiting for yt-dlp: {e}"))?;
        if let Some(status) = status {
            guard.take();
            return Ok(status);
        }
        drop(guard);
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[tauri::command]
pub fn youtube_download(
    app: AppHandle,
    state: State<'_, YoutubeState>,
    session_id: String,
    url: String,
    output_dir: String,
) -> Result<(), String> {
    if !is_youtube_url(&url) {
        return Err("Please paste a valid YouTube URL.".into());
    }
    let mut running = state.running.lock().unwrap();
    if *running {
        return Err("A YouTube download is already running.".into());
    }
    *running = true;
    drop(running);
    *state.cancelled.lock().unwrap() = false;
    let cancelled = state.cancelled.clone();
    let active_child = state.active_child.clone();
    let state_for_worker = YoutubeState {
        cancelled: cancelled.clone(),
        active_child,
        running: state.running.clone(),
    };

    std::thread::spawn(move || {
        let finish =
            |ok: bool, error: Option<String>, path: Option<String>, title: Option<String>| {
                let _ = app.emit(
                    YOUTUBE_DONE,
                    DoneEvent {
                        session_id: session_id.clone(),
                        ok,
                        error,
                        path,
                        title,
                    },
                );
            };

        let result = (|| {
            let yt = ensure_yt_dlp(&app, &session_id)?;
            std::fs::create_dir_all(&output_dir)
                .map_err(|e| format!("Cannot create output folder: {e}"))?;
            emit_log(&app, &session_id, "status", "Fetching from YouTube…");

            let out_template = Path::new(&output_dir)
                .join("%(title).200B [%(id)s].%(ext)s")
                .to_string_lossy()
                .to_string();

            let args = build_youtube_args(&out_template, &url);
            let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
            let mut command = Command::new(&yt);
            command
                .args(&arg_refs)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            stem_separate::configure_python_utf8(&mut command);
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x0800_0000);
            }

            let mut child = command
                .spawn()
                .map_err(|e| format!("Failed to start yt-dlp: {e}"))?;
            let mut stdout_pipe = child
                .stdout
                .take()
                .ok_or_else(|| "yt-dlp stdout was not piped".to_string())?;
            let mut stderr_pipe = child
                .stderr
                .take()
                .ok_or_else(|| "yt-dlp stderr was not piped".to_string())?;
            let stdout_reader = std::thread::spawn(move || {
                let mut bytes = Vec::new();
                let _ = std::io::Read::read_to_end(&mut stdout_pipe, &mut bytes);
                bytes
            });
            let stderr_reader = std::thread::spawn(move || {
                let mut bytes = Vec::new();
                let _ = std::io::Read::read_to_end(&mut stderr_pipe, &mut bytes);
                bytes
            });
            *state_for_worker.active_child.lock().unwrap() = Some(child);
            let status = wait_for_youtube_child(&state_for_worker, &cancelled)?;
            let stdout = stdout_reader
                .join()
                .map_err(|_| "yt-dlp stdout reader failed".to_string())?;
            let stderr = stderr_reader
                .join()
                .map_err(|_| "yt-dlp stderr reader failed".to_string())?;
            if *cancelled.lock().unwrap() {
                return Err("Cancelled".into());
            }
            let stdout = String::from_utf8_lossy(&stdout);
            let stderr = String::from_utf8_lossy(&stderr);
            if !stderr.trim().is_empty() {
                emit_log(&app, &session_id, "stderr", stderr.trim());
            }
            if !status.success() {
                return Err(format!(
                    "yt-dlp failed: {}",
                    stderr.lines().last().unwrap_or("unknown error")
                ));
            }

            let mut title: Option<String> = None;
            let mut path: Option<String> = None;
            for line in stdout.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if Path::new(line).exists() {
                    path = Some(line.to_string());
                } else if title.is_none() {
                    title = Some(line.to_string());
                }
            }

            if path.is_none() {
                if let Ok(entries) = std::fs::read_dir(&output_dir) {
                    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
                    for entry in entries.flatten() {
                        let p = entry.path();
                        let is_wav = p
                            .extension()
                            .and_then(|e| e.to_str())
                            .map(|e| e.eq_ignore_ascii_case("wav"))
                            == Some(true);
                        if !is_wav {
                            continue;
                        }
                        if let Ok(meta) = entry.metadata() {
                            if let Ok(modified) = meta.modified() {
                                if newest.as_ref().map(|(t, _)| modified > *t).unwrap_or(true) {
                                    newest = Some((modified, p));
                                }
                            }
                        }
                    }
                    path = newest.map(|(_, p)| p.to_string_lossy().to_string());
                }
            }

            let path =
                path.ok_or_else(|| "Download finished but no audio file was found.".to_string())?;
            let title = resolve_youtube_title(Path::new(&path), title);
            Ok((path, title))
        })();

        *state_for_worker.running.lock().unwrap() = false;

        match result {
            Ok((path, title)) => finish(true, None, Some(path), title),
            Err(error) => {
                emit_log(&app, &session_id, "error", &error);
                finish(false, Some(error), None, None);
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub fn youtube_cancel(state: State<'_, YoutubeState>) -> Result<(), String> {
    *state.cancelled.lock().unwrap() = true;
    if let Some(child) = state.active_child.lock().unwrap().as_mut() {
        let _ = child.kill();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_supported_youtube_hosts() {
        assert!(is_youtube_url("https://www.youtube.com/watch?v=abc"));
        assert!(is_youtube_url("https://youtu.be/abc"));
        assert!(!is_youtube_url(
            "https://youtube.com.evil.example/watch?v=abc"
        ));
        assert!(!is_youtube_url(
            "https://youtube.com@evil.example/watch?v=abc"
        ));
        assert!(!is_youtube_url("file://youtube.com/song.wav"));
    }

    #[test]
    fn youtube_args_keep_url_after_option_terminator() {
        let args = build_youtube_args(
            "C:/output/%(title)s.%(ext)s",
            "https://www.youtube.com/watch?v=abc",
        );
        assert_eq!(
            args.last().map(String::as_str),
            Some("https://www.youtube.com/watch?v=abc")
        );
        assert_eq!(args.get(args.len() - 2).map(String::as_str), Some("--"));
    }

    #[test]
    fn derives_title_from_unicode_downloaded_filename_when_printed_title_is_corrupt() {
        let path = Path::new(
            r"C:\Users\tester\EsanOne Stem\exports\อยากกอดเธออีกสักครา…ก่อนจากลา - ไทยตรง (Official MV) [uOlO0CPs3ac].wav",
        );

        assert_eq!(
            resolve_youtube_title(path, Some("� -  (Official MV)".into())),
            Some("อยากกอดเธออีกสักครา…ก่อนจากลา - ไทยตรง (Official MV)".into())
        );
    }

    #[test]
    fn youtube_processes_force_utf8_output() {
        let mut command = Command::new("yt-dlp");
        stem_separate::configure_python_utf8(&mut command);
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
}
