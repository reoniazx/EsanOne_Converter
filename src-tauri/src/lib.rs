mod analyze;
mod cli;
mod essentia;
mod shell_integration;
mod stem_separate;
mod youtube;

use analyze::{analyze_audio, analyze_cancel, AnalyzeState};
use cli::parse_args;
use shell_integration::{shell_menu_register, shell_menu_status, shell_menu_unregister};
use stem_separate::{
    stem_gpu_install, stem_separate_start, stem_separate_stop, stem_tool_install, stem_tool_status,
    StemSeparateState,
};
#[cfg(desktop)]
use tauri::menu::{MenuBuilder, MenuItemBuilder};
#[cfg(desktop)]
use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
use tauri::{Emitter, Manager};
use youtube::{youtube_cancel, youtube_download, YoutubeState};

#[cfg(desktop)]
#[derive(Debug, PartialEq, Eq)]
enum TrayMenuAction {
    Open,
    Exit,
}

#[cfg(desktop)]
fn tray_menu_action(id: &str) -> Option<TrayMenuAction> {
    match id {
        "open" => Some(TrayMenuAction::Open),
        "exit" => Some(TrayMenuAction::Exit),
        _ => None,
    }
}

#[cfg(desktop)]
fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg(desktop)]
fn setup_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let open = MenuItemBuilder::with_id("open", "Open").build(app)?;
    let exit = MenuItemBuilder::with_id("exit", "Exit").build(app)?;
    let menu = MenuBuilder::new(app).items(&[&open, &exit]).build()?;
    let tray = app.tray_by_id("main").ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "configured tray icon `main` not found",
        )
    })?;
    tray.set_menu(Some(menu))?;
    Ok(())
}

#[tauri::command]
fn get_startup_cli() -> cli::StartupCli {
    let args: Vec<String> = std::env::args().collect();
    parse_args(&args)
}

#[tauri::command]
fn default_output_dir() -> Result<String, String> {
    let dir = stem_separate::stem_app_data_dir_pub()
        .ok_or_else(|| "Cannot resolve app data folder".to_string())?
        .join("exports");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create exports folder: {e}"))?;
    Ok(dir.to_string_lossy().to_string())
}

#[tauri::command]
fn reveal_in_folder(path: String) -> Result<(), String> {
    #[cfg(windows)]
    {
        let status = std::process::Command::new("explorer")
            .args(["/select,", &path])
            .status()
            .map_err(|e| e.to_string())?;
        if status.success() {
            return Ok(());
        }
    }
    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("open")
            .args(["-R", &path])
            .status()
            .map_err(|e| e.to_string())?;
        if status.success() {
            return Ok(());
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(parent) = std::path::Path::new(&path).parent() {
            let status = std::process::Command::new("xdg-open")
                .arg(parent)
                .status()
                .map_err(|e| e.to_string())?;
            if status.success() {
                return Ok(());
            }
        }
    }
    Err("Could not reveal file".into())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            let _ = app.emit(
                "single-instance",
                serde_json::json!({ "args": argv, "cwd": cwd }),
            );
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            #[cfg(desktop)]
            {
                app.handle()
                    .plugin(tauri_plugin_updater::Builder::new().build())?;
                setup_tray(app)?;
            }
            Ok(())
        });

    #[cfg(desktop)]
    let builder = builder
        .on_menu_event(|app, event| match tray_menu_action(event.id().as_ref()) {
            Some(TrayMenuAction::Open) => show_main_window(app),
            Some(TrayMenuAction::Exit) => app.exit(0),
            None => {}
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
        .manage(StemSeparateState::default())
        .manage(YoutubeState::default())
        .manage(AnalyzeState::default())
        .invoke_handler(tauri::generate_handler![
            get_startup_cli,
            default_output_dir,
            reveal_in_folder,
            stem_tool_status,
            stem_tool_install,
            stem_gpu_install,
            stem_separate_start,
            stem_separate_stop,
            youtube_download,
            youtube_cancel,
            analyze_audio,
            analyze_cancel,
            shell_menu_status,
            shell_menu_register,
            shell_menu_unregister,
        ])
        .run(tauri::generate_context!())
        .expect("error while running EsanOne Converter");
}

#[cfg(all(test, desktop))]
mod tray_tests {
    use super::*;

    #[test]
    fn tray_menu_actions_route_open_and_exit() {
        assert_eq!(tray_menu_action("open"), Some(TrayMenuAction::Open));
        assert_eq!(tray_menu_action("exit"), Some(TrayMenuAction::Exit));
        assert_eq!(tray_menu_action("unknown"), None);
    }
}
