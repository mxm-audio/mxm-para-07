//! Independent sample-and-hold clock, drooping hold capacitor and output lag.
//!
//! Its source selector is independent of the LFO waveform selector and chooses the
//! LFO saw, its undelayed triangle, or noise. The own clock spans 13 ms..2 s; the
//! lag's printed 500 kohm * 1 uF is a 0.5 s RC whose roughly four-tau settling time
//! is the published 2 s (`research:instruments/sh-7.md` §§3.2, 6.2).
//!
//! **The hold leaks slowly: a 30 s time constant, chosen** (the owner, 2026-09-27).
//! The research derives about 0.7 s from the 0.068 uF hold capacitor and a 10 Mohm
//! it lists with the sample switch (Q203), where such a resistor usually biases the
//! JFET's gate rather than bleeding the capacitor; its own description is milder —
//! a held pitch "not perfectly flat at the slowest clock". 0.68 s lost a quarter of
//! every step at the default 0.2 s clock and 95 % across the slowest, which the
//! scope made plain. At 30 s a step at the default clock stays flat to under 1 % and
//! the slowest clock tilts by about 6 %. Pending a measured unit.

use crate::flush;

pub const SAMPLE_TIME_MIN_S: f32 = 0.013;
pub const SAMPLE_TIME_MAX_S: f32 = 2.0;
pub const HOLD_DROOP_TAU_S: f32 = 30.0; // chosen: see the module doc
pub const LAG_TAU_MAX_S: f32 = 0.5; // derived from 500 kohm * 1 uF

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Source {
    Saw,
    Triangle,
    #[default]
    Random,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Outputs {
    pub held: f32,
    pub out: f32,
    pub clock_high: bool,
    pub clock_rose: bool,
}

#[derive(Debug, Clone)]
pub struct SampleHold {
    phase: f64,
    held: f32,
    out: f32,
    was_high: bool,
}
impl Default for SampleHold {
    fn default() -> Self {
        Self::new()
    }
}
impl SampleHold {
    pub const fn new() -> Self {
        Self {
            phase: 0.0,
            held: 0.0,
            out: 0.0,
            was_high: true,
        }
    }
    pub fn reset(&mut self) {
        *self = Self::new();
    }
    pub fn settle(&mut self) {
        self.out = self.held;
    }
    /// Inputs stay explicit because the independently switched source voltages are
    /// part of the hardware seam, not a generic collection.
    #[allow(clippy::too_many_arguments)]
    #[inline]
    pub fn process(
        &mut self,
        sample_time_s: f32,
        lag_settle_s: f32,
        source: Source,
        saw: f32,
        triangle: f32,
        noise: f32,
        sample_rate: f32,
    ) -> Outputs {
        let fs = sample_rate.max(1.0);
        let period = sample_time_s.clamp(SAMPLE_TIME_MIN_S, SAMPLE_TIME_MAX_S);
        let high = self.phase < 0.5;
        let rose = high && !self.was_high;
        self.was_high = high;
        if rose {
            self.held = match source {
                Source::Saw => saw,
                Source::Triangle => triangle,
                Source::Random => noise,
            };
        } else {
            // Hardware wart: the sample is not perfectly held. It droops toward the
            // zero trimmer between clocks rather than remaining a digital constant.
            let c = (-1.0f64 / (f64::from(HOLD_DROOP_TAU_S) * f64::from(fs))).exp() as f32;
            self.held = flush(self.held * c);
        }
        let settle = lag_settle_s.clamp(0.0, 2.0);
        if settle == 0.0 {
            self.out = self.held;
        } else {
            let tau = (settle * 0.25).clamp(1e-6, LAG_TAU_MAX_S);
            let c = (-1.0f64 / (f64::from(tau) * f64::from(fs))).exp() as f32;
            self.out = flush(self.held + (self.out - self.held) * c);
        }
        self.phase += 1.0 / f64::from(period * fs);
        self.phase -= self.phase.floor();
        Outputs {
            held: self.held,
            out: self.out,
            clock_high: high,
            clock_rose: rose,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const FS: f32 = 48_000.0;

    #[test]
    fn own_clock_samples_its_own_source_independent_of_lfo_selection() {
        let mut s = SampleHold::new();
        let mut rises = 0;
        for _ in 0..FS as usize {
            let o = s.process(0.1, 0.0, Source::Triangle, -0.7, 0.42, -0.2, FS);
            if o.clock_rose {
                rises += 1;
                assert_eq!(o.held, 0.42);
            }
        }
        assert!((9..=10).contains(&rises));
    }

    #[test]
    fn hold_droops_and_output_lag_slews_instead_of_idealising_them() {
        let mut s = SampleHold::new();
        // Cross a complete clock cycle and capture its rising-edge sample.
        let mut start = 0.0;
        for _ in 0..(FS as usize / 25) {
            let o = s.process(0.02, 0.0, Source::Random, 0.0, 0.0, 1.0, FS);
            if o.clock_rose {
                start = o.held;
            }
        }
        assert!(start > 0.9, "test setup must have sampled the source");
        for _ in 0..(FS as usize / 100) {
            s.process(2.0, 1.0, Source::Random, 0.0, 0.0, 1.0, FS);
        }
        let o = s.process(2.0, 1.0, Source::Random, 0.0, 0.0, 1.0, FS);
        assert!(o.held.abs() < start.abs(), "removing droop must fail this");
        assert!(
            (o.out - o.held).abs() > 1e-4,
            "lag must not step to the hold"
        );
    }

    /// **The hold leaks slowly** (the owner, 2026-09-27): flat to under 1 % across the default
    /// 0.2 s sample time, and a visible tilt — more than 5 % — across the slowest 2 s clock. The
    /// derived 0.68 s lost a quarter of every default step; an ideal hold never tilts at all.
    #[test]
    fn the_hold_leaks_slowly() {
        // From reset the slowest clock first rises after one period: take a full-scale sample
        // there, then hold it through the next period with the source at zero.
        let mut s = SampleHold::new();
        let period = (FS * SAMPLE_TIME_MAX_S) as usize;
        let sampled = (0..period + 16).any(|_| {
            s.process(SAMPLE_TIME_MAX_S, 0.0, Source::Random, 0.0, 0.0, 1.0, FS)
                .clock_rose
        });
        assert!(sampled, "test setup must have sampled the source");
        let held: Vec<f32> = (0..period - 16)
            .map(|_| {
                s.process(SAMPLE_TIME_MAX_S, 0.0, Source::Random, 0.0, 0.0, 0.0, FS)
                    .held
            })
            .collect();
        let after_default = held[(FS * 0.2) as usize];
        let after_slowest = held[held.len() - 1];
        assert!(
            after_default > 0.99,
            "a default step lost {:.1} %",
            100.0 * (1.0 - after_default)
        );
        assert!(
            after_slowest < 0.95 && after_slowest > 0.9,
            "across the slowest clock the value kept {after_slowest}"
        );
    }

    #[test]
    fn clock_range_and_reset_are_finite_across_rates() {
        for fs in [1_000.0, 44_100.0, 48_000.0, 192_000.0, 768_000.0] {
            let mut s = SampleHold::new();
            for _ in 0..10_000 {
                let o = s.process(-1.0, 99.0, Source::Random, f32::MAX, -f32::MAX, 1.0, fs);
                assert!(o.held.is_finite() && o.out.is_finite());
            }
            s.reset();
            assert_eq!(s.out, 0.0);
        }
    }
}
