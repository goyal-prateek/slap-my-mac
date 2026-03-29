use crate::audio;
use crate::state::{emit_slap_event, update_tray_tooltip, AppState};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tauri::AppHandle;

#[cfg(target_os = "macos")]
use crate::spu_wake;

/// Preferred byte offsets for BMI286-style reports (after optional report ID).
const PARSE_BASES: [usize; 9] = [6, 7, 5, 8, 4, 3, 2, 1, 0];

#[cfg(target_os = "macos")]
fn parse_at(buf: &[u8], base: usize) -> Option<(f64, f64, f64)> {
  if buf.len() < base + 12 {
    return None;
  }
  let xi = i32::from_le_bytes(buf[base..base + 4].try_into().ok()?);
  let yi = i32::from_le_bytes(buf[base + 4..base + 8].try_into().ok()?);
  let zi = i32::from_le_bytes(buf[base + 8..base + 12].try_into().ok()?);
  let x = xi as f64 / 65536.0;
  let y = yi as f64 / 65536.0;
  let z = zi as f64 / 65536.0;
  let m = (x * x + y * y + z * z).sqrt();
  if m > 0.3 && m < 5.0 {
    Some((x, y, z))
  } else {
    None
  }
}

#[cfg(target_os = "macos")]
fn scan_xyz(buf: &[u8], fixed_base: &mut Option<usize>) -> Option<(f64, f64, f64)> {
  if let Some(b) = *fixed_base {
    return parse_at(buf, b);
  }
  for &b in PARSE_BASES.iter() {
    if let Some(xyz) = parse_at(buf, b) {
      *fixed_base = Some(b);
      return Some(xyz);
    }
  }
  None
}

#[cfg(target_os = "macos")]
fn probe_accelerometer(dev: &hidapi::HidDevice) -> bool {
  let mut buf = [0u8; 64];
  for _ in 0..24 {
    match dev.read_timeout(&mut buf, 20) {
      Ok(n) if n >= 12 => {
        let mut fb = None;
        if scan_xyz(&buf[..n], &mut fb).is_some() {
          return true;
        }
      }
      _ => {}
    }
  }
  false
}

#[cfg(target_os = "macos")]
fn try_open_with_probe(
  api: &hidapi::HidApi,
  dev: &hidapi::DeviceInfo,
) -> Option<hidapi::HidDevice> {
  let d = dev.open_device(api).ok()?;
  if probe_accelerometer(&d) {
    Some(d)
  } else {
    None
  }
}

#[cfg(target_os = "macos")]
fn open_motion_device() -> Option<hidapi::HidDevice> {
  spu_wake::wake_spu_drivers();
  thread::sleep(Duration::from_millis(250));

  let api = hidapi::HidApi::new().ok()?;

  for dev in api.device_list() {
    if dev.usage_page() == 0xff00 && dev.usage() == 0x03 {
      if let Some(d) = try_open_with_probe(&api, dev) {
        return Some(d);
      }
    }
  }

  for dev in api.device_list() {
    let p = dev.path().to_string_lossy().to_lowercase();
    if p.contains("applespu") || p.contains("spuhiddevice") {
      if let Some(d) = try_open_with_probe(&api, dev) {
        return Some(d);
      }
    }
  }

  for dev in api.device_list() {
    if dev.vendor_id() != 0x05ac {
      continue;
    }
    let p = dev.path().to_string_lossy().to_lowercase();
    if !(p.contains("hid") && (p.contains("spu") || p.contains("imu"))) {
      continue;
    }
    if let Some(d) = try_open_with_probe(&api, dev) {
      return Some(d);
    }
  }

  None
}

#[cfg(target_os = "macos")]
fn run_sensor_loop(app: AppHandle, state: Arc<AppState>, device: hidapi::HidDevice) {
  let mut buf = [0u8; 64];
  let mut prev = (0.0f64, 0.0f64, 0.0f64);
  let mut primed = false;
  let mut parse_base: Option<usize> = None;
  let mut ema = (0.0f64, 0.0f64, 0.0f64);
  let ema_alpha = 0.12_f64;

  loop {
    let (threshold, _cooldown_ms, enabled) = state.peek_detection();
    if !enabled {
      thread::sleep(Duration::from_millis(120));
      primed = false;
      parse_base = None;
      continue;
    }

    let n = match device.read_timeout(&mut buf, 80) {
      Ok(n) => n,
      Err(_) => {
        thread::sleep(Duration::from_millis(4));
        continue;
      }
    };

    if n < 12 {
      continue;
    }

    let Some((x, y, z)) = scan_xyz(&buf[..n], &mut parse_base) else {
      continue;
    };

    if !primed {
      prev = (x, y, z);
      ema = (x, y, z);
      primed = true;
      continue;
    }

    let dx = x - prev.0;
    let dy = y - prev.1;
    let dz = z - prev.2;
    prev = (x, y, z);

    let impulse = (dx * dx + dy * dy + dz * dz).sqrt();

    ema.0 = ema.0 * (1.0 - ema_alpha) + x * ema_alpha;
    ema.1 = ema.1 * (1.0 - ema_alpha) + y * ema_alpha;
    ema.2 = ema.2 * (1.0 - ema_alpha) + z * ema_alpha;
    let deviation =
      ((x - ema.0).powi(2) + (y - ema.1).powi(2) + (z - ema.2).powi(2)).sqrt();

    // Sudden delta (slap) dominates; deviation is weighted lower so tiny drift does not fire.
    let score = impulse.max(deviation * 1.05);
    let strength = (score / (threshold * 1.5)).clamp(0.12, 3.0);

    if score >= threshold {
      if let Some(total) = state.try_consume_slap(strength) {
        let vol = state.slap_volume_percent();
        audio::play_slap(vol);
        emit_slap_event(&app, total, strength);
        update_tray_tooltip(&app, &state);
      }
    }
  }
}

#[cfg(target_os = "macos")]
pub fn spawn(app: AppHandle, state: Arc<AppState>) {
  thread::spawn(move || loop {
    if let Some(dev) = open_motion_device() {
      state.set_sensor_connected(true);
      update_tray_tooltip(&app, &state);
      run_sensor_loop(app.clone(), state.clone(), dev);
      state.set_sensor_connected(false);
      update_tray_tooltip(&app, &state);
    } else {
      state.set_sensor_connected(false);
      update_tray_tooltip(&app, &state);
    }
    thread::sleep(Duration::from_secs(2));
  });
}

#[cfg(not(target_os = "macos"))]
pub fn spawn(_app: AppHandle, state: Arc<AppState>) {
  state.set_sensor_connected(false);
}
