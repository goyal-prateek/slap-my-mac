use crate::config::Config;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsDto {
  pub sensitivity: f64,
  pub cooldown_ms: u64,
  pub detection_enabled: bool,
  pub slap_count: u64,
  pub sensor_connected: bool,
  pub uses_custom_sound: bool,
  pub max_slap_sound_seconds: u32,
  pub slap_volume_percent: u8,
  pub launch_at_login: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetSettingsPayload {
  pub sensitivity: f64,
  pub cooldown_ms: u64,
  pub detection_enabled: bool,
  pub slap_volume_percent: u8,
  /// When absent (older clients), keep current `launch_at_login` in config.
  #[serde(default)]
  pub launch_at_login: Option<bool>,
}

pub struct AppState {
  path: PathBuf,
  custom_sound_path: PathBuf,
  inner: RwLock<Inner>,
}

struct Inner {
  config: Config,
  last_fire: Option<std::time::Instant>,
  sensor_connected: bool,
}

impl AppState {
  pub fn load_or_default(config_path: PathBuf, custom_sound_path: PathBuf) -> Self {
    let config = std::fs::read_to_string(&config_path)
      .ok()
      .and_then(|s| serde_json::from_str(&s).ok())
      .unwrap_or_default();
    Self {
      path: config_path,
      custom_sound_path,
      inner: RwLock::new(Inner {
        config,
        last_fire: None,
        sensor_connected: false,
      }),
    }
  }

  pub fn custom_sound_path(&self) -> &PathBuf {
    &self.custom_sound_path
  }

  pub fn set_sensor_connected(&self, ok: bool) {
    self.inner.write().sensor_connected = ok;
  }

  pub fn dto(&self) -> SettingsDto {
    let g = self.inner.read();
    SettingsDto {
      sensitivity: g.config.sensitivity,
      cooldown_ms: g.config.cooldown_ms,
      detection_enabled: g.config.detection_enabled,
      slap_count: g.config.slap_count,
      sensor_connected: g.sensor_connected,
      uses_custom_sound: self.custom_sound_path.is_file(),
      max_slap_sound_seconds: crate::audio::MAX_SLAP_SOUND_SECS,
      slap_volume_percent: g.config.slap_volume_percent,
      launch_at_login: g.config.launch_at_login,
    }
  }

  pub fn slap_volume_percent(&self) -> u8 {
    self.inner.read().config.slap_volume_percent.clamp(0, 100)
  }

  pub fn set_preferences(&self, p: &SetSettingsPayload) -> Result<(), String> {
    let mut g = self.inner.write();
    g.config.sensitivity = p.sensitivity.clamp(1.0, 10.0);
    g.config.cooldown_ms = p.cooldown_ms.clamp(80, 3000);
    g.config.detection_enabled = p.detection_enabled;
    g.config.slap_volume_percent = p.slap_volume_percent.clamp(0, 100);
    if let Some(v) = p.launch_at_login {
      g.config.launch_at_login = v;
    }
    drop(g);
    self.save().map_err(|e| e.to_string())
  }

  pub fn reset_counter(&self) -> Result<(), String> {
    let mut g = self.inner.write();
    g.config.slap_count = 0;
    drop(g);
    self.save().map_err(|e| e.to_string())
  }

  fn save(&self) -> std::io::Result<()> {
    if let Some(parent) = self.path.parent() {
      std::fs::create_dir_all(parent)?;
    }
    let g = self.inner.read();
    let json = serde_json::to_string_pretty(&g.config).map_err(|e| {
      std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
    })?;
    drop(g);
    std::fs::write(&self.path, json)
  }

  pub fn peek_detection(&self) -> (f64, u64, bool) {
    let g = self.inner.read();
    (
      g.config.impulse_threshold(),
      g.config.cooldown_ms,
      g.config.detection_enabled,
    )
  }

  /// Register a slap from the motion sensor (respects cooldown + enabled).
  pub fn try_consume_slap(&self, _strength: f64) -> Option<u64> {
    let mut g = self.inner.write();
    if !g.config.detection_enabled {
      return None;
    }
    let now = std::time::Instant::now();
    if let Some(prev) = g.last_fire {
      if now.duration_since(prev).as_millis() < g.config.cooldown_ms as u128 {
        return None;
      }
    }
    g.last_fire = Some(now);
    g.config.slap_count += 1;
    let c = g.config.slap_count;
    drop(g);
    let _ = self.save();
    Some(c)
  }

  pub fn record_test_slap(&self) -> u64 {
    let mut g = self.inner.write();
    g.config.slap_count += 1;
    let c = g.config.slap_count;
    drop(g);
    let _ = self.save();
    c
  }
}

pub fn update_tray_tooltip(app: &AppHandle, state: &Arc<AppState>) {
  let dto = state.dto();
  let status = if dto.sensor_connected {
    "sensor on"
  } else {
    "sensor off"
  };
  let tip = format!(
    "Slap My Mac — {} slaps · {}",
    dto.slap_count, status
  );
  let _ = app.run_on_main_thread({
    let app = app.clone();
    let tip = tip.clone();
    move || {
      if let Some(tray) = app.tray_by_id("tray") {
        let _ = tray.set_tooltip(Some(tip));
      }
    }
  });
}

pub fn emit_slap_event(app: &AppHandle, total: u64, strength: f64) {
  let _ = app.emit(
    "slap",
    serde_json::json!({
      "totalCount": total,
      "strength": strength,
    }),
  );
}
