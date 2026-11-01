//! Small signal helpers: the microphone's level, its gain, and a linear resampler between the
//! devices' rates and the voice rate.

/// A frame's loudness in dBFS (RMS); -127 for silence.
pub fn level_db(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return -127.0;
    }
    let mean = frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32;
    if mean <= 0.0 {
        return -127.0;
    }
    (10.0 * mean.log10()).max(-127.0)
}

/// Multiplies a frame by `gain`, clipped to full scale.
pub fn amplify(frame: &mut [f32], gain: f32) {
    if gain == 1.0 {
        return;
    }
    for s in frame {
        *s = (*s * gain).clamp(-1.0, 1.0);
    }
}

/// A streaming linear resampler: feed it blocks of any size at `from` Hz and it emits the same
/// signal at `to` Hz, carrying its position across blocks.
#[derive(Clone, Debug)]
pub struct Resampler {
    /// Input samples per output sample.
    step: f64,
    /// Where the next output falls between `prev` and the next input, 0 to 1.
    t: f64,
    prev: f32,
}

impl Resampler {
    pub fn new(from: u32, to: u32) -> Self {
        Self {
            step: f64::from(from) / f64::from(to.max(1)),
            t: 0.0,
            prev: 0.0,
        }
    }

    pub fn push(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if self.step == 1.0 {
            out.extend_from_slice(input);
            return;
        }
        for &x in input {
            while self.t <= 1.0 {
                out.push(self.prev + (x - self.prev) * self.t as f32);
                self.t += self.step;
            }
            self.t -= 1.0;
            self.prev = x;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels() {
        assert_eq!(level_db(&[0.0; 960]), -127.0);
        let full: Vec<f32> = (0..960).map(|i| if i % 2 == 0 { 1.0 } else { -1.0 }).collect();
        assert!(level_db(&full).abs() < 0.01);
        assert!((level_db(&[0.1; 960]) + 20.0).abs() < 0.01);
    }

    #[test]
    fn amplify_clips() {
        let mut f = [0.25, -0.5, 0.9];
        amplify(&mut f, 2.0);
        assert_eq!(f, [0.5, -1.0, 1.0]);
    }

    #[test]
    fn resampling_keeps_the_rate_ratio_across_blocks() {
        let mut r = Resampler::new(44_100, 48_000);
        let mut out = Vec::new();
        for _ in 0..100 {
            r.push(&[0.5; 441], &mut out);
        }
        // 44 100 in, about 48 000 out.
        assert!((out.len() as i64 - 48_000).abs() <= 2, "{}", out.len());
        assert!(out[100..].iter().all(|s| (s - 0.5).abs() < 1e-6));
        let mut down = Resampler::new(48_000, 16_000);
        let mut out = Vec::new();
        down.push(&[0.0; 4800], &mut out);
        assert!((out.len() as i64 - 1600).abs() <= 1);
    }
}
