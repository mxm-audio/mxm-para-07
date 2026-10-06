//! Lock-free DSP-to-editor telemetry.
//!
//! The editor consumes a max-combined/reset-on-read output peak, sticky output-clip and
//! mixer-overload warnings, atomic snapshots of the voice's explanatory values, and the sample and
//! hold's scope ring. Requests in the other direction are developer-only editor state; no DSP path
//! reads this structure.

use mxm_para_07_dsp::voice::Telemetry as VoiceTelemetry;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicUsize, Ordering};

const NO_REQUEST: u8 = u8::MAX;

/// The scope's points a second: one a millisecond, forty to a cycle of the fastest LFO (25 Hz) and
/// a clean edge on the shortest sample time (13 ms), whatever the host's sample rate.
pub const SCOPE_RATE_HZ: f32 = 1000.0;
/// How many points the scope keeps: 8.2 s, four steps at the longest sample time.
pub const SCOPE_LEN: usize = 8192;

#[derive(Debug)]
pub struct Telemetry {
    peak: AtomicU32,
    clipped: AtomicBool,
    overloaded: AtomicBool,
    /// Whether a note sounds — live or in its tail — as of the last block, for the displays that
    /// show where the voice is now.
    sounding: AtomicBool,
    sample_rate: AtomicU32,
    cutoff_hz: AtomicU32,
    vco1_hz: AtomicU32,
    vco2_hz: AtomicU32,
    /// The sample and hold's source and output, one point every [`SCOPE_RATE_HZ`]th of a second
    /// while the voice runs, as two rings. Relaxed stores of one word each; the editor reads a
    /// window that may straddle a write, which on a scope is one point of tear.
    scope_source: Box<[AtomicU32]>,
    scope_out: Box<[AtomicU32]>,
    /// How many points have been written; the next goes at this modulo [`SCOPE_LEN`].
    scope_head: AtomicUsize,
    /// Whether an editor exists to read the scope. `plugins/AGENTS.md`: visualization work stops
    /// while the editor is closed, so the audio thread fills the rings only while this is set.
    editor_open: AtomicBool,
    dev_view: AtomicU8,
    dev_disclosure: AtomicU8,
    dev_browser: AtomicU8,
    dev_theme: AtomicU8,
    /// The host tempo in force, so a synced LFO rate or sample time reads its division.
    pub tempo: mxm_tempo::TempoCell,
}

impl Default for Telemetry {
    fn default() -> Self {
        Self::new()
    }
}
impl Telemetry {
    pub fn new() -> Self {
        Self {
            peak: AtomicU32::new(0),
            clipped: AtomicBool::new(false),
            overloaded: AtomicBool::new(false),
            sounding: AtomicBool::new(false),
            sample_rate: AtomicU32::new(48_000.0f32.to_bits()),
            cutoff_hz: AtomicU32::new(0),
            vco1_hz: AtomicU32::new(0),
            vco2_hz: AtomicU32::new(0),
            scope_source: (0..SCOPE_LEN).map(|_| AtomicU32::new(0)).collect(),
            scope_out: (0..SCOPE_LEN).map(|_| AtomicU32::new(0)).collect(),
            scope_head: AtomicUsize::new(0),
            editor_open: AtomicBool::new(false),
            dev_view: AtomicU8::new(NO_REQUEST),
            dev_disclosure: AtomicU8::new(NO_REQUEST),
            dev_browser: AtomicU8::new(NO_REQUEST),
            dev_theme: AtomicU8::new(NO_REQUEST),
            tempo: mxm_tempo::TempoCell::new(),
        }
    }
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::new())
    }
    fn max_store(slot: &AtomicU32, value: f32) {
        let mut current = slot.load(Ordering::Relaxed);
        loop {
            let next = f32::from_bits(current).max(value).to_bits();
            match slot.compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(seen) => current = seen,
            }
        }
    }
    pub fn publish_block(&self, peak: f32, voice: VoiceTelemetry) {
        Self::max_store(&self.peak, peak);
        if peak >= 1.0 {
            self.clipped.store(true, Ordering::Relaxed);
        }
        if voice.overload {
            self.overloaded.store(true, Ordering::Relaxed);
        }
        self.cutoff_hz
            .store(voice.cutoff_hz.to_bits(), Ordering::Relaxed);
        self.vco1_hz
            .store(voice.vco1_hz.to_bits(), Ordering::Relaxed);
        self.vco2_hz
            .store(voice.vco2_hz.to_bits(), Ordering::Relaxed);
    }
    /// One point of the sample and hold's source and output into the scope.
    #[inline]
    pub fn push_scope(&self, source: f32, out: f32) {
        let index = self.scope_head.load(Ordering::Relaxed);
        self.scope_source[index % SCOPE_LEN].store(source.to_bits(), Ordering::Relaxed);
        self.scope_out[index % SCOPE_LEN].store(out.to_bits(), Ordering::Relaxed);
        self.scope_head
            .store(index.wrapping_add(1), Ordering::Relaxed);
    }
    /// Read once per callback: whether the scope is worth filling.
    pub fn editor_open(&self) -> bool {
        self.editor_open.load(Ordering::Relaxed)
    }
    /// The editor's lifecycle: set when it is built, cleared when it closes.
    pub fn set_editor_open(&self, open: bool) {
        self.editor_open.store(open, Ordering::Relaxed);
    }
    /// The scope's latest points, oldest first: at most `count`, and no more than have been
    /// written, so a scope that has not run yet is empty rather than a line of zeros.
    pub fn scope_recent(&self, count: usize, source: &mut Vec<f32>, out: &mut Vec<f32>) {
        source.clear();
        out.clear();
        let head = self.scope_head.load(Ordering::Relaxed);
        let len = count.min(SCOPE_LEN).min(head);
        for k in head - len..head {
            let index = k % SCOPE_LEN;
            source.push(f32::from_bits(
                self.scope_source[index].load(Ordering::Relaxed),
            ));
            out.push(f32::from_bits(
                self.scope_out[index].load(Ordering::Relaxed),
            ));
        }
    }
    pub fn publish_sounding(&self, sounding: bool) {
        self.sounding.store(sounding, Ordering::Relaxed);
    }
    pub fn sounding(&self) -> bool {
        self.sounding.load(Ordering::Relaxed)
    }
    pub fn publish_sample_rate(&self, rate: f32) {
        self.sample_rate.store(rate.to_bits(), Ordering::Relaxed);
    }
    pub fn take_peak(&self) -> f32 {
        f32::from_bits(self.peak.swap(0, Ordering::Relaxed))
    }
    pub fn clipped(&self) -> bool {
        self.clipped.load(Ordering::Relaxed)
    }
    pub fn overloaded(&self) -> bool {
        self.overloaded.load(Ordering::Relaxed)
    }
    pub fn clear_clip(&self) {
        self.clipped.store(false, Ordering::Relaxed);
    }
    pub fn clear_overload(&self) {
        self.overloaded.store(false, Ordering::Relaxed);
    }
    pub fn sample_rate(&self) -> f32 {
        f32::from_bits(self.sample_rate.load(Ordering::Relaxed))
    }
    /// The sounding cutoff and the two oscillators' sounding frequencies, in hertz.
    pub fn voice_snapshot(&self) -> [f32; 3] {
        [&self.cutoff_hz, &self.vco1_hz, &self.vco2_hz]
            .map(|v| f32::from_bits(v.load(Ordering::Relaxed)))
    }

    pub fn request_view(&self, view: u8) {
        self.dev_view
            .store(view.min(NO_REQUEST - 1), Ordering::Relaxed);
    }
    pub fn request_disclosure(&self, open: bool) {
        self.dev_disclosure.store(u8::from(open), Ordering::Relaxed);
    }
    pub fn request_browser(&self, open: bool) {
        self.dev_browser.store(u8::from(open), Ordering::Relaxed);
    }
    pub fn take_view_request(&self) -> Option<usize> {
        match self.dev_view.swap(NO_REQUEST, Ordering::Relaxed) {
            NO_REQUEST => None,
            v => Some(v as usize),
        }
    }
    pub fn take_disclosure_request(&self) -> Option<bool> {
        match self.dev_disclosure.swap(NO_REQUEST, Ordering::Relaxed) {
            NO_REQUEST => None,
            v => Some(v != 0),
        }
    }
    pub fn take_browser_request(&self) -> Option<bool> {
        match self.dev_browser.swap(NO_REQUEST, Ordering::Relaxed) {
            NO_REQUEST => None,
            v => Some(v != 0),
        }
    }
    /// A theme by index — 0 light, 1 dark, 2 system, as `mxm_ui::theme::from_index` reads it.
    pub fn request_theme(&self, theme: u8) {
        self.dev_theme
            .store(theme.min(NO_REQUEST - 1), Ordering::Relaxed);
    }
    pub fn take_theme_request(&self) -> Option<u8> {
        match self.dev_theme.swap(NO_REQUEST, Ordering::Relaxed) {
            NO_REQUEST => None,
            theme => Some(theme),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn peaks_combine_and_reset_while_warnings_latch() {
        let t = Telemetry::new();
        t.publish_block(0.4, VoiceTelemetry::default());
        let v = VoiceTelemetry {
            overload: true,
            ..VoiceTelemetry::default()
        };
        t.publish_block(1.1, v);
        assert_eq!(t.take_peak(), 1.1);
        assert_eq!(t.take_peak(), 0.0);
        assert!(t.clipped() && t.overloaded());
        t.clear_clip();
        t.clear_overload();
        assert!(!t.clipped() && !t.overloaded());
    }
    /// **The scope keeps the latest points, oldest first**, across the ring's wrap, and shows
    /// only what has been written.
    #[test]
    fn the_scope_keeps_its_latest_points_oldest_first() {
        let t = Telemetry::new();
        let (mut source, mut out) = (Vec::new(), Vec::new());
        t.scope_recent(4, &mut source, &mut out);
        assert!(
            source.is_empty() && out.is_empty(),
            "nothing has been written"
        );
        t.push_scope(0.5, -0.5);
        t.scope_recent(4, &mut source, &mut out);
        assert_eq!(
            (source.as_slice(), out.as_slice()),
            (&[0.5][..], &[-0.5][..])
        );
        for k in 0..SCOPE_LEN + 3 {
            t.push_scope(k as f32, -(k as f32));
        }
        t.scope_recent(3, &mut source, &mut out);
        let last = SCOPE_LEN as f32 + 2.0;
        assert_eq!(source, [last - 2.0, last - 1.0, last]);
        assert_eq!(out, [2.0 - last, 1.0 - last, -last]);
        t.scope_recent(usize::MAX, &mut source, &mut out);
        assert_eq!(source.len(), SCOPE_LEN, "never more than the ring holds");
        assert_eq!(source[0], last - (SCOPE_LEN - 1) as f32);
    }
    #[test]
    fn developer_requests_are_taken_once() {
        let t = Telemetry::new();
        t.request_view(2);
        t.request_disclosure(true);
        t.request_browser(false);
        assert_eq!(t.take_view_request(), Some(2));
        assert_eq!(t.take_view_request(), None);
        assert_eq!(t.take_disclosure_request(), Some(true));
        assert_eq!(t.take_browser_request(), Some(false));
        // Light is index 0: a request like any other, not the absence of one.
        t.request_theme(0);
        assert_eq!(t.take_theme_request(), Some(0));
        assert_eq!(t.take_theme_request(), None);
    }
}
