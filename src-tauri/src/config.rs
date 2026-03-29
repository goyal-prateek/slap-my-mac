use serde::{Deserialize, Serialize};

/// Persisted preferences (sensor_connected is runtime-only).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
  /// UI slider 1.0–10.0; **higher** = pick up lighter taps; **lower** = only harder slaps.
  pub sensitivity: f64,
  pub cooldown_ms: u64,
  pub detection_enabled: bool,
  pub slap_count: u64,
  /// macOS output volume (0–100) applied only while a slap sound is playing, then restored.
  #[serde(default = "default_slap_volume_percent")]
  pub slap_volume_percent: u8,
  /// Register as a login item so the app starts when you sign in (after restart / power on).
  /// Missing in older config files deserializes as `false`; new installs use [`Config::default`].
  #[serde(default)]
  pub launch_at_login: bool,
}

fn default_slap_volume_percent() -> u8 {
  85
}

impl Default for Config {
  fn default() -> Self {
    Self {
      sensitivity: 5.0,
      cooldown_ms: 450,
      detection_enabled: true,
      slap_count: 0,
      slap_volume_percent: default_slap_volume_percent(),
      launch_at_login: true,
    }
  }
}

impl Config {
  pub fn impulse_threshold(&self) -> f64 {
    let s = self.sensitivity.clamp(1.0, 10.0);
    // Higher score must exceed threshold to fire. So **larger** threshold = stricter (hard slaps only).
    // s = 1 → strict (only hard hits). s = 10 → permissive (lighter taps).
    // Values tuned so casual finger rests / light touches stay below threshold.
    const THR_PERMISSIVE: f64 = 0.022;
    const THR_STRICT: f64 = 0.32;
    THR_STRICT - (s - 1.0) / 9.0 * (THR_STRICT - THR_PERMISSIVE)
  }
}
