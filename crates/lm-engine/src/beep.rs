//! The microphone warning tone: two short notes, falling when the mics are
//! muted, rising when they go live again.
//!
//! Each beep is its own short-lived playback stream aimed at the hardware
//! output (never through the mixer, so it cannot leak into a virtual mic);
//! the engine drops it once it has played.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use libspa::pod::Pod;
use pipewire::core::CoreRc;
use pipewire::properties::properties;
use pipewire::stream::{StreamFlags, StreamListener, StreamRc};

const RATE: u32 = 48_000;
const NOTE_MS: u32 = 90;
const GAP_MS: u32 = 25;
const FADE_MS: u32 = 6;
/// About -14 dBFS: noticeable, not startling.
const AMPLITUDE: f32 = 0.2;
const LOW_HZ: f32 = 587.33; // D5
const HIGH_HZ: f32 = 880.0; // A5
/// Silence before the notes: the first buffers after a device wakes from
/// suspend are often lost.
const LEAD_MS: u32 = 60;
/// Silence after them, so the stream is not dropped while the last notes are
/// still in the device's buffer.
const TAIL_MS: u32 = 400;
/// A beep that never gets to play (no device, stuck link) is dropped after this.
const GIVE_UP: Duration = Duration::from_secs(6);

/// Mono samples of the beep at `rate`: high→low for mute, low→high for live.
pub fn samples(rising: bool, rate: u32) -> Vec<f32> {
    let (first, second) = if rising { (LOW_HZ, HIGH_HZ) } else { (HIGH_HZ, LOW_HZ) };
    let note = (rate * NOTE_MS / 1000) as usize;
    let gap = (rate * GAP_MS / 1000) as usize;
    let fade = (rate * FADE_MS / 1000) as usize;
    let mut out = Vec::with_capacity(2 * note + gap);
    for (i, freq) in [first, second].into_iter().enumerate() {
        if i == 1 {
            out.extend(std::iter::repeat_n(0.0, gap));
        }
        for n in 0..note {
            // Short linear fades keep the edges from clicking.
            let env = (n.min(note - 1 - n) as f32 / fade as f32).min(1.0);
            let phase = 2.0 * std::f32::consts::PI * freq * n as f32 / rate as f32;
            out.push(AMPLITUDE * env * phase.sin());
        }
    }
    out
}

/// One playing beep. Keep it until `is_done`: it has to wait for the link
/// and for the device to wake up, which on real hardware (Bluetooth above
/// all) can take a second or more — a fixed lifetime cut it off unheard.
pub struct Beep {
    started: Instant,
    /// Frames the process callback has written so far (lead + notes + tail).
    played: Arc<AtomicUsize>,
    total: usize,
    _stream: StreamRc,
    _listener: StreamListener<usize>,
}

impl Beep {
    /// Play to `target` (a hardware sink's node.name), or wherever the session
    /// manager sends notification sounds when None.
    pub fn play(core: CoreRc, target: Option<&str>, rising: bool) -> Result<Self, pipewire::Error> {
        let mut props = properties! {
            *pipewire::keys::MEDIA_TYPE => "Audio",
            *pipewire::keys::MEDIA_CATEGORY => "Playback",
            *pipewire::keys::NODE_NAME => "lm.beep",
            *pipewire::keys::NODE_DESCRIPTION => "linuxmeeter microphone tone",
            *pipewire::keys::APP_NAME => "linuxmeeter",
            "node.dont-reconnect" => "true",
        };
        if let Some(t) = target {
            props.insert("target.object", t);
        }
        let stream = StreamRc::new(core, "lm.beep", props)?;
        let lead = (RATE * LEAD_MS / 1000) as usize;
        let tail = (RATE * TAIL_MS / 1000) as usize;
        let mut wave = vec![0.0; lead];
        wave.extend(samples(rising, RATE));
        let total = wave.len() + tail;
        let played = Arc::new(AtomicUsize::new(0));
        let played_cb = played.clone();

        // user data = read position in `wave`; after the end, silence.
        let listener = stream
            .add_local_listener_with_user_data::<usize>(0)
            .process(move |stream, pos| {
                let Some(mut buffer) = stream.dequeue_buffer() else { return };
                let datas = buffer.datas_mut();
                let Some(data) = datas.first_mut() else { return };
                const STRIDE: usize = 8; // 2 channels * f32
                let Some(slice) = data.data() else { return };
                let frames = slice.len() / STRIDE;
                for f in 0..frames {
                    let v = wave.get(*pos).copied().unwrap_or(0.0);
                    *pos += 1;
                    let bytes = v.to_ne_bytes();
                    slice[f * STRIDE..f * STRIDE + 4].copy_from_slice(&bytes);
                    slice[f * STRIDE + 4..f * STRIDE + 8].copy_from_slice(&bytes);
                }
                played_cb.store(*pos, Ordering::Relaxed);
                let chunk = data.chunk_mut();
                *chunk.offset_mut() = 0;
                *chunk.stride_mut() = STRIDE as i32;
                *chunk.size_mut() = (frames * STRIDE) as u32;
            })
            .register()?;

        let mut info = libspa::param::audio::AudioInfoRaw::new();
        info.set_format(libspa::param::audio::AudioFormat::F32LE);
        info.set_rate(RATE);
        info.set_channels(2);
        let mut position = [0u32; libspa::sys::SPA_AUDIO_MAX_CHANNELS as usize];
        position[0] = libspa::sys::SPA_AUDIO_CHANNEL_FL;
        position[1] = libspa::sys::SPA_AUDIO_CHANNEL_FR;
        info.set_position(position);
        let obj = libspa::pod::Object {
            type_: libspa::utils::SpaTypes::ObjectParamFormat.as_raw(),
            id: libspa::param::ParamType::EnumFormat.as_raw(),
            properties: info.into(),
        };
        let bytes = libspa::pod::serialize::PodSerializer::serialize(
            std::io::Cursor::new(Vec::new()),
            &libspa::pod::Value::Object(obj),
        )
        .expect("serialize format pod")
        .0
        .into_inner();
        let mut params = [Pod::from_bytes(&bytes).expect("valid format pod")];
        stream.connect(
            libspa::utils::Direction::Output,
            None,
            StreamFlags::AUTOCONNECT | StreamFlags::MAP_BUFFERS,
            &mut params,
        )?;

        Ok(Self { started: Instant::now(), played, total, _stream: stream, _listener: listener })
    }

    /// Played to the end (with its tail), or given up on.
    pub fn is_done(&self, now: Instant) -> bool {
        let played = self.played.load(Ordering::Relaxed);
        if played >= self.total {
            tracing::debug!("warning tone played");
            return true;
        }
        if now.duration_since(self.started) > GIVE_UP {
            tracing::warn!(
                "warning tone never reached a device ({played} of {} frames written); is the output device available?",
                self.total
            );
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dominant_hz(s: &[f32], rate: u32) -> f32 {
        // Zero crossings per second / 2.
        let crossings = s.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count();
        crossings as f32 * rate as f32 / s.len() as f32 / 2.0
    }

    #[test]
    fn mute_falls_and_unmute_rises() {
        let note = (RATE * NOTE_MS / 1000) as usize;
        let gap = (RATE * GAP_MS / 1000) as usize;
        let down = samples(false, RATE);
        let up = samples(true, RATE);
        assert_eq!(down.len(), 2 * note + gap);
        assert!(dominant_hz(&down[..note], RATE) > dominant_hz(&down[note + gap..], RATE));
        assert!(dominant_hz(&up[..note], RATE) < dominant_hz(&up[note + gap..], RATE));
    }

    #[test]
    fn stays_quiet_and_starts_and_ends_at_zero() {
        let s = samples(true, RATE);
        assert!(s.iter().all(|v| v.abs() <= AMPLITUDE + 1e-6));
        assert_eq!(s[0], 0.0);
        assert!(s.last().unwrap().abs() < 0.01);
    }
}
