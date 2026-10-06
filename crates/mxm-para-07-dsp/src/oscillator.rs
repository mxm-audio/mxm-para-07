//! High-frequency core, uncleared binary divider and separately driven waveshaper.
//!
//! This is intentionally not an audible-rate phasor. RANGE selects one divider tap;
//! it never retunes the core, all five registers share that count, and sync resets
//! VCO-2's core while leaving its divider and independently CV-driven waveshaper ramps alive
//! (`research:instruments/sh-7.md` §§5.1–5.6). Discontinuities use the two-point
//! PolyBLEP residual from Välimäki et al. The folded-saw triangle is formed from its
//! independent ramp; the voice evaluates folded triangles, pulses, and active ring paths at
//! 8x so their corners and subsequent products are filtered before returning to the host rate.

pub const MIN_AUDIBLE_HZ: f32 = 5.0;
pub const NYQUIST_FRACTION: f32 = 0.45;
pub const OUTPUT_BOUND: f32 = 2.0;
const MAX_CORE_EDGES: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Range {
    Feet32,
    Feet16,
    #[default]
    Feet8,
    Feet4,
    Feet2,
}

impl Range {
    pub const ALL: [Self; 5] = [
        Self::Feet32,
        Self::Feet16,
        Self::Feet8,
        Self::Feet4,
        Self::Feet2,
    ];
    pub const fn semitones(self) -> f32 {
        match self {
            Self::Feet32 => -24.0,
            Self::Feet16 => -12.0,
            Self::Feet8 => 0.0,
            Self::Feet4 => 12.0,
            Self::Feet2 => 24.0,
        }
    }
    const fn divisor(self) -> u32 {
        match self {
            Self::Feet32 => 128,
            Self::Feet16 => 64,
            Self::Feet8 => 32,
            Self::Feet4 => 16,
            Self::Feet2 => 8,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Waveform {
    Triangle,
    Saw,
    #[default]
    Square,
    Pulse,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CoreEdges {
    fractions: [f32; MAX_CORE_EDGES],
    len: usize,
}

impl CoreEdges {
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    fn iter(&self) -> impl Iterator<Item = f32> + '_ {
        self.fractions[..self.len].iter().copied()
    }
}

#[derive(Debug, Clone)]
pub struct Vco {
    core_phase: f64,
    divider_count: u64,
    /// One independently driven ramp per range. A ramp is reset only when its divider output
    /// completes a period; a sync discharge of `core_phase` never touches it.
    shaper_phase: [f64; 5],
    core_inc: f64,
    sample_rate: f32,
    shaper_trim: f32,
}

impl Default for Vco {
    fn default() -> Self {
        Self::new(48_000.0)
    }
}

impl Vco {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            core_phase: 0.0,
            divider_count: 0,
            shaper_phase: [0.0; 5],
            core_inc: 0.0,
            sample_rate: crate::safe_sample_rate(sample_rate),
            shaper_trim: 1.0,
        }
    }
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = crate::safe_sample_rate(sample_rate);
    }
    pub fn reset(&mut self) {
        self.core_phase = 0.0;
        self.divider_count = 0;
        self.shaper_phase = [0.0; 5];
        self.core_inc = 0.0;
    }
    /// Waveshaper calibration provision. Unity is a correctly trimmed unit; no
    /// residual pitch error is invented without a measured unit.
    pub fn set_shaper_trim(&mut self, trim: f32) {
        self.shaper_trim = trim.clamp(0.8, 1.2);
    }
    pub fn divider_count(&self) -> u64 {
        self.divider_count
    }
    pub fn core_phase(&self) -> f32 {
        self.core_phase as f32
    }

    fn set_pitch(&mut self, eight_foot_hz: f32) {
        // The 2' tap must remain representable; the master core is 32 times the 8'
        // output and may legitimately cross several cycles in one audio sample.
        let max_8 = NYQUIST_FRACTION * self.sample_rate / 4.0;
        let hz = eight_foot_hz.clamp(MIN_AUDIBLE_HZ / 4.0, max_8);
        self.core_inc = f64::from(hz * 32.0 / self.sample_rate);
    }

    fn advance_shapers(&mut self, fraction: f64) {
        for (phase, range) in self.shaper_phase.iter_mut().zip(Range::ALL) {
            // The control-current ramp is driven by pitch CV, not by the resettable master-core
            // capacitor. Four periods is already beyond the output rail; bounding there avoids
            // losing precision if sync prevents a divider reset for an arbitrarily long time.
            *phase = (*phase + self.core_inc * fraction / f64::from(range.divisor())).min(4.0);
        }
    }

    fn clock_divider(&mut self) {
        self.divider_count = self.divider_count.wrapping_add(1);
        for (phase, range) in self.shaper_phase.iter_mut().zip(Range::ALL) {
            if self
                .divider_count
                .is_multiple_of(u64::from(range.divisor()))
            {
                *phase = 0.0;
            }
        }
    }

    fn advance_segment(&mut self, fraction: f64, offset: f64, edges: &mut CoreEdges) {
        if self.core_inc <= 0.0 || fraction <= 0.0 {
            return;
        }

        let mut remaining = fraction;
        let mut at = offset;
        while remaining > 0.0 {
            let to_wrap = (1.0 - self.core_phase) / self.core_inc;
            if to_wrap > remaining {
                self.advance_shapers(remaining);
                self.core_phase += self.core_inc * remaining;
                break;
            }

            self.advance_shapers(to_wrap);
            remaining -= to_wrap;
            at += to_wrap;
            self.core_phase = 0.0;
            self.clock_divider();
            if edges.len < MAX_CORE_EDGES {
                edges.fractions[edges.len] = at.clamp(0.0, 1.0) as f32;
                edges.len += 1;
            }
        }
    }

    /// Emit this sample's master-core reset instants and advance one sample.
    pub fn advance_master(&mut self, eight_foot_hz: f32) -> CoreEdges {
        self.advance_master_fraction(eight_foot_hz, 1.0)
    }

    /// Advance a fraction of one host sample. The voice uses this to evaluate nonlinear
    /// waveshapers and ring products above the host rate without changing the evidenced divider.
    pub(crate) fn advance_master_fraction(
        &mut self,
        eight_foot_hz: f32,
        fraction: f64,
    ) -> CoreEdges {
        self.set_pitch(eight_foot_hz);
        let mut edges = CoreEdges::default();
        self.advance_segment(fraction.clamp(0.0, 1.0), 0.0, &mut edges);
        edges
    }

    /// Advance VCO-2. With sync on, every VCO-1 core edge discharges this core at
    /// that fractional instant but does not clear `divider_count`. Natural slave
    /// wraps before/between resets still clock the divider, so VCO-2 CV continues
    /// to set the divided lock and failure pattern.
    pub fn advance_slave(
        &mut self,
        eight_foot_hz: f32,
        master: CoreEdges,
        sync: bool,
    ) -> CoreEdges {
        self.advance_slave_fraction(eight_foot_hz, master, sync, 1.0)
    }

    pub(crate) fn advance_slave_fraction(
        &mut self,
        eight_foot_hz: f32,
        master: CoreEdges,
        sync: bool,
        fraction: f64,
    ) -> CoreEdges {
        self.set_pitch(eight_foot_hz);
        let fraction = fraction.clamp(0.0, 1.0);
        if !sync {
            let mut own = CoreEdges::default();
            self.advance_segment(fraction, 0.0, &mut own);
            return own;
        }
        let mut own = CoreEdges::default();
        let mut at = 0.0f64;
        for edge in master.iter() {
            let edge = f64::from(edge).clamp(at, fraction);
            self.advance_segment(edge - at, at, &mut own);
            // Hardware wart: core only. Clearing the counter here changes engagement
            // phase and is guarded by `core_only_sync_keeps_the_divider_state`.
            self.core_phase = 0.0;
            at = edge;
        }
        self.advance_segment(fraction - at, at, &mut own);
        own
    }

    fn divider_phase(&self, range: Range) -> (f32, f32) {
        let d = u64::from(range.divisor());
        let phase = ((self.divider_count % d) as f64 + self.core_phase) / d as f64;
        (phase as f32, (self.core_inc / d as f64) as f32)
    }

    fn shaper_phase(&self, range: Range) -> (f32, f32) {
        let index = Range::ALL
            .iter()
            .position(|candidate| *candidate == range)
            .expect("every range has one shaper");
        (
            self.shaper_phase[index] as f32,
            (self.core_inc / f64::from(range.divisor())) as f32,
        )
    }

    pub fn render(&self, range: Range, waveform: Waveform, pulse_width: f32) -> f32 {
        self.render_at_rate(range, waveform, pulse_width, 1.0)
    }

    /// Render at a multiple of the host rate. `rate_scale` scales PolyBLEP's support to the
    /// sub-sample interval; phase advancement remains the caller's responsibility.
    pub(crate) fn render_at_rate(
        &self,
        range: Range,
        waveform: Waveform,
        pulse_width: f32,
        rate_scale: f32,
    ) -> f32 {
        let (divider_phase, host_dt) = self.divider_phase(range);
        let dt = host_dt * rate_scale.clamp(1.0 / 128.0, 1.0);
        let (shaper_phase, _) = self.shaper_phase(range);
        match waveform {
            // The separate converter controls ramp slope while the divider resets its
            // period. Unity trim reaches exactly +1 at reset; a measured residual
            // would change height rather than retune the divider.
            Waveform::Saw => {
                let reset_residual = if shaper_phase <= 1.0 {
                    poly_blep(shaper_phase, dt)
                } else {
                    0.0
                };
                2.0 * self.shaper_trim * shaper_phase - 1.0 - self.shaper_trim * reset_residual
            }
            // Wart: triangle folds that separately driven ramp at its midpoint.
            Waveform::Triangle => {
                let ramp = 2.0 * self.shaper_trim * shaper_phase - 1.0;
                1.0 - 2.0 * ramp.abs()
            }
            // Fixed square is routed directly from the divider and bypasses shaper trim.
            Waveform::Square => pulse_phase(divider_phase, dt, 0.5),
            // The comparator sees the separate ramp, so its crossing follows slope trim.
            Waveform::Pulse => {
                if shaper_phase >= 1.0 {
                    -1.0
                } else {
                    pulse_phase(shaper_phase, dt, pulse_width / self.shaper_trim)
                }
            }
        }
        .clamp(-OUTPUT_BOUND, OUTPUT_BOUND)
    }

    #[cfg(test)]
    pub(crate) fn test_divider_phase(&self, range: Range) -> (f32, f32) {
        self.divider_phase(range)
    }

    #[cfg(test)]
    pub(crate) fn test_shaper_phase(&self, range: Range) -> (f32, f32) {
        self.shaper_phase(range)
    }

    /// The five phase-locked VCO-1(A) square registers, 32' through 2'.
    pub fn registers(&self) -> [f32; 5] {
        self.registers_at_rate(1.0)
    }

    pub(crate) fn registers_at_rate(&self, rate_scale: f32) -> [f32; 5] {
        let mut out = [0.0; 5];
        for (slot, range) in out.iter_mut().zip(Range::ALL) {
            *slot = self.render_at_rate(range, Waveform::Square, 0.5, rate_scale);
        }
        out
    }
}

#[inline]
pub fn poly_blep(t: f32, dt: f32) -> f32 {
    let dt = dt.clamp(1e-8, NYQUIST_FRACTION);
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

#[inline]
pub fn clamp_pulse_width(width: f32, dt: f32) -> f32 {
    // Hardware reaches about 10%, never zero; correction overlap can narrow that
    // range further at high pitch but can never widen it past square.
    let lo = 0.10f32.max(2.0 * dt);
    let hi = 0.5f32.min(1.0 - 2.0 * dt);
    if lo > hi { 0.5 } else { width.clamp(lo, hi) }
}

#[inline]
pub fn saw_phase(t: f32, dt: f32) -> f32 {
    2.0 * t - 1.0 - poly_blep(t, dt)
}

#[inline]
pub fn pulse_phase(t: f32, dt: f32, width: f32) -> f32 {
    let width = clamp_pulse_width(width, dt);
    let mut y = if t < width { 1.0 } else { -1.0 };
    y += poly_blep(t, dt);
    let x = if t < width {
        t - width + 1.0
    } else {
        t - width
    };
    y -= poly_blep(x, dt);
    y
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crossings(x: &[f32]) -> usize {
        x.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count()
    }

    #[test]
    #[allow(clippy::needless_range_loop)] // one core advance follows all five taps at each index
    fn range_selects_a_divider_tap_without_retuning_the_core() {
        let fs = 48_000.0;
        let mut v = Vco::new(fs);
        let mut waves = [[0.0f32; 48_000]; 5];
        for i in 0..48_000 {
            for (j, r) in Range::ALL.iter().enumerate() {
                waves[j][i] = v.render(*r, Waveform::Saw, 0.5);
            }
            v.advance_master(220.0);
        }
        let c: Vec<_> = waves.iter().map(|w| crossings(w)).collect();
        assert!((c[2] as i32 - 220).abs() <= 1, "8'={}", c[2]);
        assert_eq!(c[1] / c[0], 2);
        assert_eq!(c[2] / c[1], 2);
        assert_eq!(c[3] / c[2], 2);
        assert_eq!(c[4] / c[3], 2);
    }

    #[test]
    fn all_registers_are_phase_locked_to_vco_1a() {
        let mut v = Vco::new(48_000.0);
        for _ in 0..20_000 {
            let regs = v.registers();
            assert_eq!(
                regs[2].to_bits(),
                v.render(Range::Feet8, Waveform::Square, 0.2).to_bits()
            );
            v.advance_master(261.6256);
        }
    }

    #[test]
    fn one_sided_pwm_never_exceeds_half_and_survives_high_pitch() {
        for dt in [0.001, 0.05, 0.2, 0.44] {
            let w = clamp_pulse_width(-10.0, dt);
            assert!((0.1..=0.5).contains(&w));
            assert!(pulse_phase(0.3, dt, w).is_finite());
        }
    }

    #[test]
    fn core_only_sync_keeps_the_divider_state_and_vco2_slope_matters() {
        let mut master = Vco::new(48_000.0);
        let mut slow = Vco::new(48_000.0);
        let mut fast = Vco::new(48_000.0);
        for _ in 0..123 {
            slow.advance_master(311.0);
            fast.advance_master(311.0);
        }
        let count = slow.divider_count();
        let mut different = false;
        for _ in 0..5000 {
            let e = master.advance_master(220.0);
            slow.advance_slave(180.0, e, true);
            fast.advance_slave(300.0, e, true);
            different |= slow.divider_count() != fast.divider_count();
        }
        assert!(
            slow.divider_count() >= count,
            "sync must not clear the divider"
        );
        assert!(
            different,
            "the slave CV slope must still select the pulse/count pattern"
        );
    }

    #[test]
    fn a_master_reset_that_does_not_clock_the_divider_cannot_rewind_the_audible_ramp() {
        let mut free = Vco::new(48_000.0);
        for _ in 0..37 {
            free.advance_master(220.0);
        }
        let mut synced = free.clone();
        let count = free.divider_count();
        let mut one_master_edge = CoreEdges::default();
        one_master_edge.fractions[0] = 0.5;
        one_master_edge.len = 1;

        free.advance_slave(220.0, CoreEdges::default(), false);
        synced.advance_slave(220.0, one_master_edge, true);

        assert_eq!(
            synced.divider_count(),
            count,
            "the chosen sample must not clock the divider"
        );
        assert_ne!(
            synced.core_phase().to_bits(),
            free.core_phase().to_bits(),
            "the sabotage must actually reset the slave core"
        );
        assert_eq!(
            synced.render(Range::Feet8, Waveform::Saw, 0.5).to_bits(),
            free.render(Range::Feet8, Waveform::Saw, 0.5).to_bits(),
            "core-only sync directly rewound the independently driven waveshaper"
        );
    }

    #[test]
    fn synced_waveshaper_has_both_divided_lock_and_no_divider_clock_failure_regimes() {
        let render = |master_hz: f32, slave_hz: f32| {
            let mut master = Vco::new(48_000.0);
            let mut slave = Vco::new(48_000.0);
            let mut audio = Vec::with_capacity(4096);
            for _ in 0..4096 {
                audio.push(slave.render(Range::Feet8, Waveform::Saw, 0.5));
                let edges = master.advance_master(master_hz);
                slave.advance_slave(slave_hz, edges, true);
            }
            (audio, slave.divider_count())
        };

        // Master edges arrive before the slower slave core can wrap. The divider never clocks, but
        // the control-current ramp must keep moving to the rail instead of restarting per master.
        let (failed, failed_count) = render(440.0, 220.0);
        assert_eq!(failed_count, 0);
        assert!(
            failed[512..].iter().all(|sample| *sample > 1.9),
            "the no-divider-clock regime should leave the independent ramp at its rail"
        );

        // A faster slave naturally clocks the uncleared divider between master discharges. Its
        // selected output eventually resets the ramp, producing repeated audible falls.
        let (locked, locked_count) = render(110.0, 880.0);
        let falls = locked
            .windows(2)
            .filter(|pair| pair[0] > 0.5 && pair[1] < -0.5)
            .count();
        assert!(
            locked_count >= 32,
            "the lock regime never clocked an 8' divider period"
        );
        assert!(
            falls >= 2,
            "divider-output resets were not audible in the lock regime"
        );
    }

    #[test]
    fn sync_engagement_phase_remains_audible_in_the_divider() {
        let mut master = Vco::new(48_000.0);
        let mut a = Vco::new(48_000.0);
        let mut b = Vco::new(48_000.0);
        for _ in 0..137 {
            b.advance_master(777.0);
        }
        let mut differs = false;
        for _ in 0..1000 {
            let e = master.advance_master(220.0);
            let ya = a.render(Range::Feet8, Waveform::Square, 0.5);
            let yb = b.render(Range::Feet8, Waveform::Square, 0.5);
            differs |= ya.to_bits() != yb.to_bits();
            a.advance_slave(220.0, e, true);
            b.advance_slave(220.0, e, true);
        }
        assert!(
            differs,
            "clearing the divider on engagement would erase this phase dependence"
        );
    }

    #[test]
    fn waveshaper_trim_changes_saw_and_fold_but_not_the_direct_square() {
        let mut a = Vco::new(48_000.0);
        let mut b = a.clone();
        b.set_shaper_trim(0.9);
        for _ in 0..100 {
            a.advance_master(220.0);
            b.advance_master(220.0);
        }
        assert_ne!(
            a.render(Range::Feet8, Waveform::Saw, 0.5),
            b.render(Range::Feet8, Waveform::Saw, 0.5)
        );
        assert_eq!(
            a.render(Range::Feet8, Waveform::Square, 0.5),
            b.render(Range::Feet8, Waveform::Square, 0.5)
        );
    }

    fn spectrum_metrics(x: &[f32], periods: usize) -> (f64, f64) {
        let n = x.len();
        let mut wanted = 0.0;
        let mut alias = 0.0;
        for bin in 1..n / 2 {
            let mut re = 0.0;
            let mut im = 0.0;
            for (i, &sample) in x.iter().enumerate() {
                let a = -std::f64::consts::TAU * bin as f64 * i as f64 / n as f64;
                re += f64::from(sample) * a.cos();
                im += f64::from(sample) * a.sin();
            }
            let power = re * re + im * im;
            if bin % periods == 0 {
                wanted += power;
            } else {
                alias += power;
            }
        }
        (
            10.0 * (alias / wanted.max(1e-30)).max(1e-30).log10(),
            wanted,
        )
    }

    #[test]
    fn divider_saw_alias_rejection_keeps_the_wanted_spectrum() {
        // Exactly periodic frequency as mxm-kit's docs/oscillators/06-testing.md requires.
        let (n, periods, fs) = (1024usize, 11usize, 48_000.0f32);
        let hz = periods as f32 * fs / n as f32;
        let mut v = Vco::new(fs);
        let mut corrected = Vec::with_capacity(n);
        let mut trivial = Vec::with_capacity(n);
        for _ in 0..n {
            let (phase, _) = v.shaper_phase(Range::Feet8);
            corrected.push(v.render(Range::Feet8, Waveform::Saw, 0.5));
            trivial.push(2.0 * phase - 1.0);
            v.advance_master(hz);
        }
        let (good_db, good_wanted) = spectrum_metrics(&corrected, periods);
        let (bad_db, bad_wanted) = spectrum_metrics(&trivial, periods);
        assert!(
            good_db < bad_db - 8.0,
            "PolyBLEP bought only {:.1} dB ({good_db:.1} vs {bad_db:.1})",
            bad_db - good_db
        );
        assert!(
            good_wanted > bad_wanted * 0.35,
            "alias rejection must not pass by deleting the wanted harmonics"
        );
    }

    #[test]
    fn narrow_pulse_alias_rejection_keeps_the_wanted_spectrum() {
        let (n, periods, fs) = (1024usize, 9usize, 48_000.0f32);
        let hz = periods as f32 * fs / n as f32;
        let mut v = Vco::new(fs);
        let mut corrected = Vec::with_capacity(n);
        let mut trivial = Vec::with_capacity(n);
        for _ in 0..n {
            let (phase, _) = v.shaper_phase(Range::Feet8);
            corrected.push(v.render(Range::Feet8, Waveform::Pulse, 0.1));
            trivial.push(if phase < 0.1 { 1.0 } else { -1.0 });
            v.advance_master(hz);
        }
        let (good_db, good_wanted) = spectrum_metrics(&corrected, periods);
        let (bad_db, bad_wanted) = spectrum_metrics(&trivial, periods);
        assert!(
            good_db < bad_db - 5.0,
            "second-edge correction missing: {good_db:.1} vs {bad_db:.1}"
        );
        assert!(
            good_wanted > bad_wanted * 0.25,
            "a low-pass cheat removed wanted pulse harmonics"
        );
    }

    #[test]
    fn level_normalised_register_stack_rejects_alias_without_losing_its_grid() {
        let (n, periods, fs) = (1024usize, 3usize, 48_000.0f32);
        let eight_hz = periods as f32 * fs / n as f32 * 4.0;
        let mut v = Vco::new(fs);
        let mut corrected = Vec::with_capacity(n);
        let mut trivial = Vec::with_capacity(n);
        for _ in 0..n {
            corrected.push(v.registers().iter().sum::<f32>() / 5.0);
            let hard = Range::ALL.map(|r| {
                let (phase, _) = v.divider_phase(r);
                if phase < 0.5 { 1.0 } else { -1.0 }
            });
            trivial.push(hard.iter().sum::<f32>() / 5.0);
            v.advance_master(eight_hz);
        }
        let (good_db, good_wanted) = spectrum_metrics(&corrected, periods);
        let (bad_db, bad_wanted) = spectrum_metrics(&trivial, periods);
        assert!(
            good_db < bad_db - 5.0,
            "register stack correction missing: {good_db:.1} vs {bad_db:.1}"
        );
        assert!(
            good_wanted > bad_wanted * 0.3,
            "correction must preserve the phase-locked register spectrum"
        );
    }

    #[test]
    fn core_sync_rejects_divider_edge_alias_without_erasing_the_lock_pattern() {
        let (n, periods, fs) = (1024usize, 11usize, 48_000.0f32);
        let mut master = Vco::new(fs);
        let mut slave = Vco::new(fs);
        let master_hz = periods as f32 * fs / n as f32;
        let slave_hz = 17.0 * fs / n as f32;
        let mut corrected = Vec::with_capacity(n);
        let mut trivial = Vec::with_capacity(n);
        for _ in 0..n {
            let (phase, _) = slave.divider_phase(Range::Feet8);
            corrected.push(slave.render(Range::Feet8, Waveform::Square, 0.5));
            trivial.push(if phase < 0.5 { 1.0 } else { -1.0 });
            let edges = master.advance_master(master_hz);
            slave.advance_slave(slave_hz, edges, true);
        }
        let (good_db, good_wanted) = spectrum_metrics(&corrected, periods);
        let (bad_db, bad_wanted) = spectrum_metrics(&trivial, periods);
        assert!(
            good_db < bad_db - 3.0,
            "synced divider edge correction missing: {good_db:.1} vs {bad_db:.1}"
        );
        assert!(
            good_wanted > bad_wanted * 0.25,
            "alias rejection must keep the engagement-dependent sync pattern"
        );
    }

    #[test]
    fn output_is_finite_and_bounded_at_extremes() {
        for fs in [1_000.0, 44_100.0, 48_000.0, 96_000.0, 192_000.0, 768_000.0] {
            let mut v = Vco::new(fs);
            for _ in 0..10_000 {
                for r in Range::ALL {
                    for w in [
                        Waveform::Triangle,
                        Waveform::Saw,
                        Waveform::Square,
                        Waveform::Pulse,
                    ] {
                        let y = v.render(r, w, -100.0);
                        assert!(y.is_finite() && y.abs() <= OUTPUT_BOUND);
                    }
                }
                v.advance_master(f32::MAX);
            }
        }
    }
}
