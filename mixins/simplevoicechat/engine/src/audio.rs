//! The sound devices, through cpal: the microphone fills a queue of 48 kHz mono samples, and
//! the speakers play a mix of every voice heard, each from its own jitter buffer.
//!
//! cpal runs both callbacks on threads of its own; the game thread reaches them through the two
//! mutexes. A stream can't leave the thread that made it on every platform, so `AudioIo` stays
//! in the client half's thread local.
use crate::{FRAME, SAMPLE_RATE, dsp::Resampler};
use cpal::{
    Device, FromSample, SampleFormat, SizedSample, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, MutexGuard},
};

/// A voice starts playing once this much of it waited (60 ms), so late frames don't gap.
const START: usize = 3 * FRAME;
/// More than this waiting (half a second) and the oldest goes: a voice never lags behind.
const MAX_BUFFER: usize = 25 * FRAME;
/// Recorded samples kept at most before the game takes them (200 ms).
const MAX_MIC: usize = 10 * FRAME;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Default)]
struct Voice {
    buf: VecDeque<[f32; 2]>,
    playing: bool,
}

/// Every voice heard, mixed into one stereo signal at 48 kHz.
#[derive(Default)]
pub struct Mixer {
    voices: HashMap<u128, Voice>,
}

impl Mixer {
    /// Queues `pcm` (48 kHz mono) from voice `id`, at `left` and `right` gain.
    pub fn push(&mut self, id: u128, pcm: &[f32], left: f32, right: f32) {
        let v = self.voices.entry(id).or_default();
        v.buf.extend(pcm.iter().map(|&s| [s * left, s * right]));
        let over = v.buf.len().saturating_sub(MAX_BUFFER);
        v.buf.drain(..over);
    }

    /// The next stereo sample of the mix.
    pub fn next(&mut self) -> [f32; 2] {
        let mut out = [0.0f32; 2];
        for v in self.voices.values_mut() {
            if !v.playing && v.buf.len() >= START {
                v.playing = true;
            }
            if !v.playing {
                continue;
            }
            match v.buf.pop_front() {
                Some([l, r]) => {
                    out[0] += l;
                    out[1] += r;
                }
                // Ran dry: wait for a buffer's worth again.
                None => v.playing = false,
            }
        }
        out.map(|s| s.clamp(-1.0, 1.0))
    }

    pub fn clear(&mut self) {
        self.voices.clear();
    }
}

fn named(devices: impl Iterator<Item = Device>, name: &str) -> Option<Device> {
    devices
        .into_iter()
        .find(|d| d.name().is_ok_and(|n| n == name))
}

/// The microphones' names.
pub fn input_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.input_devices()
        .map(|ds| ds.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default()
}

/// The speakers' names.
pub fn output_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.output_devices()
        .map(|ds| ds.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default()
}

/// The open devices.
pub struct AudioIo {
    _input: Option<Stream>,
    _output: Option<Stream>,
    mic: Arc<Mutex<VecDeque<f32>>>,
    mixer: Arc<Mutex<Mixer>>,
    /// Why there is no microphone or speaker, for the HUD.
    pub problem: Option<String>,
}

impl AudioIo {
    /// Opens the named microphone and speakers, else the system's defaults. A missing device
    /// leaves that side silent, with the reason in `problem`.
    pub fn open(input: Option<&str>, output: Option<&str>) -> Self {
        let host = cpal::default_host();
        let mic = Arc::new(Mutex::new(VecDeque::new()));
        let mixer = Arc::new(Mutex::new(Mixer::default()));
        let mut problems = Vec::new();

        let out_device = output
            .and_then(|n| named(host.output_devices().ok()?, n))
            .or_else(|| host.default_output_device());
        let out_stream = match out_device {
            Some(d) => open_output(&d, mixer.clone()),
            None => Err("no speakers".into()),
        };
        let out_stream = out_stream.map_err(|e| problems.push(e)).ok();

        let in_device = input
            .and_then(|n| named(host.input_devices().ok()?, n))
            .or_else(|| host.default_input_device());
        let in_stream = match in_device {
            Some(d) => open_input(&d, mic.clone()),
            None => Err("no microphone".into()),
        };
        let in_stream = in_stream.map_err(|e| problems.push(e)).ok();

        Self {
            _input: in_stream,
            _output: out_stream,
            mic,
            mixer,
            problem: (!problems.is_empty()).then(|| problems.join(", ")),
        }
    }

    pub fn has_mic(&self) -> bool {
        self._input.is_some()
    }

    /// The whole frames recorded since the last call, oldest first.
    pub fn take_frames(&self) -> Vec<Vec<f32>> {
        let mut mic = lock(&self.mic);
        let mut frames = Vec::new();
        while mic.len() >= FRAME {
            frames.push(mic.drain(..FRAME).collect());
        }
        frames
    }

    /// Plays `pcm` (48 kHz mono) as part of voice `id`.
    pub fn play(&self, id: u128, pcm: &[f32], left: f32, right: f32) {
        lock(&self.mixer).push(id, pcm, left, right);
    }

    pub fn silence(&self) {
        lock(&self.mixer).clear();
    }
}

fn open_output(device: &Device, mixer: Arc<Mutex<Mixer>>) -> Result<Stream, String> {
    let supported = device
        .default_output_config()
        .map_err(|e| format!("speakers: {e}"))?;
    let config = supported.config();
    let stream = match supported.sample_format() {
        SampleFormat::F32 => output_stream::<f32>(device, &config, mixer),
        SampleFormat::I16 => output_stream::<i16>(device, &config, mixer),
        SampleFormat::U16 => output_stream::<u16>(device, &config, mixer),
        SampleFormat::I32 => output_stream::<i32>(device, &config, mixer),
        f => return Err(format!("speakers: sample format {f} isn't supported")),
    }
    .map_err(|e| format!("speakers: {e}"))?;
    stream.play().map_err(|e| format!("speakers: {e}"))?;
    Ok(stream)
}

fn output_stream<T>(
    device: &Device,
    config: &StreamConfig,
    mixer: Arc<Mutex<Mixer>>,
) -> Result<Stream, cpal::BuildStreamError>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = usize::from(config.channels.max(1));
    // Mix samples (48 kHz) per device sample, interpolated between `prev` and `next`.
    let step = f64::from(SAMPLE_RATE) / f64::from(config.sample_rate.0.max(1));
    let (mut t, mut prev, mut next) = (0.0f64, [0.0f32; 2], [0.0f32; 2]);
    device.build_output_stream(
        config,
        move |data: &mut [T], _| {
            let mut mixer = lock(&mixer);
            for frame in data.chunks_mut(channels) {
                let k = t as f32;
                let s = [
                    prev[0] + (next[0] - prev[0]) * k,
                    prev[1] + (next[1] - prev[1]) * k,
                ];
                t += step;
                while t >= 1.0 {
                    t -= 1.0;
                    prev = next;
                    next = mixer.next();
                }
                if channels == 1 {
                    frame[0] = T::from_sample((s[0] + s[1]) * 0.5);
                } else {
                    frame[0] = T::from_sample(s[0]);
                    frame[1] = T::from_sample(s[1]);
                    for c in &mut frame[2..] {
                        *c = T::from_sample(0.0f32);
                    }
                }
            }
        },
        |e| eprintln!("[simplevoicechat] speakers: {e}"),
        None,
    )
}

fn open_input(device: &Device, mic: Arc<Mutex<VecDeque<f32>>>) -> Result<Stream, String> {
    let supported = device
        .default_input_config()
        .map_err(|e| format!("microphone: {e}"))?;
    let config = supported.config();
    let stream = match supported.sample_format() {
        SampleFormat::F32 => input_stream::<f32>(device, &config, mic),
        SampleFormat::I16 => input_stream::<i16>(device, &config, mic),
        SampleFormat::U16 => input_stream::<u16>(device, &config, mic),
        SampleFormat::I32 => input_stream::<i32>(device, &config, mic),
        f => return Err(format!("microphone: sample format {f} isn't supported")),
    }
    .map_err(|e| format!("microphone: {e}"))?;
    stream.play().map_err(|e| format!("microphone: {e}"))?;
    Ok(stream)
}

fn input_stream<T>(
    device: &Device,
    config: &StreamConfig,
    mic: Arc<Mutex<VecDeque<f32>>>,
) -> Result<Stream, cpal::BuildStreamError>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = usize::from(config.channels.max(1));
    let mut resampler = Resampler::new(config.sample_rate.0, SAMPLE_RATE);
    let (mut mono, mut out) = (Vec::new(), Vec::new());
    device.build_input_stream(
        config,
        move |data: &[T], _| {
            mono.clear();
            out.clear();
            mono.extend(data.chunks(channels).map(|f| {
                f.iter().map(|s| s.to_sample::<f32>()).sum::<f32>() / f.len() as f32
            }));
            resampler.push(&mono, &mut out);
            let mut mic = lock(&mic);
            mic.extend(out.iter().copied());
            let over = mic.len().saturating_sub(MAX_MIC);
            mic.drain(..over);
        },
        |e| eprintln!("[simplevoicechat] microphone: {e}"),
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_voice_waits_for_its_buffer_then_plays_and_rebuffers() {
        let mut m = Mixer::default();
        m.push(1, &[0.5; FRAME], 1.0, 0.5);
        assert_eq!(m.next(), [0.0, 0.0], "one frame isn't enough to start");
        m.push(1, &[0.5; 2 * FRAME], 1.0, 0.5);
        assert_eq!(m.next(), [0.5, 0.25]);
        for _ in 1..3 * FRAME {
            m.next();
        }
        assert_eq!(m.next(), [0.0, 0.0], "dry");
        m.push(1, &[0.5; FRAME], 1.0, 1.0);
        assert_eq!(m.next(), [0.0, 0.0], "waits again");
    }

    #[test]
    fn voices_mix_and_clip_and_never_lag() {
        let mut m = Mixer::default();
        m.push(1, &[0.75; 3 * FRAME], 1.0, 1.0);
        m.push(2, &[0.75; 3 * FRAME], 1.0, 0.0);
        assert_eq!(m.next(), [1.0, 0.75]);
        m.push(3, &vec![0.1; 40 * FRAME], 1.0, 1.0);
        assert_eq!(m.voices[&3].buf.len(), MAX_BUFFER);
    }
}
