//! Where a voice comes from: its volume in each ear, from the listener's position and facing.

/// The player who listens: their eyes and where they face (Minecraft's yaw, degrees: 0 faces
/// south, +Z; 90 faces west, -X).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Listener {
    pub pos: [f64; 3],
    pub yaw: f32,
    pub dimension: &'static str,
}

/// How much quieter the far ear is for a voice straight to one side.
const SIDE_DIP: f32 = 0.8;

/// Left and right gains for a voice at `source` heard by `listener`, fading linearly to silence
/// at `range` blocks, as Simple Voice Chat's default distance fade does.
pub fn gains(listener: &Listener, source: [f64; 3], range: f32) -> (f32, f32) {
    let d = [
        source[0] - listener.pos[0],
        source[1] - listener.pos[1],
        source[2] - listener.pos[2],
    ];
    let dist = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() as f32;
    if range <= 0.0 || dist >= range {
        return (0.0, 0.0);
    }
    let volume = 1.0 - dist / range;
    let flat = (d[0] * d[0] + d[2] * d[2]).sqrt();
    let pan = if flat < 0.25 {
        0.0
    } else {
        // The listener's right hand: (-cos yaw, -sin yaw) in x and z.
        let yaw = f64::from(listener.yaw).to_radians();
        ((d[0] * -yaw.cos() + d[2] * -yaw.sin()) / flat) as f32
    };
    let left = 1.0 - pan.max(0.0) * SIDE_DIP;
    let right = 1.0 + pan.min(0.0) * SIDE_DIP;
    (volume * left, volume * right)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(yaw: f32) -> Listener {
        Listener {
            pos: [0.0, 64.0, 0.0],
            yaw,
            dimension: "minecraft:overworld",
        }
    }

    #[test]
    fn fades_with_distance() {
        let (l, r) = gains(&at(0.0), [0.0, 64.0, 24.0], 48.0);
        assert!((l - 0.5).abs() < 1e-4 && (r - 0.5).abs() < 1e-4);
        assert_eq!(gains(&at(0.0), [0.0, 64.0, 60.0], 48.0), (0.0, 0.0));
        let (l, r) = gains(&at(0.0), [0.0, 64.0, 0.0], 48.0);
        assert_eq!((l, r), (1.0, 1.0));
    }

    #[test]
    fn pans_to_the_side_the_voice_is_on() {
        // Facing south (+Z), east (+X) is on the left.
        let (l, r) = gains(&at(0.0), [10.0, 64.0, 0.0], 48.0);
        assert!(l > r * 3.0, "{l} {r}");
        // Facing west (-X), north (-Z) is on the right.
        let (l, r) = gains(&at(90.0), [0.0, 64.0, -10.0], 48.0);
        assert!(r > l * 3.0, "{l} {r}");
        // Straight ahead and behind are centred.
        let (l, r) = gains(&at(180.0), [0.0, 64.0, -10.0], 48.0);
        assert!((l - r).abs() < 1e-4);
    }
}
