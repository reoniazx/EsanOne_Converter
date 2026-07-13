//! Native Essentia helper used for musical key and BPM analysis.
//!
//! The helper is the official Essentia `streaming_extractor_music` static
//! executable.  It contains the C++ Essentia algorithms and writes a JSON
//! descriptor pool, so the Tauri process does not need a Python binding.

use std::fs;
use std::path::PathBuf;

use serde_json::Value;

use super::analyze::AnalyzeResult;

#[cfg(windows)]
const EXTRACTOR_BYTES: &[u8] =
    include_bytes!("../resources/essentia/windows/streaming_extractor_music.exe");
#[cfg(target_os = "linux")]
const EXTRACTOR_BYTES: &[u8] =
    include_bytes!("../resources/essentia/linux/streaming_extractor_music");
#[cfg(target_os = "macos")]
const EXTRACTOR_BYTES: &[u8] =
    include_bytes!("../resources/essentia/macos/streaming_extractor_music");

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
const EXTRACTOR_BYTES: &[u8] = &[];

#[cfg(windows)]
const EXTRACTOR_NAME: &str = "streaming_extractor_music.exe";
#[cfg(not(windows))]
const EXTRACTOR_NAME: &str = "streaming_extractor_music";

/// Writes the platform-specific static extractor to a stable temp location.
///
/// Keeping the extracted helper outside the app install folder avoids needing
/// elevated permissions on Windows and lets the same app binary work from a
/// portable install. The file is replaced if a newer embedded copy is used.
pub fn extractor_path() -> Result<PathBuf, String> {
    if EXTRACTOR_BYTES.is_empty() {
        return Err("Essentia analysis is not available on this platform yet.".into());
    }

    let dir = std::env::temp_dir()
        .join("esanone-convert")
        .join("essentia");
    fs::create_dir_all(&dir)
        .map_err(|error| format!("Cannot prepare Essentia helper folder: {error}"))?;
    let path = dir.join(EXTRACTOR_NAME);

    let needs_write = match fs::metadata(&path) {
        Ok(metadata) => metadata.len() != EXTRACTOR_BYTES.len() as u64,
        Err(_) => true,
    };
    if needs_write {
        let partial = path.with_extension(if cfg!(windows) { "exe.part" } else { "part" });
        fs::write(&partial, EXTRACTOR_BYTES)
            .map_err(|error| format!("Cannot write Essentia helper: {error}"))?;
        fs::rename(&partial, &path)
            .map_err(|error| format!("Cannot activate Essentia helper: {error}"))?;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&path)
            .map_err(|error| format!("Cannot inspect Essentia helper: {error}"))?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions)
            .map_err(|error| format!("Cannot make Essentia helper executable: {error}"))?;
    }

    Ok(path)
}

fn nested<'a>(root: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter().try_fold(root, |value, key| value.get(*key))
}

fn number(root: &Value, path: &[&str]) -> Option<f32> {
    nested(root, path)?.as_f64().map(|value| value as f32)
}

fn string(root: &Value, path: &[&str]) -> Option<String> {
    nested(root, path)?.as_str().map(str::to_string)
}

/// Parses the JSON pool written by `streaming_extractor_music`.
pub fn parse_result(json: &str) -> Result<AnalyzeResult, String> {
    let value: Value = serde_json::from_str(json)
        .map_err(|error| format!("Essentia returned invalid JSON: {error}"))?;
    let bpm = number(&value, &["rhythm", "bpm"])
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or_else(|| "Essentia did not return a valid BPM.".to_string())?;
    let key_name = string(&value, &["tonal", "key_key"])
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "Essentia did not return a musical key.".to_string())?;
    let key_scale = string(&value, &["tonal", "key_scale"]).unwrap_or_else(|| "major".into());
    let key = if key_scale.eq_ignore_ascii_case("minor") {
        format!("{key_name}m")
    } else {
        key_name
    };
    let duration_sec = number(&value, &["metadata", "audio_properties", "length"])
        .or_else(|| number(&value, &["durationSec"]))
        .filter(|value| value.is_finite() && *value >= 0.0)
        .ok_or_else(|| "Essentia did not return the audio duration.".to_string())?;

    Ok(AnalyzeResult {
        bpm: (bpm * 10.0).round() / 10.0,
        key,
        duration_sec: (duration_sec * 100.0).round() / 100.0,
    })
}

#[cfg(test)]
mod tests {
    use super::parse_result;

    #[test]
    fn parses_essentia_key_bpm_and_duration() {
        let result = parse_result(
            r#"{
                "rhythm": {"bpm": 123.456},
                "tonal": {"key_key": "A", "key_scale": "minor"},
                "metadata": {"audio_properties": {"length": 42.3456}}
            }"#,
        )
        .expect("valid Essentia result");

        assert_eq!(result.key, "Am");
        assert_eq!(result.bpm, 123.5);
        assert_eq!(result.duration_sec, 42.35);
    }

    #[test]
    fn rejects_missing_key() {
        let error = parse_result(
            r#"{
                "rhythm": {"bpm": 120},
                "metadata": {"audio_properties": {"length": 1}}
            }"#,
        )
        .expect_err("missing key should fail");

        assert!(error.contains("musical key"));
    }
}
