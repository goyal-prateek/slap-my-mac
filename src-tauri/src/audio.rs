use hound::{SampleFormat, WavSpec, WavWriter};
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use serde::Serialize;
use std::fs::File;
use std::io::{Cursor, Read};
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

/// Hard cap on stored / played slap clip length.
pub const MAX_SLAP_SOUND_SECS: u32 = 3;

/// Bundled default slap (MP3). Canonical copy lives under `src-tauri/resources/sounds/`.
static DEFAULT_SLAP_MP3: &[u8] = include_bytes!("../resources/sounds/default-slap.mp3");

static OUTPUT: OnceLock<OutputStreamHandle> = OnceLock::new();
static CUSTOM_SLAP_PATH: OnceLock<std::path::PathBuf> = OnceLock::new();

pub fn set_custom_sound_path(path: std::path::PathBuf) {
  let _ = CUSTOM_SLAP_PATH.set(path);
}

pub fn try_init() {
  if OUTPUT.get().is_some() {
    return;
  }
  if let Ok((stream, handle)) = OutputStream::try_default() {
    std::mem::forget(stream);
    let _ = OUTPUT.set(handle);
  }
}

/// `output_volume_percent` is the macOS output level (0–100) to apply for the duration of this play.
pub fn play_slap(output_volume_percent: u8) {
  let Some(handle) = OUTPUT.get().cloned() else {
    return;
  };
  let max = Duration::from_secs(u64::from(MAX_SLAP_SOUND_SECS));

  if let Some(p) = CUSTOM_SLAP_PATH.get() {
    if p.is_file() {
      if let Ok(bytes) = std::fs::read(p) {
        if !bytes.is_empty() {
          std::thread::spawn(move || {
            let _vol = crate::system_volume::begin_slap_volume_session(output_volume_percent);
            let cursor = Cursor::new(bytes);
            let Ok(decoder) = Decoder::new(cursor) else {
              return;
            };
            let Ok(sink) = Sink::try_new(&handle) else {
              return;
            };
            sink.append(decoder);
            sink.sleep_until_end();
          });
          return;
        }
      }
    }
  }

  std::thread::spawn(move || {
    let _vol = crate::system_volume::begin_slap_volume_session(output_volume_percent);
    let cursor = Cursor::new(DEFAULT_SLAP_MP3.to_vec());
    let Ok(decoder) = Decoder::new(cursor) else {
      return;
    };
    let trimmed = decoder.take_duration(max);
    let Ok(sink) = Sink::try_new(&handle) else {
      return;
    };
    sink.append(trimmed);
    sink.sleep_until_end();
  });
}

const MAX_SOURCE_FILE_BYTES: u64 = 80 * 1024 * 1024;

/// Decode with rodio (most formats) or symphonia (M4A/MP4 without rodio’s gapless path), trim to
/// `MAX_SLAP_SOUND_SECS`, write 16-bit PCM WAV to `dest`.
pub fn import_trim_save(src: &Path, dest: &Path) -> Result<SoundImportInfo, String> {
  let meta = std::fs::metadata(src).map_err(|e| e.to_string())?;
  if meta.len() > MAX_SOURCE_FILE_BYTES {
    return Err(format!(
      "File is too large (max {} MB before processing).",
      MAX_SOURCE_FILE_BYTES / (1024 * 1024)
    ));
  }

  let max = Duration::from_secs(u64::from(MAX_SLAP_SOUND_SECS));
  let ext = src
    .extension()
    .and_then(|e| e.to_str())
    .map(|s| s.to_ascii_lowercase());

  let (sample_rate, channels, samples, was_trimmed) = match ext.as_deref() {
    Some("m4a" | "mp4" | "m4b" | "mov") => {
      let (sr, ch, samp) = crate::isomp4_decode::decode_m4a_like_to_i16(src, max)?;
      let frames = samp.len() as f64 / f64::from(ch);
      let secs = frames / f64::from(sr);
      let trimmed = secs >= (MAX_SLAP_SOUND_SECS as f64) - 0.05;
      (sr, ch, samp, trimmed)
    }
    _ => {
      let mut file = File::open(src).map_err(|e| e.to_string())?;
      let mut buf = Vec::new();
      file.read_to_end(&mut buf).map_err(|e| e.to_string())?;
      if buf.is_empty() {
        return Err("File is empty.".to_string());
      }
      let decoder = Decoder::new(Cursor::new(buf)).map_err(|_| {
        "Could not decode this file. Try WAV, MP3, FLAC, Ogg, M4A, or AAC.".to_string()
      })?;

      let sample_rate = decoder.sample_rate();
      let channels = decoder.channels() as u16;
      if sample_rate == 0 || channels == 0 || channels > 8 {
        return Err("Unsupported channel or sample rate.".to_string());
      }

      let total_dur = decoder.total_duration();
      let trimmed_dec = decoder.take_duration(max);
      let samples: Vec<i16> = trimmed_dec.convert_samples::<i16>().collect();

      let frames = samples.len() as u64 / channels as u64;
      let out_secs = frames as f64 / f64::from(sample_rate);

      let was_trimmed = match total_dur {
        Some(d) => d > max,
        None => out_secs >= (MAX_SLAP_SOUND_SECS as f64) - 0.08,
      };
      (sample_rate, channels, samples, was_trimmed)
    }
  };

  if samples.is_empty() {
    return Err("No audio samples could be read.".to_string());
  }
  if sample_rate == 0 || channels == 0 || channels > 8 {
    return Err("Unsupported channel or sample rate.".to_string());
  }

  let frames = samples.len() as u64 / channels as u64;
  let out_secs = frames as f64 / f64::from(sample_rate);

  let spec = WavSpec {
    channels,
    sample_rate,
    bits_per_sample: 16,
    sample_format: SampleFormat::Int,
  };

  if let Some(parent) = dest.parent() {
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
  }

  let mut out = Cursor::new(Vec::new());
  {
    let mut writer = WavWriter::new(&mut out, spec).map_err(|e| e.to_string())?;
    for s in samples {
      writer.write_sample(s).map_err(|e| e.to_string())?;
    }
    writer.finalize().map_err(|e| e.to_string())?;
  }

  std::fs::write(dest, out.into_inner()).map_err(|e| e.to_string())?;

  Ok(SoundImportInfo {
    duration_seconds: out_secs,
    was_trimmed,
    max_seconds: MAX_SLAP_SOUND_SECS,
  })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoundImportInfo {
  pub duration_seconds: f64,
  pub was_trimmed: bool,
  pub max_seconds: u32,
}
