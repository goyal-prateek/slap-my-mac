//! ISOBMFF / MP4 / M4A decode for import. Rodio’s symphonia wrapper uses `enable_gapless: true`,
//! which can hit `SeekError` on iPhone recordings and **panic**. We probe with gapless off instead.

use std::fs::File;
use std::io::{Cursor, Read};
use std::path::Path;
use std::time::Duration;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units;

/// Decode the first audio track to interleaved `i16` PCM, trimmed to at most `max` wall-clock duration.
pub fn decode_m4a_like_to_i16(src: &Path, max: Duration) -> Result<(u32, u16, Vec<i16>), String> {
  let mut file = File::open(src).map_err(|e| e.to_string())?;
  let mut buf = Vec::new();
  file.read_to_end(&mut buf).map_err(|e| e.to_string())?;
  if buf.is_empty() {
    return Err("File is empty.".to_string());
  }

  let ext = src
    .extension()
    .and_then(|e| e.to_str())
    .unwrap_or("m4a");
  let cursor = Cursor::new(buf);
  let mss = MediaSourceStream::new(Box::new(cursor), Default::default());

  let mut hint = Hint::new();
  hint.with_extension(ext);

  let fmt_opts = FormatOptions {
    enable_gapless: false,
    ..Default::default()
  };
  let meta_opts = MetadataOptions::default();

  let mut probed = symphonia::default::get_probe()
    .format(&hint, mss, &fmt_opts, &meta_opts)
    .map_err(|e| format!("Could not read container: {e}"))?;

  let format = &mut probed.format;

  let track = format
    .tracks()
    .iter()
    .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
    .ok_or_else(|| "No supported audio track in this file.".to_string())?;

  let track_id = track.id;
  let dec_opts = DecoderOptions::default();
  let mut decoder = symphonia::default::get_codecs()
    .make(&track.codec_params, &dec_opts)
    .map_err(|e| format!("Unsupported codec: {e}"))?;

  let mut out: Vec<i16> = Vec::new();
  let mut sample_rate: u32 = 0;
  let mut channels: u16 = 0;
  let mut max_interleaved: usize = 0;

  loop {
    let packet = match format.next_packet() {
      Ok(p) => p,
      Err(Error::IoError(_)) => break,
      Err(Error::ResetRequired) => {
        return Err("This file uses chained streams; try exporting to M4A or WAV in Voice Memos."
          .to_string());
      }
      Err(e) => return Err(format!("Read error: {e}")),
    };

    if packet.track_id() != track_id {
      continue;
    }

    match decoder.decode(&packet) {
      Ok(decoded) => {
        let spec = *decoded.spec();
        if sample_rate == 0 {
          sample_rate = spec.rate;
          let ch = spec.channels.count();
          if ch == 0 || ch > 8 {
            return Err("Unsupported channel layout.".to_string());
          }
          channels = ch as u16;
          let cap = (max.as_secs_f64() * sample_rate as f64 * channels as f64).ceil() as usize;
          max_interleaved = cap.max(1);
        }

        let duration = units::Duration::from(decoded.capacity() as u64);
        let mut sample_buf = SampleBuffer::<i16>::new(duration, spec);
        sample_buf.copy_interleaved_ref(decoded);
        let s = sample_buf.samples();

        if out.len() >= max_interleaved {
          break;
        }
        let room = max_interleaved - out.len();
        let take = room.min(s.len());
        out.extend_from_slice(&s[..take]);
      }
      Err(Error::IoError(_)) | Err(Error::DecodeError(_)) => continue,
      Err(e) => return Err(format!("Decode error: {e}")),
    }

    if sample_rate > 0 && out.len() >= max_interleaved {
      break;
    }
  }

  if out.is_empty() || sample_rate == 0 {
    return Err("No audio samples could be read.".to_string());
  }

  Ok((sample_rate, channels, out))
}
