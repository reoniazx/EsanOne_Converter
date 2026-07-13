//! OS cascading context menus for local audio files (Win / macOS / Linux).

use std::path::PathBuf;
use std::process::Command;

use serde::Serialize;
use tauri::AppHandle;
#[cfg(not(windows))]
use tauri::Manager;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellMenuStatus {
    pub registered: bool,
    pub available: bool,
    pub detail: String,
}

#[cfg(windows)]
fn exe_path(_app: &AppHandle) -> Result<PathBuf, String> {
    // Tauri's executable_dir is unsupported on Windows; current_exe is the app binary.
    let current =
        std::env::current_exe().map_err(|e| format!("Cannot resolve current exe: {e}"))?;
    resolve_windows_executable_path(current)
}

#[cfg(not(windows))]
fn exe_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .executable_dir()
        .map_err(|e| format!("Cannot resolve executable dir: {e}"))?;
    std::env::current_exe().map_err(|e| format!("Cannot resolve current exe: {e}"))
}

#[cfg(windows)]
fn resolve_windows_executable_path(current: PathBuf) -> Result<PathBuf, String> {
    if current.is_file() {
        Ok(current)
    } else {
        Err(format!("Executable does not exist: {}", current.display()))
    }
}

#[cfg(windows)]
fn build_windows_register_script(exe: &str) -> String {
    format!(
        r#"
$exe = '{exe}'
  $base = 'HKCU:\Software\Classes\SystemFileAssociations\audio\shell\EsanOneConvert'
Remove-Item -Path $base -Recurse -Force -ErrorAction SilentlyContinue
New-Item -Path $base -Force | Out-Null
New-ItemProperty -Path $base -Name 'MUIVerb' -Value 'EsanOne Converter' -PropertyType String -Force | Out-Null
New-ItemProperty -Path $base -Name 'SubCommands' -Value '' -PropertyType String -Force | Out-Null

function Set-Verb($id, $label, $arguments) {{
  $p = "$base\shell\$id"
  New-Item -Path $p -Force | Out-Null
  New-ItemProperty -Path $p -Name 'MUIVerb' -Value $label -PropertyType String -Force | Out-Null
  $cmd = "$p\command"
  New-Item -Path $cmd -Force | Out-Null
  Set-ItemProperty -Path $cmd -Name '(default)' -Value ('"' + $exe + '" ' + $arguments)
}}

Set-Verb 'vocals' 'Vocals (MelBandRoFormer)' 'separate --model melbandroformer_vocals --path "%1"'
Set-Verb 'instruments' 'Instruments (HT-Demucs)' 'separate --model htdemucs_ft --path "%1"'
Set-Verb 'instruments6' 'Instruments 6 (Guitar + Piano)' 'separate --model htdemucs_6s --path "%1"'
Set-Verb 'analyze' 'Analyze Key & BPM' 'analyze --path "%1"'
"#
    )
}

#[cfg(windows)]
fn register_windows(exe: &str) -> Result<(), String> {
    let script = build_windows_register_script(exe);
    let status = Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .status()
        .map_err(|e| format!("Failed to start PowerShell: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("Registry update failed (code {:?})", status.code()))
    }
}

#[cfg(windows)]
fn unregister_windows() -> Result<(), String> {
    let status = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "Remove-Item -Path 'HKCU:\\Software\\Classes\\SystemFileAssociations\\audio\\shell\\EsanOneConvert' -Recurse -Force -ErrorAction SilentlyContinue; Remove-Item -Path 'HKCU:\\Software\\Classes\\SystemFileAssociations\\audio\\shell\\EsanOneStem' -Recurse -Force -ErrorAction SilentlyContinue",
        ])
        .status()
        .map_err(|e| format!("Failed to start PowerShell: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "Registry cleanup failed (code {:?})",
            status.code()
        ))
    }
}

#[cfg(windows)]
fn windows_registered() -> bool {
    Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "(Test-Path 'HKCU:\\Software\\Classes\\SystemFileAssociations\\audio\\shell\\EsanOneConvert') -or (Test-Path 'HKCU:\\Software\\Classes\\SystemFileAssociations\\audio\\shell\\EsanOneStem')",
        ])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "True")
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn services_plist_path() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(
        PathBuf::from(home)
            .join("Library")
            .join("Services")
            .join("EsanOneConvert.workflow"),
    )
}

#[cfg(target_os = "macos")]
fn register_macos(exe: &str) -> Result<(), String> {
    // Lightweight Services wrapper: shell script + Info.plist via automator-compatible layout.
    let home = std::env::var("HOME").map_err(|_| "HOME missing".to_string())?;
    let dir = PathBuf::from(&home)
        .join("Library")
        .join("Services")
        .join("EsanOneConvert.workflow")
        .join("Contents");
    std::fs::create_dir_all(dir.join("MacOS")).map_err(|e| e.to_string())?;
    let script = format!(
        "#!/bin/bash\nfor f in \"$@\"; do\n  \"{exe}\" separate --model melbandroformer_vocals --path \"$f\" &\ndone\n"
    );
    let script_path = dir.join("MacOS").join("run");
    std::fs::write(&script_path, script).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&script_path)
            .map_err(|e| e.to_string())?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script_path, perms).map_err(|e| e.to_string())?;
    }
    let plist = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>NSServices</key>
  <array><dict>
    <key>NSMenuItem</key><dict><key>default</key><string>EsanOne Converter — Separate Vocals</string></dict>
    <key>NSMessage</key><string>runWorkflowAsService</string>
    <key>NSSendFileTypes</key><array><string>public.audio</string></array>
  </dict></array>
</dict></plist>"#;
    std::fs::write(dir.join("Info.plist"), plist).map_err(|e| e.to_string())?;
    let _ = Command::new("/System/Library/CoreServices/pbs")
        .arg("-flush")
        .status();
    Ok(())
}

#[cfg(target_os = "macos")]
fn unregister_macos() -> Result<(), String> {
    if let Some(path) = services_plist_path() {
        let _ = std::fs::remove_dir_all(path);
    }
    if let Ok(home) = std::env::var("HOME") {
        let _ = std::fs::remove_dir_all(
            PathBuf::from(home)
                .join("Library")
                .join("Services")
                .join("EsanOneStem.workflow"),
        );
    }
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn desktop_path() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(
        PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("file-manager")
            .join("actions")
            .join("esanone-convert.desktop"),
    )
}

#[cfg(all(unix, not(target_os = "macos")))]
fn nautilus_scripts_dir() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(
        PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("nautilus")
            .join("scripts")
            .join("EsanOne Converter"),
    )
}

#[cfg(all(unix, not(target_os = "macos")))]
fn shell_double_quote(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "\\$")
        .replace('`', "\\`")
}

#[cfg(all(unix, not(target_os = "macos")))]
fn register_linux(exe: &str) -> Result<(), String> {
    let path = desktop_path().ok_or_else(|| "HOME missing".to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // Also write MIME actions desktop for common audio types.
    let apps = PathBuf::from(std::env::var("HOME").unwrap_or_default())
        .join(".local")
        .join("share")
        .join("applications")
        .join("esanone-convert.desktop");
    if let Some(parent) = apps.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = format!(
        r#"[Desktop Entry]
Type=Application
Name=EsanOne Converter
Comment=Separate stems
Exec="{exe}" separate --model melbandroformer_vocals --path %f
Icon=audio-x-generic
Terminal=false
Categories=AudioVideo;Audio;
MimeType=audio/mpeg;audio/wav;audio/flac;audio/ogg;audio/x-m4a;
Actions=vocals;instruments;instruments6;

[Desktop Action vocals]
Name=EsanOne Converter — Vocals
Exec="{exe}" separate --model melbandroformer_vocals --path %f

[Desktop Action instruments]
Name=EsanOne Converter — Instruments 4
Exec="{exe}" separate --model htdemucs_ft --path %f

[Desktop Action instruments6]
Name=EsanOne Converter — Instruments 6
Exec="{exe}" separate --model htdemucs_6s --path %f
"#
    );
    std::fs::write(&apps, &content).map_err(|e| e.to_string())?;
    std::fs::write(&path, &content).map_err(|e| e.to_string())?;
    if let Some(scripts_dir) = nautilus_scripts_dir() {
        std::fs::create_dir_all(&scripts_dir).map_err(|e| e.to_string())?;
        let script_exe = shell_double_quote(exe);
        for (name, model) in [
            ("Vocals", "melbandroformer_vocals"),
            ("Instruments 4", "htdemucs_ft"),
            ("Instruments 6", "htdemucs_6s"),
        ] {
            let script = format!(
                "#!/bin/sh\nset -eu\nfor file in \"$@\"; do\n  \"{script_exe}\" separate --model {model} --path \"$file\" &\ndone\n"
            );
            let script_path = scripts_dir.join(name);
            std::fs::write(&script_path, script).map_err(|e| e.to_string())?;
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&script_path)
                .map_err(|e| e.to_string())?
                .permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script_path, perms).map_err(|e| e.to_string())?;
        }
    }
    let _ = Command::new("update-desktop-database")
        .arg(apps.parent().unwrap())
        .status();
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn unregister_linux() -> Result<(), String> {
    if let Some(path) = desktop_path() {
        let _ = std::fs::remove_file(path);
    }
    if let Ok(home) = std::env::var("HOME") {
        let legacy_actions = PathBuf::from(&home)
            .join(".local")
            .join("share")
            .join("file-manager")
            .join("actions")
            .join("esanone-stem.desktop");
        let _ = std::fs::remove_file(legacy_actions);
    }
    let apps = PathBuf::from(std::env::var("HOME").unwrap_or_default())
        .join(".local")
        .join("share")
        .join("applications")
        .join("esanone-convert.desktop");
    let _ = std::fs::remove_file(apps);
    let legacy_apps = PathBuf::from(std::env::var("HOME").unwrap_or_default())
        .join(".local")
        .join("share")
        .join("applications")
        .join("esanone-stem.desktop");
    let _ = std::fs::remove_file(legacy_apps);
    if let Some(scripts_dir) = nautilus_scripts_dir() {
        let _ = std::fs::remove_dir_all(scripts_dir);
    }
    if let Ok(home) = std::env::var("HOME") {
        let scripts_root = PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("nautilus")
            .join("scripts");
        let _ = std::fs::remove_dir_all(scripts_root.join("EsanOne Stem"));
        let _ = std::fs::remove_dir_all(scripts_root.join("EsanOne Convert"));
    }
    Ok(())
}

#[tauri::command]
pub fn shell_menu_status() -> Result<ShellMenuStatus, String> {
    #[cfg(windows)]
    {
        let registered = windows_registered();
        return Ok(ShellMenuStatus {
            registered,
            available: true,
            detail: if registered {
                "Windows cascading menu registered".into()
            } else {
                "Not registered".into()
            },
        });
    }
    #[cfg(target_os = "macos")]
    {
        let registered = services_plist_path().map(|p| p.exists()).unwrap_or(false);
        return Ok(ShellMenuStatus {
            registered: false,
            available: false,
            detail: if registered {
                "macOS Quick Action needs a signed app-bundle service provider".into()
            } else {
                "macOS Quick Action is unavailable in this build".into()
            },
        });
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let registered = desktop_path().map(|p| p.exists()).unwrap_or(false)
            || nautilus_scripts_dir().map(|p| p.exists()).unwrap_or(false);
        return Ok(ShellMenuStatus {
            registered,
            available: true,
            detail: if registered {
                "Linux launcher actions + Nautilus scripts installed".into()
            } else {
                "Not registered".into()
            },
        });
    }
    #[cfg(not(any(windows, unix)))]
    {
        Ok(ShellMenuStatus {
            registered: false,
            available: false,
            detail: "Unsupported platform".into(),
        })
    }
}

#[tauri::command]
pub fn shell_menu_register(app: AppHandle) -> Result<ShellMenuStatus, String> {
    #[cfg(target_os = "macos")]
    {
        let _ = app;
        return shell_menu_status();
    }
    let exe = exe_path(&app)?;
    let exe_raw = exe.to_string_lossy().to_string();
    #[cfg(windows)]
    let exe_s = exe_raw.replace('\'', "''");
    #[cfg(not(windows))]
    let exe_s = exe_raw;
    #[cfg(windows)]
    register_windows(&exe_s)?;
    #[cfg(all(unix, not(target_os = "macos")))]
    register_linux(&exe_s)?;
    shell_menu_status()
}

#[tauri::command]
pub fn shell_menu_unregister() -> Result<ShellMenuStatus, String> {
    #[cfg(windows)]
    unregister_windows()?;
    #[cfg(target_os = "macos")]
    unregister_macos()?;
    #[cfg(all(unix, not(target_os = "macos")))]
    unregister_linux()?;
    shell_menu_status()
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn uses_current_executable_when_tauri_executable_dir_is_unavailable() {
        let current = std::env::current_exe().expect("test executable path");

        assert_eq!(
            resolve_windows_executable_path(current.clone()).unwrap(),
            current
        );
    }

    #[test]
    fn windows_register_script_includes_key_bpm_action() {
        let script = build_windows_register_script(r"C:\Apps\esanone-convert.exe");
        assert!(script.contains("Analyze Key & BPM"));
        assert!(script.contains("'analyze --path \"%1\"'"));
    }
}
