//! macOS output volume: refcounted temporary override for slap playback.

use parking_lot::Mutex;

#[derive(Clone, Copy)]
struct SavedOutput {
  volume: i32,
  muted: bool,
}

struct Session {
  active: u32,
  saved: Option<SavedOutput>,
}

static SESSION: Mutex<Session> = Mutex::new(Session {
  active: 0,
  saved: None,
});

pub struct SlapVolumeGuard;

impl Drop for SlapVolumeGuard {
  fn drop(&mut self) {
    leave_session();
  }
}

/// Start of each slap playback thread: refcount++, snapshot volume on first active play,
/// apply `desired_percent` (0–100). Guard drop decrements; last one restores the snapshot.
pub fn begin_slap_volume_session(desired_percent: u8) -> SlapVolumeGuard {
  enter_session(desired_percent);
  SlapVolumeGuard
}

#[cfg(target_os = "macos")]
fn enter_session(desired_percent: u8) {
  let mut g = SESSION.lock();
  if g.active == 0 {
    g.saved = read_output_state().ok();
  }
  g.active += 1;
  drop(g);
  let _ = apply_output_state(desired_percent);
}

#[cfg(target_os = "macos")]
fn leave_session() {
  let to_restore = {
    let mut g = SESSION.lock();
    g.active = g.active.saturating_sub(1);
    if g.active == 0 {
      g.saved.take()
    } else {
      None
    }
  };
  if let Some(s) = to_restore {
    let _ = restore_output_state(s);
  }
}

#[cfg(not(target_os = "macos"))]
fn enter_session(_desired_percent: u8) {}

#[cfg(not(target_os = "macos"))]
fn leave_session() {}

#[cfg(target_os = "macos")]
fn read_output_state() -> Result<SavedOutput, String> {
  let out = std::process::Command::new("osascript")
    .args([
      "-e",
      "set vs to get volume settings",
      "-e",
      "return (output volume of vs as text) & \"|\" & (output muted of vs as text)",
    ])
    .output()
    .map_err(|e| e.to_string())?;
  if !out.status.success() {
    return Err(String::from_utf8_lossy(&out.stderr).to_string());
  }
  let text = String::from_utf8_lossy(&out.stdout);
  let line = text.trim();
  let (vol_s, mute_s) = line
    .split_once('|')
    .ok_or_else(|| format!("unexpected osascript output: {line}"))?;
  let volume: i32 = vol_s
    .trim()
    .parse()
    .map_err(|_| format!("bad volume: {vol_s}"))?;
  let muted = mute_s.trim().eq_ignore_ascii_case("true");
  Ok(SavedOutput { volume, muted })
}

#[cfg(target_os = "macos")]
fn apply_output_state(desired_percent: u8) -> Result<(), String> {
  let v = (desired_percent as i32).clamp(0, 100);
  if v > 0 {
    run_osascript_two(
      &format!("set volume output volume {}", v),
      "set volume without output muted",
    )
  } else {
    run_osascript_two("set volume output volume 0", "set volume with output muted")
  }
}

#[cfg(target_os = "macos")]
fn restore_output_state(s: SavedOutput) -> Result<(), String> {
  let v = s.volume.clamp(0, 100);
  let first = format!("set volume output volume {}", v);
  if s.muted {
    run_osascript_two(&first, "set volume with output muted")
  } else {
    run_osascript_two(&first, "set volume without output muted")
  }
}

#[cfg(target_os = "macos")]
fn run_osascript_two(first: &str, second: &str) -> Result<(), String> {
  let out = std::process::Command::new("osascript")
    .arg("-e")
    .arg(first)
    .arg("-e")
    .arg(second)
    .output()
    .map_err(|e| e.to_string())?;
  if out.status.success() {
    Ok(())
  } else {
    Err(String::from_utf8_lossy(&out.stderr).to_string())
  }
}
