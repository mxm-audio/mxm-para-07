//! Framework-free DSP for mxm-para-07: two divider-derived keyed pitches into one
//! shared HPF, VCF and VCA. The machine evidence is `research:instruments/sh-7.md`.
//!
//! This implementation is original MIT-licensed code. The oscillator uses the
//! PolyBLEP technique described by Välimäki et al.; the low-pass uses Zavalishin-style
//! topology-preserving one-poles and a fixed-iteration Newton solve. No third-party
//! implementation was ported.

#[cfg(any(test, feature = "conformance"))]
pub mod conformance;
pub mod envelope;
pub mod filter;
pub mod keyboard;
pub mod lfo;
pub mod oscillator;
pub mod routing;
pub mod sh;
pub mod voice;

/// Lowest host rate that can place the 5 Hz VCO/filter floor below the 0.45×Nyquist ceiling.
/// The mathematical boundary is 11.12 Hz; 12 Hz leaves a small explicit margin before clamp
/// bounds are formed.
pub const MIN_SAMPLE_RATE: f32 = 12.0;
/// Deterministic fallback for non-finite rates passed directly to the framework-free DSP API.
pub const DEFAULT_SAMPLE_RATE: f32 = 48_000.0;

#[inline]
pub(crate) fn safe_sample_rate(sample_rate: f32) -> f32 {
    if sample_rate.is_finite() {
        sample_rate.max(MIN_SAMPLE_RATE)
    } else {
        DEFAULT_SAMPLE_RATE
    }
}

/// Flush recursive state before it enters the denormal range. The relatively high
/// threshold is about -400 dB and is what makes exact idle silence portable.
#[inline(always)]
pub fn flush(x: f32) -> f32 {
    if x.abs() < 1e-20 { 0.0 } else { x }
}

/// Deterministic xorshift32 source used for noise and filter excitation.
#[derive(Debug, Clone)]
pub struct Rng {
    state: u32,
}

impl Rng {
    pub const fn new(seed: u32) -> Self {
        Self {
            state: if seed == 0 { 0x9E37_79B9 } else { seed },
        }
    }

    #[inline]
    pub fn next_bipolar(&mut self) -> f32 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 17;
        self.state ^= self.state << 5;
        ((self.state >> 8) as f32 / 8_388_608.0) - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flush_and_rng_keep_the_numeric_contract() {
        assert_eq!(flush(1e-30), 0.0);
        assert_eq!(flush(-1e-30), 0.0);
        assert_eq!(flush(0.25), 0.25);
        let mut a = Rng::new(0);
        let mut b = Rng::new(0);
        for _ in 0..10_000 {
            let x = a.next_bipolar();
            assert_eq!(x, b.next_bipolar());
            assert!((-1.0..1.0).contains(&x));
        }
    }
}
