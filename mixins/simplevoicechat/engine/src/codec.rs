//! The voice codec: 4-bit IMA ADPCM at 24 kHz, 243 bytes a 20 ms frame (about 12 KB/s).
//!
//! Simple Voice Chat uses Opus, which Horizon's Cargo.lock doesn't carry (a native mod may only
//! use crates already in it), so frames are ADPCM instead: cheap, robust, and clear enough for
//! speech. Each frame carries its own predictor and step index, so a lost frame costs only itself.
use crate::FRAME;

/// Samples a frame holds after halving the rate.
const HALF: usize = FRAME / 2;
/// Bytes of one encoded frame: predictor (i16), step index (u8), then two samples a byte.
pub const ENCODED_LEN: usize = 3 + HALF / 2;

const INDEX_TABLE: [i32; 16] = [-1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8];
const STEP_TABLE: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66,
    73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408,
    449, 494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066,
    2272, 2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630,
    9493, 10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794,
    32767,
];

fn to_i16(s: f32) -> i32 {
    (s.clamp(-1.0, 1.0) * 32767.0).round() as i32
}

/// One sample's step: the new predictor and index from a 4-bit code.
fn step(code: u8, pred: i32, index: i32) -> (i32, i32) {
    let step = STEP_TABLE[index as usize];
    let mut diff = step >> 3;
    if code & 4 != 0 {
        diff += step;
    }
    if code & 2 != 0 {
        diff += step >> 1;
    }
    if code & 1 != 0 {
        diff += step >> 2;
    }
    let pred = if code & 8 != 0 { pred - diff } else { pred + diff };
    let index = (index + INDEX_TABLE[code as usize]).clamp(0, 88);
    (pred.clamp(-32768, 32767), index)
}

/// Encodes one frame of `FRAME` samples at 48 kHz.
pub fn encode(frame: &[f32]) -> Vec<u8> {
    debug_assert_eq!(frame.len(), FRAME);
    // Halve the rate, averaging pairs: a gentle low pass on the way down.
    let pcm: Vec<i32> = frame
        .chunks_exact(2)
        .map(|p| to_i16((p[0] + p[1]) * 0.5))
        .collect();
    let mut pred = pcm[0];
    // Start with a step that fits the frame's opening slope, so loud frames don't ramp up.
    let slope = pcm.windows(2).take(8).map(|w| (w[1] - w[0]).abs()).sum::<i32>() / 8;
    let mut index = STEP_TABLE.iter().position(|&s| s >= slope).unwrap_or(88) as i32;
    let mut out = Vec::with_capacity(ENCODED_LEN);
    out.extend_from_slice(&(pred as i16).to_le_bytes());
    out.push(index as u8);
    let mut byte = 0u8;
    for (i, &s) in pcm.iter().enumerate() {
        let step_size = STEP_TABLE[index as usize];
        let mut diff = s - pred;
        let mut code = 0u8;
        if diff < 0 {
            code = 8;
            diff = -diff;
        }
        let mut threshold = step_size;
        for bit in [4u8, 2, 1] {
            if diff >= threshold {
                code |= bit;
                diff -= threshold;
            }
            threshold >>= 1;
        }
        (pred, index) = self::step(code, pred, index);
        if i % 2 == 0 {
            byte = code;
        } else {
            out.push(byte | code << 4);
        }
    }
    out
}

/// Decodes one frame back to `FRAME` samples at 48 kHz; None for bytes that aren't a frame.
pub fn decode(bytes: &[u8]) -> Option<Vec<f32>> {
    if bytes.len() != ENCODED_LEN || bytes[2] > 88 {
        return None;
    }
    let mut pred = i32::from(i16::from_le_bytes([bytes[0], bytes[1]]));
    let mut index = i32::from(bytes[2]);
    let mut pcm = Vec::with_capacity(HALF);
    for &b in &bytes[3..] {
        for code in [b & 0x0F, b >> 4] {
            (pred, index) = step(code, pred, index);
            pcm.push(pred as f32 / 32768.0);
        }
    }
    // Back to 48 kHz: each sample, then the midpoint to the next.
    let mut out = Vec::with_capacity(FRAME);
    for i in 0..HALF {
        let a = pcm[i];
        let b = pcm.get(i + 1).copied().unwrap_or(a);
        out.push(a);
        out.push((a + b) * 0.5);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(freq: f32, amp: f32) -> Vec<f32> {
        (0..FRAME)
            .map(|i| amp * (i as f32 * freq * std::f32::consts::TAU / 48_000.0).sin())
            .collect()
    }

    #[test]
    fn a_frame_has_a_fixed_size() {
        assert_eq!(encode(&tone(440.0, 0.5)).len(), ENCODED_LEN);
        assert_eq!(ENCODED_LEN, 243);
    }

    #[test]
    fn speech_band_tones_survive_the_round_trip() {
        // The upper band loses more to the 24 kHz round trip's interpolation.
        for (freq, floor) in [(200.0, 15.0), (440.0, 15.0), (1000.0, 15.0), (3000.0, 10.0)] {
            let input = tone(freq, 0.5);
            let output = decode(&encode(&input)).unwrap();
            assert_eq!(output.len(), FRAME);
            // Skip the first samples while the predictor settles.
            let err: f32 = input[40..]
                .iter()
                .zip(&output[40..])
                .map(|(a, b)| (a - b).powi(2))
                .sum::<f32>()
                / (FRAME - 40) as f32;
            let signal: f32 = input[40..].iter().map(|a| a * a).sum::<f32>() / (FRAME - 40) as f32;
            let snr = 10.0 * (signal / err).log10();
            assert!(snr > floor, "{freq} Hz: SNR {snr:.1} dB");
        }
    }

    #[test]
    fn silence_stays_silent_and_garbage_is_refused() {
        let out = decode(&encode(&[0.0; FRAME])).unwrap();
        assert!(out.iter().all(|s| s.abs() < 0.001));
        assert!(decode(&[0; 10]).is_none());
        let mut bad = encode(&[0.0; FRAME]);
        bad[2] = 200;
        assert!(decode(&bad).is_none());
    }
}
