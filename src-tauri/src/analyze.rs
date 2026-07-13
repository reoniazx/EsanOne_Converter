//! Audio analysis: BPM + musical key via the native Essentia extractor.

use std::io::Read;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::essentia;

pub const ANALYZE_EVENT: &str = "analyze-audio-event";
pub const ANALYZE_DONE: &str = "analyze-audio-done";

#[derive(Default, Clone)]
pub struct AnalyzeState {
    cancelled: Arc<Mutex<bool>>,
    active_child: Arc<Mutex<Option<Child>>>,
    running: Arc<Mutex<bool>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeResult {
    pub bpm: f32,
    pub key: String,
    pub duration_sec: f32,
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
    result: Option<AnalyzeResult>,
}

fn emit_log(app: &AppHandle, session_id: &str, kind: &str, text: impl Into<String>) {
    let _ = app.emit(
        ANALYZE_EVENT,
        LogEvent {
            session_id: session_id.to_string(),
            kind: kind.to_string(),
            text: Some(text.into()),
        },
    );
}

fn wait_for_analyzer(
    state: &AnalyzeState,
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
            .ok_or_else(|| "Analyzer process was not started".to_string())?
            .try_wait()
            .map_err(|e| format!("Failed waiting for analyzer: {e}"))?;
        if let Some(status) = status {
            guard.take();
            return Ok(status);
        }
        drop(guard);
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn run_analysis(
    app: &AppHandle,
    state: &AnalyzeState,
    cancelled: &Arc<Mutex<bool>>,
    session_id: &str,
    input_path: &str,
) -> Result<AnalyzeResult, String> {
    emit_log(
        app,
        session_id,
        "status",
        "Analyzing key & BPM with Essentia…",
    );
    let extractor = essentia::extractor_path()?;
    let mut command = Command::new(&extractor);
    command
        // `-` makes the official extractor write JSON to stdout. This keeps
        // output paths ASCII-independent for users whose temp folder or home
        // directory contains non-Latin characters.
        .args([input_path, "-"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("Failed to start analyzer: {e}"))?;
    let mut stdout_pipe = child
        .stdout
        .take()
        .ok_or_else(|| "Analyzer stdout was not piped".to_string())?;
    let mut stderr_pipe = child
        .stderr
        .take()
        .ok_or_else(|| "Analyzer stderr was not piped".to_string())?;
    let stdout_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout_pipe.read_to_end(&mut bytes);
        bytes
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stderr_pipe.read_to_end(&mut bytes);
        bytes
    });
    *state.active_child.lock().unwrap() = Some(child);
    let status = wait_for_analyzer(state, cancelled)?;
    let stdout = stdout_reader
        .join()
        .map_err(|_| "Analyzer stdout reader failed".to_string())?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| "Analyzer stderr reader failed".to_string())?;
    if *cancelled.lock().unwrap() {
        return Err("Cancelled".into());
    }
    let stdout = String::from_utf8_lossy(&stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&stderr).trim().to_string();
    if !stderr.is_empty() {
        emit_log(app, session_id, "stderr", &stderr);
    }
    if !status.success() {
        return Err(if stderr.is_empty() {
            "Essentia analysis failed.".into()
        } else {
            format!("Essentia analysis failed: {stderr}")
        });
    }
    essentia::parse_result(&stdout)
}

fn start_analysis(
    app: AppHandle,
    state: &AnalyzeState,
    session_id: String,
    input_path: String,
) -> Result<(), String> {
    let mut running = state.running.lock().unwrap();
    if *running {
        return Err("Audio analysis is already running.".into());
    }
    *running = true;
    drop(running);
    *state.cancelled.lock().unwrap() = false;
    let cancelled = state.cancelled.clone();
    let state = state.clone();
    std::thread::spawn(move || {
        let finish = |ok: bool, error: Option<String>, result: Option<AnalyzeResult>| {
            let _ = app.emit(
                ANALYZE_DONE,
                DoneEvent {
                    session_id: session_id.clone(),
                    ok,
                    error,
                    result,
                },
            );
        };

        let result = run_analysis(&app, &state, &cancelled, &session_id, &input_path);

        state.active_child.lock().unwrap().take();
        *state.running.lock().unwrap() = false;

        match result {
            Ok(parsed) => finish(true, None, Some(parsed)),
            Err(error) => {
                emit_log(&app, &session_id, "error", &error);
                finish(false, Some(error), None);
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub fn analyze_audio(
    app: AppHandle,
    state: tauri::State<'_, AnalyzeState>,
    session_id: String,
    input_path: String,
) -> Result<(), String> {
    start_analysis(app, state.inner(), session_id, input_path)
}

#[tauri::command]
pub fn analyze_cancel(state: tauri::State<'_, AnalyzeState>) -> Result<(), String> {
    *state.cancelled.lock().unwrap() = true;
    if let Some(child) = state.active_child.lock().unwrap().as_mut() {
        let _ = child.kill();
    }
    Ok(())
}
