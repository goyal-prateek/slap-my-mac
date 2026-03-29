mod audio;
mod config;
mod isomp4_decode;
mod sensor;
mod state;
mod system_volume;
#[cfg(target_os = "macos")]
mod spu_wake;

use state::{
  emit_slap_event, update_tray_tooltip, AppState, SetSettingsPayload, SettingsDto,
};
use serde::Serialize;
use tauri_plugin_autostart::ManagerExt;
#[cfg(target_os = "macos")]
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
use std::sync::Arc;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::WindowEvent;
use tauri::{Manager, Emitter};

#[tauri::command]
fn get_settings(state: tauri::State<'_, Arc<AppState>>) -> SettingsDto {
  state.dto()
}

fn apply_launch_at_login(app: &tauri::AppHandle, enabled: bool) -> Result<(), String> {
  let mgr = app.autolaunch();
  if enabled {
    mgr.enable().map_err(|e| e.to_string())
  } else {
    mgr.disable().map_err(|e| e.to_string())
  }
}

#[tauri::command]
fn set_settings(
  app: tauri::AppHandle,
  state: tauri::State<'_, Arc<AppState>>,
  payload: SetSettingsPayload,
) -> Result<SettingsDto, String> {
  state.set_preferences(&payload)?;
  apply_launch_at_login(&app, state.dto().launch_at_login)?;
  update_tray_tooltip(&app, &state);
  Ok(state.dto())
}

#[tauri::command]
fn test_slap(
  app: tauri::AppHandle,
  state: tauri::State<'_, Arc<AppState>>,
) -> Result<SettingsDto, String> {
  let n = state.record_test_slap();
  let vol = state.slap_volume_percent();
  crate::audio::play_slap(vol);
  emit_slap_event(&app, n, 1.0);
  update_tray_tooltip(&app, &state);
  Ok(state.dto())
}

#[tauri::command]
fn reset_counter(
  app: tauri::AppHandle,
  state: tauri::State<'_, Arc<AppState>>,
) -> Result<SettingsDto, String> {
  state.reset_counter()?;
  update_tray_tooltip(&app, &state);
  Ok(state.dto())
}

/// True when the app is running from a mounted disk image (or any non-Applications path on /Volumes/).
/// In that case the process keeps the DMG busy and Finder cannot eject until the user quits from the menu bar.
#[cfg(target_os = "macos")]
fn running_from_install_disk() -> bool {
  let Ok(exe) = std::env::current_exe() else {
    return false;
  };
  let p = exe.to_string_lossy();
  p.starts_with("/Volumes/") && !p.contains("/Applications/")
}

#[cfg(target_os = "macos")]
fn warn_running_from_dmg(app: &tauri::AppHandle) {
  if !running_from_install_disk() {
    return;
  }
  app
    .dialog()
    .message(
      "You opened Slap My Mac from the disk image. While it is running, macOS cannot eject the DMG.\n\n\
       Install it: drag “Slap My Mac” onto the Applications folder in this window, then eject the disk image and open the app from Applications or Launchpad.\n\n\
       Or, to only try it from the disk image: when you are done, choose Quit from the menu bar (hand-on-Mac icon) before ejecting.",
    )
    .title("Install Slap My Mac")
    .kind(MessageDialogKind::Info)
    .blocking_show();
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportCustomSoundResponse {
  settings: SettingsDto,
  import: crate::audio::SoundImportInfo,
}

#[tauri::command]
fn import_custom_sound(
  app: tauri::AppHandle,
  state: tauri::State<'_, Arc<AppState>>,
  path: String,
) -> Result<ImportCustomSoundResponse, String> {
  let src = std::path::PathBuf::from(path.trim());
  if !src.is_file() {
    return Err("Could not read that file.".to_string());
  }
  let dest = state.custom_sound_path().clone();
  let import = crate::audio::import_trim_save(&src, &dest)?;
  update_tray_tooltip(&app, &state);
  Ok(ImportCustomSoundResponse {
    settings: state.dto(),
    import,
  })
}

#[tauri::command]
fn clear_custom_sound(
  app: tauri::AppHandle,
  state: tauri::State<'_, Arc<AppState>>,
) -> Result<SettingsDto, String> {
  let p = state.custom_sound_path();
  if p.is_file() {
    let _ = std::fs::remove_file(p);
  }
  update_tray_tooltip(&app, &state);
  Ok(state.dto())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .plugin(
      tauri_plugin_autostart::Builder::new()
        .app_name("Slap My Mac")
        .build(),
    )
    .plugin(tauri_plugin_dialog::init())
    .setup(|app| {
      let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
      std::fs::create_dir_all(&app_data).map_err(|e| e.to_string())?;
      let custom_sound_path = app_data.join("custom_slap.wav");
      crate::audio::set_custom_sound_path(custom_sound_path.clone());

      let cfg_dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
      std::fs::create_dir_all(&cfg_dir).map_err(|e| e.to_string())?;
      let config_path = cfg_dir.join("config.json");
      let state = Arc::new(AppState::load_or_default(config_path, custom_sound_path));
      app.manage(state.clone());

      apply_launch_at_login(app.handle(), state.dto().launch_at_login)?;

      let icon_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("icons/128x128.png");
      let icon = Image::from_path(&icon_path).map_err(|e| e.to_string())?;

      let settings = MenuItem::with_id(app, "settings", "Settings…", true, Option::<&str>::None)?;
      let test = MenuItem::with_id(app, "test", "Test slap", true, Option::<&str>::None)?;
      let reset = MenuItem::with_id(app, "reset", "Reset counter", true, Option::<&str>::None)?;
      let sep = PredefinedMenuItem::separator(app)?;
      let quit = PredefinedMenuItem::quit(app, None)?;

      let menu = Menu::with_items(
        app,
        &[&settings, &test, &reset, &sep, &quit],
      )?;

      let handle = app.handle().clone();
      TrayIconBuilder::with_id("tray")
        .icon(icon)
        .menu(&menu)
        .tooltip("Slap My Mac")
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| {
          if event.id == "settings" {
            if let Some(w) = app.get_webview_window("main") {
              let _ = w.show();
              let _ = w.set_focus();
            }
          } else if event.id == "test" {
            if let Some(s) = app.try_state::<Arc<AppState>>() {
              let n = s.record_test_slap();
              let vol = s.slap_volume_percent();
              crate::audio::play_slap(vol);
              let _ = app.emit(
                "slap",
                serde_json::json!({ "totalCount": n, "strength": 1.0f64 }),
              );
              update_tray_tooltip(app, &s);
            }
          } else if event.id == "reset" {
            if let Some(s) = app.try_state::<Arc<AppState>>() {
              let _ = s.reset_counter();
              update_tray_tooltip(app, &s);
            }
          }
        })
        .build(app)?;

      update_tray_tooltip(&handle, &state);
      crate::audio::try_init();
      sensor::spawn(handle.clone(), state);

      #[cfg(target_os = "macos")]
      warn_running_from_dmg(app.handle());

      if let Some(w) = app.get_webview_window("main") {
        let win = w.clone();
        w.on_window_event(move |e| {
          if let WindowEvent::CloseRequested { api, .. } = e {
            api.prevent_close();
            let _ = win.hide();
          }
        });
      }

      Ok(())
    })
    .invoke_handler(tauri::generate_handler![
      get_settings,
      set_settings,
      test_slap,
      reset_counter,
      import_custom_sound,
      clear_custom_sound
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
