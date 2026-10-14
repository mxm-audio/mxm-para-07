//! Framework-free DSP for mxm-para-07: two divider-derived keyed pitches into one
//! shared HPF, VCF and VCA. The machine evidence is `research:instruments/sh-7.md`.
//!
//! This implementation is original MIT-licensed code. The oscillator uses the
//! PolyBLEP technique described by Välimäki et al.; the low-pass uses Zavalishin-style
//! topology-preserving one-poles and a fixed-iteration Newton solve. No third-party
//! implementation was ported.

#[cfg(any(test, feature = "conformance"))]
pub mod conformance;
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
