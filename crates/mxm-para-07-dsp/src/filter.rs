//! The SH-7's four-OTA cascade: 4-pole resonant lowpass, diode clamp in the loop.
//!
//! `research:instruments/sh-7.md` §7.5 is the circuit: four CA3080 stages and a
//! diode-limited global feedback path. This is a local copy of the collection's
//! topology-preserving (TPT/ZDF) four-OTA model, with this machine's 5 Hz floor,
//! resonance calibration, headroom and fixed discrete-stage spread. The Newton
//! solve and diode curve remain local rather than being prematurely extracted.
//!
//! # Why the nonlinearity is inside the loop
//!
//! An earlier design solved a *linear* feedback loop analytically and applied
//! `tanh` only to the filter's input. That cannot work: a linear resonant loop
//! driven past its oscillation threshold grows without bound, and clipping the
//! input constrains neither the loop state nor the output. The nonlinearity has
//! to sit in the feedback path, which is also where it sits in the real circuit.
//!
//! # Why the feedback saturator is a diode clamp, not `tanh`
//!
//! The SH-7 puts back-to-back diodes to ground in its resonance feedback path
//! (`research:instruments/sh-7.md` §7.5). Below their conduction knee the pair does
//! essentially nothing; above it the pair shunts feedback so self-oscillation
//! settles at a bounded level. `tanh` is curved from zero and would colour the loop
//! continuously, so substituting it would erase this evidenced hardware wart.
//!
//! [`diode_clamp`] is `research:filters/machines/ir3109-roland.md` §7's curve,
//! `f(x) = x / (1 + |x/knee|³)^(1/3)`:
//! bounded by the knee, strictly monotonic, odd, and with a derivative that falls
//! out of the same cube root — which the Newton solve below needs every step.
//! `tanh` shapes the *input stage*, scaled by `INPUT_KNEE` so it bounds late: that is
//! headroom, not the resonance circuit.
//!
//! # Why this is bounded
//!
//! Each TPT one-pole is stable for `g > 0`. The only path back into the loop is
//! through [`diode_clamp`], whose magnitude never exceeds [`CLAMP_KNEE`]. So the
//! injected feedback is bounded by `k * CLAMP_KNEE` regardless of internal state,
//! and the ladder input satisfies `|x| <= INPUT_KNEE + k * CLAMP_KNEE`. With unity DC gain
//! per stage that bounds the output, and self-oscillation is amplitude-limited by
//! the same clamp that limits the analog loop.
//!
//! # Verified behaviour
//!
//! Cascading the four one-poles and substituting the feedback gives a scalar
//! nonlinear equation in the output, which [`Ladder::process`] solves with four
//! fixed Newton steps from the equation's linear small-signal root. Root uniqueness
//! alone does not guarantee fixed-step convergence from an arbitrary previous output;
//! the local residual test therefore compares this solve with a converged `f64`
//! reference across rate/control boundaries and abrupt input/state transitions.
//!
//! The tests in this module measure the implementation that ships: solver residual,
//! oscillation onset across cutoff and sample rate, frequency tracking, bounded output,
//! no-Q DC droop, the diode clamp's clean-to-limited transition, and the effect of this
//! unit's fixed stage spread. `examples/measure.rs` independently exercises the filter's no-Q
//! DC gain alongside the crate's other evidence-backed seams. No comparison
//! table for rejected prototype solvers is claimed here because this crate does not
//! contain a reproducer for those implementations.

use crate::{Rng, flush};
use std::f32::consts::PI;

/// Resonance maps to `k` in `0..=K_MAX`, and `K_MAX` is **calibrated to the machine**:
/// the SH-7's adjustment procedure requires self-oscillation to start and hold
/// between 7 and 9 on the RESONANCE slider's scale (`research:instruments/sh-7.md`
/// §3.6, §11). With the measured threshold at `k` = 4.00 the onset lands at
/// `4 / K_MAX` of the control, so 5.0 puts it at 0.80 — the middle of the band —
/// and any value in `4/0.9 ..= 4/0.7` would satisfy the procedure. The onset is
/// measured on the running filter by `calibration_puts_self_oscillation_between_7_and_9`.
/// mono-01's 4.5 was chosen as headroom for a different scale.
pub const K_MAX: f32 = 5.0;

/// Above this resonance the filter is excited so self-oscillation can start from
/// silence. See [`Ladder::process`]. Below the calibrated onset (`4 / K_MAX` = 0.80)
/// with a margin, so a filter that would sing is never left waiting for a signal;
/// mono-01's 0.9 sat below *its* onset the same way.
pub const EXCITATION_THRESHOLD: f32 = 0.75;

/// Amplitude of that excitation. About -120 dB: inaudible against any real signal,
/// but enough to seed oscillation.
pub const EXCITATION_LEVEL: f32 = 1e-6;

/// Where the input stage begins to bound, in the loop's units. The SH-7's first
/// stage is padded and its CA3080 stages run linearly over ordinary mixer level;
/// the bound here
/// exists for the collection's numeric contract, not as the machine's drive.
/// **Chosen**: eight times the mixer's full scale, so a full-scale signal is
/// compressed by about 0.05 dB (`the_input_stage_bounds_late_and_the_stages_do_not_saturate`
/// holds it under 0.1). A working-unit drive sweep would replace this calibration.
pub const INPUT_KNEE: f32 = 8.0;

/// The hardware's published floor (`research:instruments/sh-7.md` §§3.9, 7.5):
/// 5 Hz–20 kHz.
const CUTOFF_MIN_HZ: f32 = 5.0;

/// Newton steps used to solve the resonance feedback each sample. Four steps from
/// the linear small-signal root keep the measured residual within the fixed gate in
/// `fixed_solver_matches_a_converged_reference_across_abrupt_boundaries`; the count
/// stays fixed so the audio thread has no convergence branch.
const NEWTON_ITERATIONS: usize = 4;

/// Cutoff ceiling as a fraction of the sample rate. `tan` blows up at Nyquist, and
/// the Pade approximation below is only valid short of it.
const NYQUIST_FRACTION: f32 = 0.45;

/// Where the diode clamp in the resonance path starts to conduct, in the loop's
/// own units — the ladder input after the drive stage never exceeds 1.
///
/// **Chosen, not measured.** The knee is what sets the level self-oscillation
/// settles at and the resonance setting at which the clamp's colour first appears.
/// 1.0 puts the knee at the loop's full scale: the feedback path is clean for any
/// signal the drive stage can deliver and only starts limiting once the resonant
/// peak carries the loop past full scale, which is the "flat for most of the knob"
/// property the circuit implies; a tighter knee sings quieter and buzzier. The two
/// bench measurements that would replace this number are self-oscillation amplitude
/// against resonance and level against resonance at DC.
pub const CLAMP_KNEE: f32 = 1.0;

/// How far each stage's integrator gain sits from nominal, as a fraction — this unit's
/// discrete-stage calibration (`research:instruments/sh-7.md` §§7.5, 11). The four
/// CA3080s and capacitors are discrete; their loaded mismatch was not measured.
/// **Chosen**: ±1.5 %, slightly below the ±2 % sibling experiment whose effect the
/// research has measured (`research:filters/machines/ir3109-roland.md` §10). A bench
/// sweep of one working unit would replace it. Drawn once, seeded, at construction: this
/// build is always this one unit, so a golden score can exist.
/// `this_units_stage_spread_moves_the_peak_and_not_the_onset` measures what it does.
pub const STAGE_SPREAD: f32 = 0.015;

/// The seed the spread is drawn from. Any fixed number would do; this one names the chip
/// and the machine.
const SPREAD_SEED: u32 = 0xCA30_8007;

/// Output bound following from the boundedness argument above — the input stage is
/// bounded by `INPUT_KNEE`, the feedback by `K_MAX * CLAMP_KNEE` — plus headroom for
/// the resonant peak's transient overshoot.
pub const OUTPUT_BOUND: f32 = INPUT_KNEE + K_MAX * CLAMP_KNEE + 2.5;

/// `tan(x)` via the [5/4] Pade approximant, for `x` in `[0, PI * 0.45]`.
///
/// `f32::tan` is a libm call and this runs per sample. The approximant is accurate
/// to better than 1e-4 relative across the whole valid range and has its pole at
/// `PI/2`, exactly where `tan` does. Evaluated in `f64` because numerator and
/// denominator both lose significance near the top of the range.
#[inline]
pub fn tan_approx(x: f64) -> f64 {
    let x2 = x * x;
    let x4 = x2 * x2;
    x * (945.0 - 105.0 * x2 + x4) / (945.0 - 420.0 * x2 + 15.0 * x4)
}

/// `tanh(x)` via the [7/6] Pade approximant, with the input clamped.
///
/// Accurate to better than 1e-4 absolute over the clamped range, which matters
/// more than it might look: this saturator *is* the filter's drive character and
/// it sets the amplitude at which self-oscillation settles. A cheaper [3/2] form
/// was tried first and deviates by up to 2.4% around `x = 1.5` — audible as a
/// different saturation colour, for the sake of a few multiplies.
///
/// The input clamp is load-bearing, not defensive: without it the rational form
/// diverges for large `x`, which would break the boundedness argument the whole
/// filter design rests on. `tanh(4) = 0.9993`, so the curve is essentially flat
/// where the clamp takes over and the discontinuity in slope is negligible.
///
/// The output clamp guards the last ulp: the invariant `|tanh_approx(x)| <= 1` is
/// what bounds the filter, so it should be exactly true in `f32`, not nearly true.
#[inline]
pub fn tanh_approx(x: f32) -> f32 {
    let x = x.clamp(-4.0, 4.0);
    let x2 = x * x;
    let num = x * (135135.0 + x2 * (17325.0 + x2 * (378.0 + x2)));
    let den = 135135.0 + x2 * (62370.0 + x2 * (3150.0 + x2 * 28.0));
    (num / den).clamp(-1.0, 1.0)
}

/// Back-to-back diodes to ground, as a curve: `f(x) = x / (1 + |x/knee|³)^(1/3)`,
/// from `research:filters/machines/ir3109-roland.md` §7.
///
/// Returns `(f(x), f'(x))`. With `d = (1 + |x/knee|³)^(1/3)` the value is `x/d` and
/// the derivative is `1/d⁴`, so one cube root serves both — which matters because
/// the Newton solve in [`Ladder::process`] wants the derivative every step.
///
/// Properties the filter's boundedness argument rests on, each pinned by a test:
/// `|f(x)| <= knee` for every finite `x` (the output clamp makes that exactly true
/// in `f32`, not nearly true); `f` is strictly increasing, so the solve has one
/// root; `f` is odd, so it adds no even harmonics and no DC; and `f'(0) = 1`, so
/// the small-signal loop is the ideal ladder and the oscillation threshold is
/// unchanged from the linear theory.
#[inline]
pub fn diode_clamp(x: f32, knee: f32) -> (f32, f32) {
    let a = x / knee;
    let a3 = (a * a * a).abs();
    // Past about 1e12 knees the cube overflows and `x / inf` would return 0 —
    // the one input for which the plain formula is *not* bounded by the knee.
    // The solve never gets there; the bound is promised for every finite input.
    if !a3.is_finite() {
        return (knee.copysign(x), 0.0);
    }
    let inv = 1.0 / (1.0 + a3).cbrt();
    let inv2 = inv * inv;
    ((x * inv).clamp(-knee, knee), inv2 * inv2)
}

#[cfg(test)]
#[inline]
fn feedback_residual(y: f32, pu_a: f32, pk: f32) -> f32 {
    pu_a - pk * diode_clamp(y, CLAMP_KNEE).0 - y
}

#[inline]
fn solve_feedback(pu_a: f32, pk: f32) -> f32 {
    // Replacing the clamp by its unity-slope small-signal form gives
    // y = pu_a / (1 + pk). The real clamp has no greater magnitude than that
    // linear form, so this seed has the root's sign and starts on its near side.
    let mut y = pu_a / (1.0 + pk);
    for _ in 0..NEWTON_ITERATIONS {
        let (clamped, derivative) = diode_clamp(y, CLAMP_KNEE);
        let residual = pu_a - pk * clamped - y;
        y += residual / (1.0 + pk * derivative);
    }
    y
}

/// A 4-pole resonant lowpass ladder.
#[derive(Debug, Clone)]
pub struct Ladder {
    /// One integrator state per pole.
    s: [f32; 4],
    /// Per-stage multiplicative trim on the integrator gain — this unit's matching
    /// (`STAGE_SPREAD`). A property of the unit, not state: `reset` leaves it.
    trim: [f32; 4],
    /// Deterministic excitation source for self-oscillation.
    rng: Rng,
}

impl Default for Ladder {
    fn default() -> Self {
        Self::new()
    }
}

impl Ladder {
    /// This unit's filter: four stages with the fixed seeded spread. What ships.
    pub fn new() -> Self {
        let mut rng = Rng::new(SPREAD_SEED);
        let mut trim = [1.0f32; 4];
        for t in trim.iter_mut() {
            *t = 1.0 + STAGE_SPREAD * rng.next_bipolar();
        }
        Self::with_trim(trim)
    }

    /// Four identical stages: the matched reference used to measure the chosen spread,
    /// not what ships.
    pub fn matched() -> Self {
        Self::with_trim([1.0; 4])
    }

    const fn with_trim(trim: [f32; 4]) -> Self {
        Self {
            s: [0.0; 4],
            trim,
            // Fixed seed: excitation must be bit-repeatable so it can be tested.
            rng: Rng::new(0x5EED_1101),
        }
    }

    /// The per-stage trims, for the measurements of what the spread does.
    pub fn trim(&self) -> [f32; 4] {
        self.trim
    }

    /// Clear all state. Leaves no tail from previous playback. The trims stay: they are
    /// the unit, not its state.
    pub fn reset(&mut self) {
        self.s = [0.0; 4];
        self.rng = Rng::new(0x5EED_1101);
    }

    /// Process one sample.
    ///
    /// `resonance` is `0..=1`. Coefficients are recomputed every sample so that
    /// per-sample envelope and LFO modulation of the cutoff actually takes effect.
    #[inline]
    pub fn process(&mut self, input: f32, cutoff_hz: f32, resonance: f32, sample_rate: f32) -> f32 {
        let sample_rate = crate::safe_sample_rate(sample_rate);
        let fc = cutoff_hz.clamp(CUTOFF_MIN_HZ, NYQUIST_FRACTION * sample_rate);
        let g = tan_approx((PI * fc / sample_rate) as f64) as f32;

        let resonance = resonance.clamp(0.0, 1.0);
        let k = K_MAX * resonance;

        // A late, bounded input stage: linear over the mixer's range, limiting far
        // above it. See `INPUT_KNEE`.
        let mut u = INPUT_KNEE * tanh_approx(input / INPUT_KNEE);

        // A filter fed digital silence stays silent forever, so self-oscillation
        // needs a seed. This is deliberate excitation, not denormal protection
        // (that is `flush`), and it is gated so that "no input, low resonance"
        // stays exactly zero.
        if resonance > EXCITATION_THRESHOLD {
            u += EXCITATION_LEVEL * self.rng.next_bipolar();
        }

        // Each TPT one-pole gives y = G*x + (1-G)*s. With per-stage trims the four G
        // differ (`mxm-poly-06-dsp`'s form), so the cascade's input gain and its
        // constant term are products over the stages:
        //   y4 = p*x + a,   p = G1 G2 G3 G4,   a = G4 G3 G2 s1' + G4 G3 s2' + G4 s3' + s4'
        // With the resonance feedback x = u - k*f(y4), f the diode clamp, that
        // becomes a scalar nonlinear equation in y4:
        //   F(y) = p*u + a - p*k*f(y) - y = 0
        let mut big_g = [0.0f32; 4];
        for (gi, t) in big_g.iter_mut().zip(&self.trim) {
            let gt = g * t;
            *gi = gt / (1.0 + gt);
        }
        let s1 = (1.0 - big_g[0]) * self.s[0];
        let s2 = (1.0 - big_g[1]) * self.s[1];
        let s3 = (1.0 - big_g[2]) * self.s[2];
        let s4 = (1.0 - big_g[3]) * self.s[3];
        let a = big_g[3] * big_g[2] * big_g[1] * s1 + big_g[3] * big_g[2] * s2 + big_g[3] * s3 + s4;
        let p = big_g[0] * big_g[1] * big_g[2] * big_g[3];
        let pu_a = p * u + a;
        let pk = p * k;

        // Solve the current-sample equation with fixed cost. F is strictly decreasing
        // (F' <= -1), so it has exactly one root and the derivative cannot vanish.
        // Root uniqueness does not make Newton globally convergent from the stale previous
        // output: an abrupt sign change can make three such steps alternate across the knee.
        // The linear small-signal solution is a stable seed on the root's own side, and the
        // converged-reference test measures four steps from it over the supported sweep.
        //
        // Keeping the solve current-sample puts the clamp inside the loop. Delayed feedback or
        // a stale linear gain would be a different equation and would no longer establish the
        // bound: |k*f(y)| <= k*CLAMP_KNEE holds here by construction.
        let y_solved = solve_feedback(pu_a, pk);

        let x = u - k * diode_clamp(y_solved, CLAMP_KNEE).0;

        let mut y = x;
        for (s, gi) in self.s.iter_mut().zip(&big_g) {
            let v = (y - *s) * gi;
            y = v + *s;
            *s = flush(y + v);
        }

        y
    }
}

/// Measure the resonance at which the filter starts to self-oscillate, by exciting
/// it with an impulse and comparing energy early and late in the decay.
///
/// The ideal ladder's threshold is `k = 4`, but a discretised nonlinear loop earns
/// its onset rather than inheriting it, so this exists to find out what the running
/// implementation actually does rather than assume it.
pub fn measure_oscillation_threshold(cutoff_hz: f32, sample_rate: f32) -> f32 {
    measure_oscillation_threshold_of(Ladder::new, cutoff_hz, sample_rate)
}

/// The same measurement on a filter built by `make` — `Ladder::matched` for the
/// reference this unit's spread is measured against.
pub fn measure_oscillation_threshold_of(
    make: fn() -> Ladder,
    cutoff_hz: f32,
    sample_rate: f32,
) -> f32 {
    let sustains = |resonance: f32| {
        let mut f = make();
        // Impulse, then let the transient settle before measuring.
        f.process(1.0, cutoff_hz, resonance, sample_rate);
        let settle = (sample_rate * 0.20) as usize;
        for _ in 0..settle {
            f.process(0.0, cutoff_hz, resonance, sample_rate);
        }
        let window = (sample_rate * 0.05) as usize;
        let mut early = 0.0f32;
        for _ in 0..window {
            early = early.max(f.process(0.0, cutoff_hz, resonance, sample_rate).abs());
        }
        for _ in 0..(sample_rate as usize / 2) {
            f.process(0.0, cutoff_hz, resonance, sample_rate);
        }
        let mut late = 0.0f32;
        for _ in 0..window {
            late = late.max(f.process(0.0, cutoff_hz, resonance, sample_rate).abs());
        }
        // Sustained (or growing) rather than decaying away. The floor sits far above
        // the excitation's -120 dB and far below any real oscillation: with this
        // crate's excitation starting *below* the calibrated onset, mono-01's 1e-9
        // floor read the excitation as oscillation at high cutoffs, where the
        // impulse has decayed inside the settle window and early and late are both
        // the noise floor. The absolute floor keeps that excitation out of the onset result.
        late > early * 0.5 && late > 1e-3
    };

    // Bisect on resonance. The predicate is monotonic in practice.
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    if sustains(lo) {
        return 0.0;
    }
    if !sustains(hi) {
        return f32::NAN; // never oscillates in range
    }
    for _ in 0..24 {
        let mid = 0.5 * (lo + hi);
        if sustains(mid) { hi = mid } else { lo = mid }
    }
    0.5 * (lo + hi)
}

/// Frequency of self-oscillation, estimated by counting zero crossings.
pub fn measure_oscillation_frequency(cutoff_hz: f32, sample_rate: f32) -> f32 {
    let mut f = Ladder::new();
    for _ in 0..(sample_rate as usize) {
        f.process(0.0, cutoff_hz, 1.0, sample_rate);
    }
    let n = (sample_rate as usize) / 2;
    let (mut crossings, mut prev) = (0usize, 0.0f32);
    for _ in 0..n {
        let y = f.process(0.0, cutoff_hz, 1.0, sample_rate);
        if prev <= 0.0 && y > 0.0 {
            crossings += 1;
        }
        prev = y;
    }
    crossings as f32 / (n as f32 / sample_rate)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATES: [f32; 4] = [44_100.0, 48_000.0, 96_000.0, 192_000.0];

    /// Peak magnitude of the response to a sine at `freq`, after the transient has
    /// decayed. Amplitude is small so the saturators stay in their linear region
    /// and we measure the *filter*, not the drive.
    fn magnitude_at(cutoff: f32, resonance: f32, freq: f32, fs: f32) -> f32 {
        magnitude_of(&mut Ladder::new(), cutoff, resonance, freq, fs)
    }

    /// The same measurement on a given filter — this unit's or the matched reference.
    fn magnitude_of(f: &mut Ladder, cutoff: f32, resonance: f32, freq: f32, fs: f32) -> f32 {
        const AMP: f32 = 1e-3;
        let settle = (fs * 0.5) as usize;
        let measure = (fs / freq * 8.0) as usize + 64;

        for n in 0..settle {
            let t = n as f32 / fs;
            f.process(AMP * (2.0 * PI * freq * t).sin(), cutoff, resonance, fs);
        }
        let mut peak = 0.0f32;
        for n in settle..settle + measure {
            let t = n as f32 / fs;
            let y = f.process(AMP * (2.0 * PI * freq * t).sin(), cutoff, resonance, fs);
            peak = peak.max(y.abs());
        }
        peak / AMP
    }

    #[test]
    fn tan_approx_matches_libm_across_the_valid_range() {
        let max_x = std::f64::consts::PI * 0.45;
        let mut worst = 0.0f64;
        for i in 0..=10_000 {
            let x = max_x * i as f64 / 10_000.0;
            let (approx, exact) = (tan_approx(x), x.tan());
            if exact > 1e-9 {
                worst = worst.max(((approx - exact) / exact).abs());
            }
        }
        assert!(worst < 1e-4, "worst relative tan error {worst:e}");
    }

    #[test]
    fn tanh_approx_is_bounded_and_monotonic() {
        // The boundedness argument for the whole filter depends on this.
        let mut prev = f32::NEG_INFINITY;
        for i in -20_000..=20_000 {
            let x = i as f32 / 1000.0; // -20..20, well past the clamp
            let y = tanh_approx(x);
            assert!(y.abs() <= 1.0, "tanh_approx({x}) = {y} exceeds 1");
            assert!(y >= prev - 1e-6, "not monotonic at x={x}");
            prev = y;
        }
        assert!(tanh_approx(0.0).abs() < 1e-9);

        // Odd symmetry: a saturator that is not odd adds even harmonics and DC.
        for i in 0..=300 {
            let x = i as f32 / 100.0;
            assert!(
                (tanh_approx(x) + tanh_approx(-x)).abs() < 1e-6,
                "not odd at x={x}"
            );
        }

        // Close to real tanh across the whole clamped range. This curve is the
        // filter's drive character and sets the self-oscillation amplitude, so it
        // is held to a real tolerance rather than "close enough for a shaper".
        let mut worst = 0.0f32;
        for i in 0..=400 {
            let x = i as f32 / 100.0; // 0..4
            worst = worst.max((tanh_approx(x) - x.tanh()).abs());
        }
        assert!(worst < 1e-4, "worst deviation from tanh: {worst}");

        for i in 0..=20 {
            let x = i as f32 / 100.0; // small signal: 0..0.2
            assert!(
                (tanh_approx(x) - x.tanh()).abs() < 1e-6,
                "small-signal error at x={x}"
            );
        }
    }

    #[test]
    fn diode_clamp_is_bounded_monotonic_odd_and_its_derivative_is_exact() {
        // The boundedness argument for the whole filter depends on the first
        // property; the Newton solve's single root depends on the second.
        let knee = CLAMP_KNEE;
        let mut prev = f32::NEG_INFINITY;
        for i in -40_000..=40_000 {
            let x = i as f32 / 1000.0; // -40..40, far past the knee
            let (y, dy) = diode_clamp(x, knee);
            assert!(
                y.abs() <= knee,
                "diode_clamp({x}) = {y} exceeds the knee {knee}"
            );
            // Strictly increasing where the solve can land (the deep-dive tests
            // +-4 knees); beyond that the curve is within an ulp of the bound and
            // "non-decreasing" is all f32 can promise.
            if x.abs() <= 4.0 * knee {
                assert!(y > prev, "not strictly increasing at x={x}");
            } else {
                // Rounding noise of `x * inv` at the flat top is an ulp or so
                // either way; the solve's single root rests on the analytic
                // derivative below, which is positive everywhere.
                assert!(y >= prev - 1e-6, "not monotonic at x={x}");
            }
            assert!(
                dy > 0.0 && dy <= 1.0,
                "derivative {dy} out of (0, 1] at x={x}"
            );
            prev = y;
        }
        assert_eq!(
            diode_clamp(f32::MAX, knee).0,
            knee,
            "bounded up to f32::MAX"
        );
        assert_eq!(diode_clamp(f32::MIN, knee).0, -knee);
        assert_eq!(diode_clamp(0.0, knee).0, 0.0);

        // Odd: a saturator that is not odd adds even harmonics and DC.
        for i in 0..=300 {
            let x = i as f32 / 100.0;
            let (p, n) = (diode_clamp(x, knee).0, diode_clamp(-x, knee).0);
            assert!((p + n).abs() < 1e-6, "not odd at x={x}");
        }

        // The closed-form derivative matches a central difference. Evaluated in
        // f64 so the difference quotient is not swamped by f32 rounding.
        let f = |x: f64| x / (1.0 + (x / knee as f64).abs().powi(3)).cbrt();
        let mut worst = 0.0f32;
        for i in 0..=400 {
            let x = i as f32 / 100.0; // 0..4
            let h = 1e-5f64;
            let numeric = ((f(x as f64 + h) - f(x as f64 - h)) / (2.0 * h)) as f32;
            worst = worst.max((diode_clamp(x, knee).1 - numeric).abs());
        }
        assert!(worst < 1e-4, "worst derivative error {worst}");

        // Unity slope at zero: the small-signal loop is the ideal ladder, which
        // is what keeps the measured threshold at k = 4.
        assert!((diode_clamp(0.0, knee).1 - 1.0).abs() < 1e-6);
        assert!((diode_clamp(1e-3, knee).0 - 1e-3).abs() < 1e-9);
    }

    #[test]
    fn fixed_solver_matches_a_converged_reference_across_abrupt_boundaries() {
        fn reference_clamp(x: f64) -> f64 {
            x / (1.0 + (x / f64::from(CLAMP_KNEE)).abs().powi(3)).cbrt()
        }
        fn converged_reference(pu_a: f32, pk: f32) -> f64 {
            let pu_a = f64::from(pu_a);
            let pk = f64::from(pk);
            if pk == 0.0 {
                return pu_a;
            }
            // Since |clamp(y)| <= knee, the unique root is in this bracket.
            let radius = pk * f64::from(CLAMP_KNEE);
            let (mut lo, mut hi) = (pu_a - radius, pu_a + radius);
            for _ in 0..96 {
                let mid = 0.5 * (lo + hi);
                let residual = pu_a - pk * reference_clamp(mid) - mid;
                if residual > 0.0 {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            0.5 * (lo + hi)
        }
        fn three_steps(pu_a: f32, pk: f32, seed: f32) -> f32 {
            let mut y = seed;
            for _ in 0..3 {
                let (clamped, derivative) = diode_clamp(y, CLAMP_KNEE);
                let residual = pu_a - pk * clamped - y;
                y += residual / (1.0 + pk * derivative);
            }
            y
        }

        // These are post-drive inputs and integrator histories. Pairing every sign with every
        // prior output models hard input, cutoff and resonance changes on the next sample rather
        // than merely measuring a settled trajectory.
        let stage_histories = [
            [0.0; 4],
            [OUTPUT_BOUND; 4],
            [-OUTPUT_BOUND; 4],
            [OUTPUT_BOUND, -OUTPUT_BOUND, OUTPUT_BOUND, -OUTPUT_BOUND],
            [-OUTPUT_BOUND, OUTPUT_BOUND, -OUTPUT_BOUND, OUTPUT_BOUND],
        ];
        let mut worst_residual = 0.0f32;
        let mut worst_root_error = 0.0f64;
        let mut linear_three_step_worst_residual = 0.0f32;
        let mut stale_three_step_worst_residual = 0.0f32;
        for sample_rate in [
            crate::MIN_SAMPLE_RATE,
            1_000.0,
            44_100.0,
            48_000.0,
            96_000.0,
            192_000.0,
            768_000.0,
        ] {
            for requested_cutoff in [5.0, 20_000.0, 0.45 * sample_rate, f32::MAX] {
                let cutoff = requested_cutoff.clamp(5.0, NYQUIST_FRACTION * sample_rate);
                let g = tan_approx((PI * cutoff / sample_rate) as f64) as f32;
                let mut big_g = [0.0f32; 4];
                for (gi, trim) in big_g.iter_mut().zip(Ladder::new().trim()) {
                    let gt = g * trim;
                    *gi = gt / (1.0 + gt);
                }
                let p = big_g.iter().product::<f32>();
                for resonance in [0.0, EXCITATION_THRESHOLD, 1.0] {
                    let pk = p * K_MAX * resonance;
                    for u in [-INPUT_KNEE, 0.0, INPUT_KNEE] {
                        for states in stage_histories {
                            let state_terms = [
                                (1.0 - big_g[0]) * states[0],
                                (1.0 - big_g[1]) * states[1],
                                (1.0 - big_g[2]) * states[2],
                                (1.0 - big_g[3]) * states[3],
                            ];
                            let a = big_g[3] * big_g[2] * big_g[1] * state_terms[0]
                                + big_g[3] * big_g[2] * state_terms[1]
                                + big_g[3] * state_terms[2]
                                + state_terms[3];
                            let pu_a = p * u + a;
                            let solved = solve_feedback(pu_a, pk);
                            let residual = feedback_residual(solved, pu_a, pk).abs();
                            let reference = converged_reference(pu_a, pk);
                            worst_residual = worst_residual.max(residual);
                            worst_root_error =
                                worst_root_error.max((f64::from(solved) - reference).abs());

                            let linear_seed = pu_a / (1.0 + pk);
                            let linear_three = three_steps(pu_a, pk, linear_seed);
                            linear_three_step_worst_residual = linear_three_step_worst_residual
                                .max(feedback_residual(linear_three, pu_a, pk).abs());
                            for previous in [-OUTPUT_BOUND, 0.0, OUTPUT_BOUND] {
                                let stale_three = three_steps(pu_a, pk, previous);
                                stale_three_step_worst_residual = stale_three_step_worst_residual
                                    .max(feedback_residual(stale_three, pu_a, pk).abs());
                            }
                        }
                    }
                }
            }
        }
        eprintln!(
            "fixed solver: worst residual {worst_residual:e}, root error {worst_root_error:e}; \
             linear-seeded three-step residual {linear_three_step_worst_residual:e}; \
             stale-seeded three-step residual {stale_three_step_worst_residual:e}"
        );
        assert!(
            stale_three_step_worst_residual > 1e-3,
            "the oracle must reject the old stale-seeded three-step solve"
        );
        assert!(
            linear_three_step_worst_residual > EXCITATION_LEVEL,
            "the oracle must show why the stable seed still needs the fourth step"
        );
        assert!(
            worst_residual <= EXCITATION_LEVEL,
            "fixed solver residual {worst_residual:e} exceeded the {EXCITATION_LEVEL:e} gate"
        );
        assert!(
            worst_root_error <= EXCITATION_LEVEL as f64,
            "fixed solver differs from converged reference by {worst_root_error:e}"
        );
    }

    #[test]
    fn the_resonance_loop_is_clean_below_the_knee_and_limits_above_it() {
        // The property that separates a diode clamp from tanh. Near the
        // oscillation threshold the peak gain at cutoff is 1/(4 - k_eff), so any
        // compression of the fed-back signal is amplified into a large change of
        // gain — which makes the loop's saturator measurable from outside. The
        // clamp's small-signal compression grows with the cube of the level, tanh's
        // with the square, so: a signal that keeps the loop well under the knee
        // sees almost the tiny-signal gain, doubling it costs about eight times as
        // much rather than four, and a signal past the knee is limited hard.
        //
        // Measured at k = 3.83 — resonance 0.766 on this crate's calibrated scale,
        // the same loop gain mono-01 measures at 0.85 — where the loop dominates and
        // the input stage's own compression is negligible at every level used.
        // The numbers this crate measured, and the tanh sabotage, are in AGENTS.md;
        // the thresholds are the same as mono-01's because the loop is the same.
        //
        // On the **matched** reference, deliberately: this is a measurement of the
        // saturator, and it only reads at a known distance from the onset — the
        // peak gain there is 1/(4 − k), so a stage spread that moves the peak (by
        // 1.5 dB at this k, measured) moves the whole operating point and the
        // numbers stop being comparable with mono-01's and the deep-dive's §7.3.
        // The spread has its own test.
        let fs = 48_000.0;
        let fc = 1_000.0;
        let res = 3.83 / K_MAX;
        let gain_at = |amp: f32| {
            let mut f = Ladder::matched();
            let settle = (fs * 1.0) as usize;
            let measure = (fs / fc * 8.0) as usize + 64;
            for n in 0..settle {
                let t = n as f32 / fs;
                f.process(amp * (2.0 * PI * fc * t).sin(), fc, res, fs);
            }
            let mut peak = 0.0f32;
            for n in settle..settle + measure {
                let t = n as f32 / fs;
                peak = peak.max(
                    f.process(amp * (2.0 * PI * fc * t).sin(), fc, res, fs)
                        .abs(),
                );
            }
            peak / amp
        };
        let db = |g: f32| 20.0 * g.log10();
        let tiny = db(gain_at(1e-3));
        let low = db(gain_at(0.03));
        let mid = db(gain_at(0.06));
        let loud = db(gain_at(0.12));
        let (c_low, c_mid) = (tiny - low, tiny - mid);
        println!(
            "gain at cutoff: {tiny:.2} dB tiny, {low:.2} at 0.03, {mid:.2} at 0.06, {loud:.2} at 0.12"
        );
        assert!(
            c_low < 0.8,
            "the loop must stay clean below the knee: {c_low:.2} dB of compression at 0.03"
        );
        assert!(
            c_mid / c_low > 4.0,
            "compression must grow with the cube of the level, not the square: \
             {c_low:.2} dB at 0.03, {c_mid:.2} dB at 0.06 (ratio {:.1})",
            c_mid / c_low
        );
        assert!(
            tiny - loud > 3.0,
            "the loop must limit past the knee: {tiny:.2} dB tiny, {loud:.2} dB at 0.12"
        );
    }

    #[test]
    fn four_pole_rolloff_is_minus_12_db_at_cutoff() {
        // Each TPT one-pole is -3.01 dB at its cutoff; four in series give
        // (1/sqrt(2))^4 = 0.25.
        for fs in RATES {
            let m = magnitude_at(1_000.0, 0.0, 1_000.0, fs);
            let db = 20.0 * m.log10();
            assert!(
                (db - -12.04).abs() < 1.0,
                "at {fs} Hz: {db:.2} dB at cutoff, expected about -12.04"
            );
        }
    }

    #[test]
    fn passband_is_flat_and_stopband_rolls_off() {
        let fs = 48_000.0;
        let fc = 1_000.0;

        let passband = magnitude_at(fc, 0.0, fc / 16.0, fs);
        assert!(
            (20.0 * passband.log10()).abs() < 0.5,
            "passband not flat: {passband}"
        );

        // Two octaves above cutoff a 4-pole should be down about 48 dB.
        let two_oct = 20.0 * magnitude_at(fc, 0.0, fc * 4.0, fs).log10();
        assert!(
            (two_oct - -48.0).abs() < 6.0,
            "two octaves up: {two_oct:.1} dB, expected about -48"
        );
    }

    #[test]
    fn resonance_produces_a_peak_at_cutoff() {
        let fs = 48_000.0;
        let fc = 1_000.0;
        let flat = magnitude_at(fc, 0.0, fc, fs);
        let resonant = magnitude_at(fc, 0.7, fc, fs);
        assert!(
            resonant > flat * 2.0,
            "resonance did not lift the cutoff peak: {flat} -> {resonant}"
        );
    }

    #[test]
    fn silence_in_gives_exactly_silence_out_below_the_excitation_threshold() {
        for fs in RATES {
            let mut f = Ladder::new();
            for res in [0.0f32, 0.5, EXCITATION_THRESHOLD] {
                f.reset();
                for _ in 0..(fs as usize / 10) {
                    let y = f.process(0.0, 1_000.0, res, fs);
                    assert_eq!(y, 0.0, "resonance {res} at {fs} Hz produced {y}");
                }
            }
        }
    }

    #[test]
    fn excitation_above_the_threshold_is_bit_repeatable() {
        let fs = 48_000.0;
        let run = || {
            let mut f = Ladder::new();
            (0..4_000)
                .map(|_| f.process(0.0, 1_000.0, 1.0, fs))
                .collect::<Vec<_>>()
        };
        assert_eq!(run(), run(), "excitation must be deterministic");
        assert!(run().iter().any(|y| *y != 0.0), "no oscillation started");
    }

    #[test]
    fn stays_bounded_at_maximum_resonance_across_a_cutoff_sweep() {
        for fs in RATES {
            let mut f = Ladder::new();
            let steps = 400;
            let per_step = (fs as usize) / 100;
            let mut peak = 0.0f32;
            for i in 0..steps {
                let t = i as f32 / steps as f32;
                let cutoff = 20.0 * (20_000.0f32 / 20.0).powf(t);
                for n in 0..per_step {
                    // Drive it hard as well as resonating it.
                    let x = ((n as f32 / fs) * 2.0 * PI * 110.0).sin() * 2.0;
                    let y = f.process(x, cutoff, 1.0, fs);
                    assert!(y.is_finite(), "non-finite at {fs} Hz, cutoff {cutoff}");
                    peak = peak.max(y.abs());
                }
            }
            assert!(
                peak < OUTPUT_BOUND,
                "peak {peak} exceeded bound {OUTPUT_BOUND} at {fs} Hz"
            );
        }
    }

    #[test]
    fn no_nan_under_extreme_parameters() {
        for fs in RATES {
            let mut f = Ladder::new();
            for &cutoff in &[-1_000.0f32, 0.0, 1.0, 20.0, 20_000.0, 1e9] {
                for &res in &[-1.0f32, 0.0, 1.0, 2.0] {
                    for &amp in &[0.0f32, 1.0, 100.0, -100.0] {
                        f.reset();
                        for _ in 0..1_000 {
                            let y = f.process(amp, cutoff, res, fs);
                            assert!(
                                y.is_finite(),
                                "non-finite: fs={fs} cutoff={cutoff} res={res} amp={amp}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn self_oscillates_within_the_resonance_range() {
        for fs in RATES {
            let t = measure_oscillation_threshold(1_000.0, fs);
            assert!(
                t.is_finite() && t < 1.0,
                "no self-oscillation within range at {fs} Hz (threshold {t})"
            );
            assert!(
                t > EXCITATION_THRESHOLD - 0.35,
                "threshold {t} at {fs} Hz is suspiciously low"
            );
        }
    }

    #[test]
    fn self_oscillation_tracks_the_cutoff() {
        let fs = 48_000.0;
        for fc in [220.0f32, 440.0, 1_000.0] {
            let measured = measure_oscillation_frequency(fc, fs);
            let ratio = measured / fc;
            assert!(
                (0.5..2.0).contains(&ratio),
                "oscillation at {measured:.1} Hz for cutoff {fc} Hz (ratio {ratio:.2})"
            );
        }
    }

    /// The one hardware figure this filter has: the SH-7's adjustment procedure sets
    /// self-oscillation to start and hold between 7 and 9 on the RESONANCE slider's
    /// scale, from below 50 Hz to the top of the range
    /// (`research:instruments/sh-7.md` §§3.9, 7.5).
    /// Measured on the running filter, not derived from `K_MAX`.
    #[test]
    fn calibration_puts_self_oscillation_between_7_and_9() {
        for fs in RATES {
            // The top is the hardware's 20 kHz where the rate allows it and the DSP's
            // own ceiling where it does not — at 44.1 kHz the lower wins, a labelled
            // deviation (plan §5.3).
            let top = (20_000.0f32).min(NYQUIST_FRACTION * fs * 0.999);
            for fc in [40.0f32, 1_000.0, top] {
                let onset = measure_oscillation_threshold(fc, fs);
                assert!(
                    (0.70..=0.90).contains(&onset),
                    "onset at {onset:.3} of the control for {fc} Hz at {fs} Hz; the                      procedure requires 7 to 9 on the scale"
                );
            }
        }
    }

    #[test]
    fn no_invented_q_compensation_preserves_the_cascade_droop() {
        // No compensation path was found (`research:instruments/sh-7.md` §7.5).
        // This chosen uncompensated topology has DC gain 1/(1+k); its exact hardware
        // droop remains unverified. An invented compensation multiply makes this fail.
        let mut f = Ladder::matched();
        let input = 0.001;
        let resonance = 0.5;
        let k = resonance * K_MAX;
        let mut y = 0.0;
        for _ in 0..96_000 {
            y = f.process(input, 4_000.0, resonance, 48_000.0);
        }
        let gain = y / input;
        assert!(
            (gain - 1.0 / (1.0 + k)).abs() < 0.01,
            "DC gain {gain}, expected {}",
            1.0 / (1.0 + k)
        );
    }

    /// The four discrete stages receive one fixed chosen spread. What the test pins
    /// is the sibling experiment's finding: spread moves peak height, not onset.
    #[test]
    fn this_units_stage_spread_moves_the_peak_and_not_the_onset() {
        let fs = 48_000.0;
        let fc = 1_000.0;

        let mut unit = Ladder::new();
        let trim = unit.trim();
        assert_ne!(trim, [1.0; 4], "the shipped filter has no spread at all");
        for t in trim {
            assert!(
                (t - 1.0).abs() <= STAGE_SPREAD,
                "trim {t} outside the stated ±{STAGE_SPREAD}"
            );
        }
        assert_eq!(Ladder::matched().trim(), [1.0; 4]);
        unit.reset();
        assert_eq!(unit.trim(), trim, "reset must not change the unit");

        // The resonant peak below the onset, where a stage mismatch shows most.
        let db = |g: f32| 20.0 * g.log10();
        let res = 0.7;
        let peak_unit = db(magnitude_of(&mut Ladder::new(), fc, res, fc, fs));
        let peak_ref = db(magnitude_of(&mut Ladder::matched(), fc, res, fc, fs));
        let moved = peak_unit - peak_ref;
        println!(
            "peak at cutoff, resonance {res}: this unit {peak_unit:.2} dB, matched {peak_ref:.2} dB \
             ({moved:+.3} dB); trims {trim:?}"
        );
        assert!(
            moved.abs() > 0.02,
            "the spread must move the peak measurably: {moved:+.3} dB"
        );
        assert!(
            moved.abs() < 3.0,
            "the spread must stay small — hand-matched, not random parts: {moved:+.2} dB"
        );

        // And the onset stays where the calibration put it.
        let onset_unit = measure_oscillation_threshold(fc, fs);
        let onset_ref = measure_oscillation_threshold_of(Ladder::matched, fc, fs);
        println!("onset: this unit {onset_unit:.4}, matched {onset_ref:.4}");
        assert!(
            (onset_unit - onset_ref).abs() < 0.02,
            "the spread moved the onset: {onset_unit:.3} against the matched {onset_ref:.3}"
        );
    }

    #[test]
    fn the_input_stage_bounds_late_and_the_stages_do_not_saturate() {
        // A full-scale mixer signal at zero resonance passes with negligible
        // compression: the OTAs run linear behind their attenuators.
        let fs = 48_000.0;
        let fc = 1_000.0;
        let gain_at = |amp: f32| {
            let mut f = Ladder::new();
            let settle = (fs * 0.5) as usize;
            let measure = (fs / fc * 8.0) as usize + 64;
            for n in 0..settle {
                let t = n as f32 / fs;
                f.process(amp * (2.0 * PI * fc * t).sin(), fc, 0.0, fs);
            }
            let mut peak = 0.0f32;
            for n in settle..settle + measure {
                let t = n as f32 / fs;
                peak = peak.max(
                    f.process(amp * (2.0 * PI * fc * t).sin(), fc, 0.0, fs)
                        .abs(),
                );
            }
            peak / amp
        };
        let tiny = 20.0 * gain_at(1e-3).log10();
        let full = 20.0 * gain_at(1.0).log10();
        assert!(
            (tiny - full).abs() < 0.1,
            "full scale compressed by {:.3} dB at zero resonance; the stage must be linear there",
            tiny - full
        );
        // And it is bounded: far past full scale the stage limits.
        let huge = gain_at(100.0) * 100.0;
        assert!(
            huge <= INPUT_KNEE * 1.01,
            "input stage exceeded its bound: {huge}"
        );
    }
}
