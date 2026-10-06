//! One LFO with destination-dependent offsets and a sine-only delay.
//!
//! The voltage annotations are preserved as ratios rather than centring every shape:
//! VCO/VCF `PLUS` is saw +13/-2 V, square +13/0 V, sine +/-6 V; VCA
//! `0-CENTER` is saw +8/-4 V, square +/-7 V, sine +/-6 V. PWM always reads
//! the separate undelayed 0..+7 V triangle (`research:instruments/sh-7.md` §6.1).
//! The switched-inversion saw is rendered with a perfect seam: a residual seam and
//! its possible doubled rate are explicitly unverified and are not invented.

use crate::flush;

pub const RATE_MIN_HZ: f32 = 0.2;
pub const RATE_MAX_HZ: f32 = 25.0;
pub const DELAY_MAX_S: f32 = 3.0;
/// Chosen mapping: the displayed delay reaches 95% at its endpoint (three RC taus).
pub const DELAY_TAUS: f32 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Waveform {
    Saw,
    Square,
    #[default]
    Sine,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Outputs {
    pub saw: f32,
    pub triangle_bipolar: f32,
    pub pwm_triangle: f32,
    pub square_high: bool,
    pub square_rose: bool,
    pub plus: f32,
    pub zero_center: f32,
    pub delayed_sine: f32,
}

#[derive(Debug, Clone)]
pub struct Lfo {
    phase: f64,
    fade: f32,
    square_was_high: bool,
}
impl Default for Lfo {
    fn default() -> Self {
        Self::new()
    }
}
impl Lfo {
    pub const fn new() -> Self {
        Self {
            phase: 0.0,
            fade: 1.0,
            square_was_high: true,
        }
    }
    pub fn reset(&mut self) {
        *self = Self::new();
    }
    /// Keyboard gate dumps the sine-delay capacitor without restarting LFO phase.
    pub fn keyboard_gate(&mut self, delay_s: f32) {
        if delay_s > 0.0 {
            self.fade = 0.0;
        }
    }
    /// Keyboard trigger slams the integrator to the waveform's highest phase.
    pub fn keyboard_restart(&mut self) {
        self.phase = 0.0;
        self.square_was_high = false;
    }
    pub fn phase(&self) -> f32 {
        self.phase as f32
    }
    pub fn fade(&self) -> f32 {
        self.fade
    }
    pub fn settle(&mut self) {
        self.fade = 1.0;
    }

    #[inline]
    pub fn process(
        &mut self,
        rate_hz: f32,
        waveform: Waveform,
        delay_s: f32,
        sample_rate: f32,
    ) -> Outputs {
        let p = self.phase as f32;
        // phase zero is the top of all three selected shapes, as the restart circuit requires.
        let tri = if p < 0.5 {
            1.0 - 4.0 * p
        } else {
            -3.0 + 4.0 * p
        };
        let saw = 1.0 - 2.0 * p;
        let square_high = p < 0.5;
        let square_rose = square_high && !self.square_was_high;
        self.square_was_high = square_high;
        // A diode-rounded triangle, normalised. Chosen knee; no trace exists to fit it.
        let sine = {
            let x = tri;
            let y = x - 0.18 * x * x * x;
            y / 0.82
        };

        let delay = delay_s.clamp(0.0, DELAY_MAX_S);
        if delay <= 0.0 {
            self.fade = 1.0;
        } else if self.fade < 1.0 {
            let tau = delay / DELAY_TAUS;
            let c = (-1.0f64 / (f64::from(tau) * f64::from(sample_rate.max(1.0)))).exp() as f32;
            self.fade = flush(1.0 + (self.fade - 1.0) * c);
            if self.fade > 0.999 {
                self.fade = 1.0;
            }
        }
        let delayed_sine = sine * self.fade;
        let plus = match waveform {
            Waveform::Saw => (15.0 * (saw + 1.0) * 0.5 - 2.0) / 13.0,
            Waveform::Square => {
                if square_high {
                    1.0
                } else {
                    0.0
                }
            }
            Waveform::Sine => delayed_sine * (6.0 / 13.0),
        };
        let zero_center = match waveform {
            Waveform::Saw => (12.0 * (saw + 1.0) * 0.5 - 4.0) / 8.0,
            Waveform::Square => {
                if square_high {
                    7.0 / 8.0
                } else {
                    -7.0 / 8.0
                }
            }
            Waveform::Sine => delayed_sine * 0.75,
        };

        self.phase +=
            f64::from(rate_hz.clamp(RATE_MIN_HZ, RATE_MAX_HZ)) / f64::from(sample_rate.max(1.0));
        self.phase -= self.phase.floor();
        Outputs {
            saw,
            triangle_bipolar: tri,
            pwm_triangle: 0.5 * (tri + 1.0),
            square_high,
            square_rose,
            plus,
            zero_center,
            delayed_sine,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const FS: f32 = 48_000.0;

    #[test]
    fn shapes_keep_their_destination_offsets() {
        for shape in [Waveform::Saw, Waveform::Square, Waveform::Sine] {
            let mut l = Lfo::new();
            let mut plus = (f32::INFINITY, f32::NEG_INFINITY);
            let mut zero = plus;
            for _ in 0..FS as usize {
                let o = l.process(1.0, shape, 0.0, FS);
                plus = (plus.0.min(o.plus), plus.1.max(o.plus));
                zero = (zero.0.min(o.zero_center), zero.1.max(o.zero_center));
            }
            match shape {
                Waveform::Saw => {
                    assert!(plus.0 > -0.17 && plus.1 > 0.99);
                    assert!(zero.0 < -0.49 && zero.1 > 0.99);
                }
                Waveform::Square => {
                    assert!(plus.0 == 0.0 && plus.1 == 1.0);
                    assert!((zero.0 + zero.1).abs() < 1e-6);
                }
                Waveform::Sine => {
                    assert!((plus.0 + plus.1).abs() < 0.01);
                    assert!((zero.0 + zero.1).abs() < 0.01);
                }
            }
        }
    }

    #[test]
    fn delay_fades_sine_only_and_pwm_is_always_undelayed_triangle() {
        let mut delayed = Lfo::new();
        for _ in 0..1234 {
            delayed.process(2.0, Waveform::Sine, 0.0, FS);
        }
        let phase = delayed.phase();
        let mut undelayed = delayed.clone();
        delayed.keyboard_gate(1.0);
        assert_eq!(delayed.phase(), phase, "delay dump must not restart phase");
        for shape in [Waveform::Saw, Waveform::Square] {
            let oa = delayed.process(2.0, shape, 1.0, FS);
            let ob = undelayed.process(2.0, shape, 0.0, FS);
            assert_eq!(oa.plus, ob.plus, "non-sine shape must ignore delay");
            assert_eq!(oa.pwm_triangle, ob.pwm_triangle);
        }
        let mut l = Lfo::new();
        l.keyboard_gate(1.0);
        let o = l.process(2.0, Waveform::Sine, 1.0, FS);
        assert!(o.plus.abs() < 0.01 && o.pwm_triangle > 0.99);
    }

    #[test]
    fn keyboard_restart_puts_every_shape_at_its_documented_high_phase() {
        let mut l = Lfo::new();
        for _ in 0..1234 {
            l.process(3.0, Waveform::Sine, 0.0, FS);
        }
        l.keyboard_restart();
        for shape in [Waveform::Saw, Waveform::Square, Waveform::Sine] {
            let mut c = l.clone();
            let o = c.process(3.0, shape, 0.0, FS);
            assert!(o.plus > 0.45);
        }
    }

    #[test]
    fn rate_and_reset_are_deterministic_at_all_supported_rates() {
        for fs in [1_000.0, 44_100.0, 48_000.0, 96_000.0, 192_000.0, 768_000.0] {
            let mut l = Lfo::new();
            let mut rises = 0;
            for _ in 0..(fs as usize * 2) {
                rises += l.process(4.0, Waveform::Square, 0.0, fs).square_rose as usize;
            }
            assert!((7..=9).contains(&rises), "fs={fs} rises={rises}");
        }
    }
}
