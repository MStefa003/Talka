use std::sync::Arc;
use anyhow::Result;
use parking_lot::Mutex;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

mod audio;
mod commands;
mod output;
mod state;
mod transcriber;

use state::{AppConfig, AppState, HotkeyMode, SharedState};

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // Load persisted config (or use defaults).
            let config = load_config(app.handle());

            // Create shared state.
            let shared: SharedState = Arc::new(Mutex::new(AppState::new(config)));
            app.manage(shared.clone());

            // Register global shortcut.
            register_shortcut(app.handle(), shared.clone())?;

            // Build system tray.
            build_tray(app)?;

            // Auto-download ggml-base on first run if no model is present.
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let data_dir = match app_handle.path().app_data_dir() {
                    Ok(d) => d,
                    Err(_) => return,
                };
                if !crate::transcriber::list_models(&data_dir)
                    .iter()
                    .any(|m| m.downloaded)
                {
                    let _ = commands::do_download("ggml-base".to_string(), &app_handle).await;
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::set_config,
            commands::get_audio_devices,
            commands::get_models,
            commands::download_model,
            commands::open_models_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tauri application");
}

// ---------------------------------------------------------------------------
// Global shortcut registration (called on startup and after config change)
// ---------------------------------------------------------------------------

pub fn register_shortcut(app: &tauri::AppHandle, shared: SharedState) -> Result<()> {
    use tauri_plugin_global_shortcut::Shortcut;

    // Unregister all existing shortcuts before re-registering.
    let _ = app.global_shortcut().unregister_all();

    let config = shared.lock().config.clone();
    let hotkey_str = config.hotkey.clone();
    let mode = config.mode.clone();

    let shortcut: Shortcut = hotkey_str
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid hotkey '{}': {:?}", hotkey_str, e))?;

    let app_handle = app.clone();
    let shared_clone = shared.clone();

    app.global_shortcut()
        .on_shortcut(shortcut, move |_app, _shortcut, event| {
            match mode {
                HotkeyMode::PushToTalk => match event.state {
                    ShortcutState::Pressed => {
                        commands::start_recording(&app_handle, &shared_clone);
                    }
                    ShortcutState::Released => {
                        commands::stop_recording(&app_handle, &shared_clone);
                    }
                },
                HotkeyMode::Toggle => {
                    if event.state == ShortcutState::Pressed {
                        let status = shared_clone.lock().status.clone();
                        if status == state::AppStatus::Idle {
                            commands::start_recording(&app_handle, &shared_clone);
                        } else if status == state::AppStatus::Recording {
                            commands::stop_recording(&app_handle, &shared_clone);
                        }
                    }
                }
            }
        })?;

    Ok(())
}

// ---------------------------------------------------------------------------
// System tray
// ---------------------------------------------------------------------------

fn build_tray(app: &mut tauri::App) -> Result<()> {
    let settings_item = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit Talka", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&settings_item, &quit_item])?;

    TrayIconBuilder::new()
        .icon(app.default_window_icon().cloned().unwrap())
        .menu(&menu)
        .tooltip("Talka — speech to text")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "settings" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            // Double-click on tray opens settings
            if let tauri::tray::TrayIconEvent::DoubleClick { .. } = event {
                let app = tray.app_handle();
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Config persistence using tauri-plugin-store
// ---------------------------------------------------------------------------

fn load_config(app: &tauri::AppHandle) -> AppConfig {
    use tauri_plugin_store::StoreExt;

    let store = match app.store("settings.json") {
        Ok(s) => s,
        Err(_) => return AppConfig::default(),
    };

    let json = store.get("config").unwrap_or(serde_json::Value::Null);
    serde_json::from_value(json).unwrap_or_default()
}

/// Called from commands::set_config to persist the new config.
pub fn save_config(app: &tauri::AppHandle, config: &AppConfig) {
    use tauri_plugin_store::StoreExt;

    if let Ok(store) = app.store("settings.json") {
        let val = serde_json::to_value(config).unwrap_or_default();
        let _ = store.set("config", val);
        let _ = store.save();
    }
}
