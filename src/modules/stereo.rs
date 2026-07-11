//! Mid/side stereo utilities (Q150).
//!
//! [`MidSideEncode`] converts a left/right pair to mid/side; [`MidSideDecode`]
//! converts mid/side back to left/right with an adjustable stereo width. Encoding
//! then decoding at width 1.0 is an exact identity.
//!
//! Conventions: `M = (L + R) * 0.5`, `S = (L - R) * 0.5`; decode
//! `L = M + S*width`, `R = M - S*width`. Width 0 collapses to mono
//! (`L == R == M`); a mono source (`L == R`) encodes to `S == 0`.

use crate::port::{GraphModule, PortDef, PortSpec, PortValues, SignalKind};
use alloc::vec;

/// Encode a left/right stereo pair to mid/side.
pub struct MidSideEncode {
    spec: PortSpec,
}

impl MidSideEncode {
    pub fn new() -> Self {
        Self {
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "left", SignalKind::Audio),
                    PortDef::new(1, "right", SignalKind::Audio),
                ],
                outputs: vec![
                    PortDef::new(10, "mid", SignalKind::Audio),
                    PortDef::new(11, "side", SignalKind::Audio),
                ],
            },
        }
    }
}

impl Default for MidSideEncode {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphModule for MidSideEncode {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let left = inputs.get_or(0, 0.0);
        let right = inputs.get_or(1, 0.0);
        outputs.set(10, (left + right) * 0.5);
        outputs.set(11, (left - right) * 0.5);
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "mid_side_encode"
    }
}

/// Decode a mid/side pair back to left/right, scaling the side signal by `width`.
///
/// `width` is an input port (default 1.0, clamped to `0..2`): 0 = mono, 1 = the
/// exact inverse of [`MidSideEncode`], 2 = doubled stereo width.
pub struct MidSideDecode {
    spec: PortSpec,
}

impl MidSideDecode {
    pub fn new() -> Self {
        Self {
            spec: PortSpec {
                inputs: vec![
                    PortDef::new(0, "mid", SignalKind::Audio),
                    PortDef::new(1, "side", SignalKind::Audio),
                    PortDef::new(2, "width", SignalKind::CvUnipolar)
                        .with_default(1.0)
                        .with_attenuverter(),
                ],
                outputs: vec![
                    PortDef::new(10, "left", SignalKind::Audio),
                    PortDef::new(11, "right", SignalKind::Audio),
                ],
            },
        }
    }
}

impl Default for MidSideDecode {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphModule for MidSideDecode {
    fn port_spec(&self) -> &PortSpec {
        &self.spec
    }

    fn tick(&mut self, inputs: &PortValues, outputs: &mut PortValues) {
        let mid = inputs.get_or(0, 0.0);
        let side = inputs.get_or(1, 0.0);
        let width = inputs.get_or(2, 1.0).clamp(0.0, 2.0);
        let scaled = side * width;
        outputs.set(10, mid + scaled);
        outputs.set(11, mid - scaled);
    }

    fn reset(&mut self) {}

    fn set_sample_rate(&mut self, _: f64) {}

    fn type_id(&self) -> &'static str {
        "mid_side_decode"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(l: f64, r: f64) -> (f64, f64) {
        let mut m = MidSideEncode::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, l);
        inputs.set(1, r);
        m.tick(&inputs, &mut outputs);
        (outputs.get(10).unwrap(), outputs.get(11).unwrap())
    }

    fn decode(mid: f64, side: f64, width: f64) -> (f64, f64) {
        let mut d = MidSideDecode::new();
        let mut inputs = PortValues::new();
        let mut outputs = PortValues::new();
        inputs.set(0, mid);
        inputs.set(1, side);
        inputs.set(2, width);
        d.tick(&inputs, &mut outputs);
        (outputs.get(10).unwrap(), outputs.get(11).unwrap())
    }

    #[test]
    fn test_encode_decode_identity() {
        for &(l, r) in &[(1.0, -1.0), (0.3, 0.7), (-2.5, 4.1), (0.0, 0.0)] {
            let (mid, side) = encode(l, r);
            let (dl, dr) = decode(mid, side, 1.0);
            assert!((dl - l).abs() < 1e-12, "L identity failed: {dl} != {l}");
            assert!((dr - r).abs() < 1e-12, "R identity failed: {dr} != {r}");
        }
    }

    #[test]
    fn test_width_zero_is_mono() {
        let (mid, side) = encode(1.0, -1.0);
        let (dl, dr) = decode(mid, side, 0.0);
        assert!((dl - dr).abs() < 1e-12, "width 0 must be mono");
        assert!((dl - mid).abs() < 1e-12, "mono value must equal mid");
    }

    #[test]
    fn test_mono_input_has_zero_side() {
        let (_mid, side) = encode(0.5, 0.5);
        assert!(side.abs() < 1e-12, "mono input must have zero side");
    }

    #[test]
    fn test_width_clamped_and_doubled() {
        let (mid, side) = encode(1.0, 0.0); // mid 0.5, side 0.5
        let (dl, dr) = decode(mid, side, 2.0);
        // width 2 => L = mid + 2*side = 0.5 + 1.0 = 1.5, R = mid - 1.0 = -0.5
        assert!((dl - 1.5).abs() < 1e-12);
        assert!((dr + 0.5).abs() < 1e-12);
        // Over-range width clamps to 2.0.
        let (dl2, _dr2) = decode(mid, side, 5.0);
        assert!((dl2 - 1.5).abs() < 1e-12);
    }

    #[test]
    fn test_type_ids() {
        assert_eq!(MidSideEncode::default().type_id(), "mid_side_encode");
        assert_eq!(MidSideDecode::default().type_id(), "mid_side_decode");
    }
}
