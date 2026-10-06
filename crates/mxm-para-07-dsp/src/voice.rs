//! The complete two-pitch, one-articulation voice.
//!
//! Signal order and fixed restrictions follow `research:instruments/sh-7.md` §2:
//! high/low keyboard holds -> two directional lags -> divider VCOs and sources ->
//! mixer -> passive one-pole HPF -> four-OTA low-pass -> one discrete VCA. ENV-2
//! can select the VCA and has no path to filter or PWM. The gate mux, retrigger,
//! LFO reset and auto-bend are deliberately separate lines; external/S&H selector
//! edges reach the envelope gates but do not get promoted to keyboard reset or
//! auto-bend edges (`research:instruments/sh-7.md` §§4.3–4.7).

use crate::envelope::{Adsr, Stage};
use crate::filter::{self, Ladder};
use crate::keyboard::{Keyboard, NoteId, Outcome, Owner};
use crate::lfo::{Lfo, Waveform as LfoWave};
use crate::oscillator::{Range, Vco, Waveform};
use crate::routing::{self, Graph, Routing};
use crate::sh::{SampleHold, Source as ShSource};
use crate::{Rng, flush};
use mxm_modulation::standard;

pub const DEFAULT_KEY: u8 = 60;
pub const DEFAULT_CHANNEL: u8 = 0;
/// Mixer resistor ratios, derived from 100 k feedback over 82/100/33 k inputs.
pub const BANK_MIX_GAIN: f32 = 100.0 / 82.0;
pub const RING_MIX_GAIN: f32 = 100.0 / 33.0;
/// Firm input-weight ratio, derived from cutoff 120 k over ENV-1 47 k.
pub const FILTER_ENV_WEIGHT: f32 = 120.0 / 47.0;
/// Chosen full-depth spans awaiting a unit: direct source amounts remain separate.
pub const VCO_LFO_SEMITONES: f32 = 12.0;
pub const VCO_SH_SEMITONES: f32 = 24.0;
pub const AUTO_BEND_MAX_SEMITONES: f32 = 24.0;
pub const FILTER_ENV_OCTAVES: f32 = 4.0;
pub const FILTER_MOD_OCTAVES: f32 = 4.0;
pub const FILTER_AUDIO_OCTAVES: f32 = 2.0;
/// Firm service-spec VCO bender spans (`research:instruments/sh-7.md` §3.12).
pub const BENDER_VCO_CV_MAX_SEMITONES: f32 = 15.0;
pub const BENDER_VCO_LFO_MAX_SEMITONES: f32 = 10.0;
pub const BENDER_FILTER_OCTAVES: f32 = 2.0;
/// Service-spec tuning limits (`research:instruments/sh-7.md` §§3.1, 3.4).
pub const TOTAL_TUNE_MAX_SEMITONES: f32 = 3.5;
pub const VCO2_TUNE_MAX_SEMITONES: f32 = 7.5;
/// Published envelope endpoints (`research:instruments/sh-7.md` §§3.11, 6.3).
pub const ENV_ATTACK_MAX_S: f32 = 4.0;
pub const ENV_DECAY_RELEASE_MAX_S: f32 = 8.0;
/// Chosen differentiator threshold. It is intentionally downstream of portamento;
/// sufficiently slow motion can miss it, preserving the candidate-wart seam.
pub const RETRIGGER_DELTA_SEMITONES: f64 = 0.002;
/// A conservative whole-voice bound: filter bound times the maximum summed VCA CV.
pub const OUTPUT_BOUND: f32 = filter::OUTPUT_BOUND * 3.0;
/// Chosen fixed source-rate multiplier. Eight evaluations plus box decimation materially reject
/// folded triangle/PWM/ring products while retaining the source topology and fixed realtime cost.
const SOURCE_OVERSAMPLE: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeyMode {
    #[default]
    TwoPitch,
    OnePitch,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GateSource {
    #[default]
    Host,
    SampleHold,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PortamentoMode {
    #[default]
    Normal,
    Up,
    Down,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TriggerMode {
    #[default]
    GateTrigger,
    Gate,
    Lfo,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PwmSource {
    Lfo,
    #[default]
    Manual,
    Env1,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EnvChoice {
    #[default]
    Env1,
    Env2,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FilterModSource {
    #[default]
    Lfo,
    SampleHold,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FilterAudioSource {
    #[default]
    Vco2,
    Noise,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrackSource {
    #[default]
    Keyboard,
    Pedal,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NoiseColour {
    #[default]
    White,
    Pink,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BendMode {
    Direct,
    #[default]
    Off,
    Lfo,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Activity {
    Live,
    Tailing,
    #[default]
    Inert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum PendingNoteTermination {
    #[default]
    None,
    Release,
    Choke,
}

#[derive(Debug, Clone, Copy)]
pub struct EnvelopePatch {
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
    pub trigger: TriggerMode,
}
impl Default for EnvelopePatch {
    fn default() -> Self {
        Self {
            attack: 0.005,
            decay: 0.2,
            sustain: 0.7,
            release: 0.2,
            trigger: TriggerMode::GateTrigger,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct OscPatch {
    pub range: Range,
    pub waveform: Waveform,
    /// **The target's base**, which the routes add to. The voice maps the total from 50% toward
    /// 10%. In Manual this is the width knob; in the other two positions it is zero and the knob is
    /// [`OscPatch::pwm_depth`] instead (§5).
    pub pulse_width: f32,
}
impl Default for OscPatch {
    fn default() -> Self {
        Self {
            range: Range::Feet8,
            waveform: Waveform::Saw,
            pulse_width: 0.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Patch {
    pub key_mode: KeyMode,
    pub gate_source: GateSource,
    pub portamento_s: f32,
    pub portamento_mode: PortamentoMode,
    pub total_tune_semitones: f32,
    pub vco2_tune_semitones: f32,
    pub osc1: OscPatch,
    pub osc2: OscPatch,
    pub sync: bool,
    pub register_levels: [f32; 5],
    pub bank_level: f32,
    pub osc1_level: f32,
    pub osc2_level: f32,
    pub noise_level: f32,
    /// The mixer's fifth channel: the ring modulator, VCO-1 times VCO-2.
    pub fifth_level: f32,
    pub noise_colour: NoiseColour,
    pub hpf_hz: f32,
    pub cutoff_hz: f32,
    pub resonance: f32,
    pub env1: EnvelopePatch,
    pub env2: EnvelopePatch,
    pub hold: f32,
    pub lfo_wave: LfoWave,
    pub lfo_rate_hz: f32,
    pub lfo_delay_s: f32,
    pub keyboard_trigger_lfo: bool,
    pub sh_source: ShSource,
    pub sh_sample_time_s: f32,
    pub sh_lag_s: f32,
    pub auto_bend_time_s: f32,
    pub auto_bend_up: bool,
    pub volume: f32,
}
impl Default for Patch {
    fn default() -> Self {
        Self {
            key_mode: KeyMode::TwoPitch,
            gate_source: GateSource::Host,
            portamento_s: 0.0,
            portamento_mode: PortamentoMode::Normal,
            total_tune_semitones: 0.0,
            vco2_tune_semitones: 0.07,
            osc1: OscPatch::default(),
            osc2: OscPatch {
                waveform: Waveform::Square,
                ..OscPatch::default()
            },
            sync: false,
            register_levels: [0.0; 5],
            bank_level: 0.0,
            osc1_level: 0.7,
            osc2_level: 0.0,
            noise_level: 0.0,
            fifth_level: 0.0,
            noise_colour: NoiseColour::White,
            hpf_hz: 10.0,
            cutoff_hz: 18_000.0,
            resonance: 0.0,
            env1: EnvelopePatch::default(),
            env2: EnvelopePatch::default(),
            hold: 0.0,
            lfo_wave: LfoWave::Sine,
            lfo_rate_hz: 5.0,
            lfo_delay_s: 0.0,
            keyboard_trigger_lfo: false,
            sh_source: ShSource::Random,
            sh_sample_time_s: 0.2,
            sh_lag_s: 0.0,
            auto_bend_time_s: 0.2,
            auto_bend_up: false,
            volume: 0.8,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OwnerTelemetry {
    pub voice_id: Option<i32>,
    pub channel: u8,
    pub key: u8,
}
impl Default for OwnerTelemetry {
    fn default() -> Self {
        Self {
            voice_id: None,
            channel: DEFAULT_CHANNEL,
            key: DEFAULT_KEY,
        }
    }
}
impl From<Owner> for OwnerTelemetry {
    fn from(owner: Owner) -> Self {
        Self {
            voice_id: owner.id.voice_id,
            channel: owner.id.channel,
            key: owner.id.key,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Telemetry {
    pub high_target: f32,
    pub low_target: f32,
    pub high_cv: f32,
    pub low_cv: f32,
    pub high_owner: OwnerTelemetry,
    pub low_owner: OwnerTelemetry,
    pub shared_target: bool,
    pub gate_source: GateSource,
    pub selected_gate: bool,
    pub env1_gate: bool,
    pub env2_gate: bool,
    pub lfo_reset: bool,
    pub auto_bend_started: bool,
    pub retrigger: bool,
    pub sh_clock: bool,
    /// The voltage selected at the S&H input, the drooping held voltage, and the lagged output.
    /// Editor telemetry only; none feeds back into processing.
    pub sh_source_value: f32,
    pub sh_held: f32,
    pub sh_out: f32,
    pub overload: bool,
    pub cutoff_hz: f32,
    pub vco1_hz: f32,
    pub vco2_hz: f32,
}

#[derive(Debug, Clone)]
struct Lag {
    value: f64,
    target: f64,
}
impl Lag {
    fn new(v: f64) -> Self {
        Self {
            value: v,
            target: v,
        }
    }
    fn process(&mut self, time: f32, mode: PortamentoMode, fs: f32) -> f64 {
        let delta = self.target - self.value;
        let slew = match mode {
            PortamentoMode::Normal => true,
            PortamentoMode::Up => delta > 0.0,
            PortamentoMode::Down => delta < 0.0,
        };
        if time <= 0.0 || !slew {
            self.value = self.target;
        } else {
            let c = (-1.0 / (f64::from(time.clamp(0.0, 3.0)) * f64::from(fs.max(1.0)))).exp();
            self.value = self.target + (self.value - self.target) * c;
            if (self.value - self.target).abs() < 1e-9 {
                self.value = self.target;
            }
        }
        self.value
    }
    fn settle(&mut self) {
        self.value = self.target;
    }
}

#[derive(Debug, Clone, Default)]
struct OnePole {
    state: f32,
}
impl OnePole {
    fn lp(&mut self, x: f32, hz: f32, fs: f32) -> f32 {
        let hz = hz.clamp(0.01, 0.45 * fs.max(1.0));
        let g = filter::tan_approx(std::f64::consts::PI * f64::from(hz) / f64::from(fs.max(1.0)));
        let big_g = (g / (1.0 + g)) as f32;
        let v = big_g * (x - self.state);
        let y = v + self.state;
        self.state = flush(y + v);
        y
    }
    fn hp(&mut self, x: f32, hz: f32, fs: f32) -> f32 {
        x - self.lp(x, hz, fs)
    }
    fn reset(&mut self) {
        self.state = 0.0;
    }
}

#[derive(Debug, Clone, Copy)]
struct SourceConfig {
    osc1: OscPatch,
    osc2: OscPatch,
    hz1: f32,
    hz2: f32,
    width1: f32,
    width2: f32,
    sync: bool,
    ring_active: bool,
}

#[derive(Debug, Clone, Copy, Default)]
struct SourceFrame {
    osc1: f32,
    osc2: f32,
    registers: [f32; 5],
    ring: f32,
}

fn register_bank_mix(registers: &[f32; 5], raw_levels: [f32; 5]) -> f32 {
    let levels = raw_levels.map(|level| level.clamp(0.0, 1.0));
    let loading: f32 = levels.iter().sum();
    let weighted = registers
        .iter()
        .zip(levels)
        .map(|(sample, level)| sample * level)
        .sum::<f32>();
    // Topology-derived approximation of the dual-gang network: one standing ladder load plus
    // loading proportional to the aggregate slider opening. The factor of two calibrates one
    // fully raised register to unity. Unlike division by `loading`, this remains continuous at
    // silence and lets host smoothing attenuate a register as its slider approaches zero.
    weighted * 2.0 / (1.0 + loading)
}

#[derive(Debug, Clone)]
pub struct Voice {
    sample_rate: f32,
    keyboard: Keyboard,
    high_lag: Lag,
    low_lag: Lag,
    previous_high_cv: f64,
    vco1: Vco,
    vco2: Vco,
    lfo: Lfo,
    sh: SampleHold,
    env1: Adsr,
    env2: Adsr,
    filter: Ladder,
    hpf: OnePole,
    pink_a: OnePole,
    pink_b: OnePole,
    rng: Rng,
    bends: [f32; 16],
    pedals: [f32; 16],
    /// The mod wheel and channel pressure, **retained per channel** like the bend and the pedal, so
    /// a note started while the wheel is already up inherits it rather than beginning at zero.
    /// Velocity is not here: it belongs to the press and lives in the ledger's `Owner`.
    wheels: [f32; 16],
    pressures: [f32; 16],
    env_gate: [bool; 2],
    /// The velocity of **the press that last triggered an envelope** — the Velocity source, by the
    /// modulation standard. Latched where the envelopes trigger; full before any press, so the
    /// source rests at zero.
    envelope_velocity: f32,
    trigger_pulse_release: [bool; 2],
    pending_keyboard_gate_assertion: bool,
    pending_keyboard_depression: bool,
    retrigger_armed: bool,
    pending_external_trigger: bool,
    pending_note_termination: PendingNoteTermination,
    trigger_input_high: bool,
    auto_bend: f32,
    panic_latched: bool,
    idle_settled: bool,
    previous_gate_source: GateSource,
    previous_hold_open: bool,
    /// The source frame and the compacted live lists, rebuilt once per processing interval.
    graph: Graph,
    /// **The routing grid the voice owns**, adopted by [`Voice::set_topology`].
    ///
    /// It does not travel in [`Patch`], which is rebuilt every sample and is `Clone`: a 162-float
    /// grid carried there would be a memcpy per sample for values that change on a parameter event.
    /// A caller that advances route amounts per sample does it in place through
    /// [`Voice::routing_mut`], which is the shape `mxm-mono-pr1` already carries.
    routing: Routing,
    telemetry: Telemetry,
}

impl Default for Voice {
    fn default() -> Self {
        Self::new(48_000.0)
    }
}
impl Voice {
    pub fn new(sample_rate: f32) -> Self {
        let fs = crate::safe_sample_rate(sample_rate);
        let mut a = Adsr::new();
        a.set_sample_rate(fs);
        let mut b = Adsr::new();
        b.set_sample_rate(fs);
        Self {
            sample_rate: fs,
            keyboard: Keyboard::new(DEFAULT_KEY, DEFAULT_CHANNEL),
            high_lag: Lag::new(DEFAULT_KEY as f64),
            low_lag: Lag::new(DEFAULT_KEY as f64),
            previous_high_cv: DEFAULT_KEY as f64,
            vco1: Vco::new(fs),
            vco2: Vco::new(fs),
            lfo: Lfo::new(),
            sh: SampleHold::new(),
            env1: a,
            env2: b,
            filter: Ladder::new(),
            hpf: OnePole::default(),
            pink_a: OnePole::default(),
            pink_b: OnePole::default(),
            rng: Rng::new(0x0007_1978),
            bends: [0.0; 16],
            pedals: [0.0; 16],
            wheels: [0.0; 16],
            pressures: [0.0; 16],
            env_gate: [false; 2],
            envelope_velocity: 1.0,
            trigger_pulse_release: [false; 2],
            pending_keyboard_gate_assertion: false,
            pending_keyboard_depression: false,
            retrigger_armed: false,
            pending_external_trigger: false,
            pending_note_termination: PendingNoteTermination::None,
            trigger_input_high: false,
            auto_bend: 0.0,
            panic_latched: false,
            idle_settled: false,
            previous_gate_source: GateSource::Host,
            previous_hold_open: false,
            // **Empty, not the init patch, and the two match.** An unarmed voice renders no
            // modulation at all, which is loudly wrong; had it defaulted to `Routing::init()` a
            // caller that forgot `set_topology` would quietly render the init patch over somebody
            // else's parameters. The sampler's conversion shipped a voice that was never armed.
            graph: Graph::new(),
            routing: Routing::new(),
            telemetry: Telemetry::default(),
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = crate::safe_sample_rate(sample_rate);
        self.vco1.set_sample_rate(self.sample_rate);
        self.vco2.set_sample_rate(self.sample_rate);
        self.env1.set_sample_rate(self.sample_rate);
        self.env2.set_sample_rate(self.sample_rate);
    }
    pub fn telemetry(&self) -> Telemetry {
        self.telemetry
    }
    pub fn assignment(&self) -> crate::keyboard::Assignment {
        self.keyboard.assignment()
    }
    pub fn env_stage(&self, env: usize) -> Stage {
        if env == 0 {
            self.env1.stage()
        } else {
            self.env2.stage()
        }
    }

    fn sync_targets(&mut self, outcome: Outcome, was_held: bool) {
        let previous_high_target = self.high_lag.target;
        let a = self.keyboard.assignment();
        self.high_lag.target = f64::from(a.high.unwrap().id.key);
        self.low_lag.target = f64::from(a.low.unwrap().id.key);
        // Owner identity, channel and per-note tuning may change while the keyboard target does
        // not. Only a different high-key CV target can arm the downstream pitch differentiator.
        if was_held
            && self.high_lag.target != previous_high_target
            && (self.high_lag.target - self.high_lag.value).abs() > f64::EPSILON
        {
            self.retrigger_armed = true;
        }
        if outcome.gate_opened {
            self.pending_keyboard_gate_assertion = true;
            self.panic_latched = false;
        }
        if outcome.cut {
            // The press ledger does not know which gate the live patch selects. Defer the cut so
            // an independently clocked S&H articulation is not touched by a host note.
            self.pending_note_termination = PendingNoteTermination::Choke;
            self.pending_keyboard_gate_assertion = false;
            self.pending_keyboard_depression = false;
            self.retrigger_armed = false;
        }
    }

    /// A key went down, at a velocity the ledger keeps as a routable source. **New MIDI path**: the
    /// machine's keyboard read no velocity, so nothing but a route can hear it.
    pub fn note_on(&mut self, id: NoteId, velocity: f32) -> bool {
        let was = self.keyboard.is_held();
        let was_panicked = self.panic_latched;
        let o = self.keyboard.note_on(
            id,
            if velocity.is_finite() {
                velocity.clamp(0.0, 1.0)
            } else {
                0.0
            },
        );
        if o.accepted {
            self.sync_targets(o, was);
            // A physical keyboard depression is not the shared gate's low-to-high assertion:
            // second, middle and duplicate keys still reach KYBD TRIG while the gate stays high.
            self.pending_keyboard_depression = true;
            // A note arriving after panic is the explicit wake event even when old
            // held presses kept the shared hardware gate logically high.
            if was_panicked {
                self.panic_latched = false;
                self.pending_keyboard_gate_assertion = true;
            }
        }
        o.accepted
    }
    pub fn note_off(&mut self, voice_id: Option<i32>, channel: u8, key: u8) -> bool {
        let was = self.keyboard.is_held();
        let o = self.keyboard.note_off(voice_id, channel, key);
        if o.accepted {
            self.sync_targets(o, was);
        }
        o.accepted
    }
    pub fn choke(&mut self, voice_id: Option<i32>, channel: u8, key: u8) -> bool {
        let was = self.keyboard.is_held();
        let o = self.keyboard.choke(voice_id, channel, key);
        if o.accepted {
            self.sync_targets(o, was);
        }
        o.accepted
    }
    pub fn all_notes_off(&mut self) {
        let o = self.keyboard.all_notes_off();
        self.pending_keyboard_gate_assertion = false;
        self.pending_keyboard_depression = false;
        self.retrigger_armed = false;
        if o.gate_closed {
            // As with final Choke, only the selected Host gate may articulate this release.
            self.pending_note_termination = PendingNoteTermination::Release;
        }
    }
    pub fn set_poly_tuning(
        &mut self,
        voice_id: Option<i32>,
        channel: u8,
        key: u8,
        semitones: f32,
    ) -> bool {
        if !semitones.is_finite() {
            return false;
        }
        self.keyboard
            .set_tuning(voice_id, channel, key, semitones.clamp(-48.0, 48.0))
    }
    pub fn set_pitch_bend(&mut self, channel: u8, normalised: f32) {
        if normalised.is_finite() {
            self.bends[(channel & 15) as usize] = normalised.clamp(-1.0, 1.0);
        }
    }
    pub fn set_expression(&mut self, channel: u8, normalised: f32) {
        if normalised.is_finite() {
            self.pedals[(channel & 15) as usize] = normalised.clamp(0.0, 1.0);
        }
    }
    /// CC 1. **New MIDI path**: this machine had no mod wheel, so the value is kept only because a
    /// route can read it, and it starts at zero depth on every target.
    pub fn set_mod_wheel(&mut self, channel: u8, normalised: f32) {
        if normalised.is_finite() {
            self.wheels[(channel & 15) as usize] = normalised.clamp(0.0, 1.0);
        }
    }
    /// Channel pressure. **New MIDI path**, retained per channel whether or not a note sounds.
    pub fn set_channel_pressure(&mut self, channel: u8, normalised: f32) {
        if normalised.is_finite() {
            self.pressures[(channel & 15) as usize] = normalised.clamp(0.0, 1.0);
        }
    }
    pub fn restore_trigger_input(&mut self, high: bool) {
        self.trigger_input_high = high;
    }
    pub fn set_trigger_input(&mut self, high: bool) {
        if high && !self.trigger_input_high {
            self.pending_external_trigger = true;
            self.panic_latched = false;
        }
        self.trigger_input_high = high;
    }

    pub fn all_sound_off(&mut self) {
        self.env1.silence();
        self.env2.silence();
        self.env_gate = [false; 2];
        self.trigger_pulse_release = [false; 2];
        self.high_lag.settle();
        self.low_lag.settle();
        self.previous_high_cv = self.high_lag.value;
        self.vco1.reset();
        self.vco2.reset();
        self.lfo.reset();
        self.sh.reset();
        self.filter.reset();
        self.hpf.reset();
        self.pink_a.reset();
        self.pink_b.reset();
        self.rng = Rng::new(0x0007_1978);
        self.auto_bend = 0.0;
        self.pending_keyboard_gate_assertion = false;
        self.pending_keyboard_depression = false;
        self.pending_external_trigger = false;
        self.pending_note_termination = PendingNoteTermination::None;
        self.retrigger_armed = false;
        self.panic_latched = true;
        self.idle_settled = false;
    }
    /// Adopt a routing grid and rebuild which routes are live.
    ///
    /// **Once per processing interval, never per sample**, and every path that can reach
    /// [`Voice::process`] owes it: `mxm-creative-sampler`'s conversion shipped a voice that was
    /// never armed with a topology, so the first note after any allocation rendered with nothing
    /// routed.
    pub fn set_topology(&mut self, routing: &Routing) {
        self.routing = *routing;
        self.graph.set_topology(&self.routing);
    }

    /// The live amounts, for a caller advancing them in place each sample.
    ///
    /// Presence is not meant to change through here — it decides the compacted live lists, which
    /// [`Voice::set_topology`] rebuilds — so a caller that moves a presence and not the topology
    /// gets a grid the sums do not walk.
    #[inline]
    pub fn routing_mut(&mut self) -> &mut Routing {
        &mut self.routing
    }

    /// The routing grid, as it stands.
    #[inline]
    #[must_use]
    /// What the frame holds for `source`, **in raw units**, for tests.
    #[cfg(test)]
    pub(crate) fn published_for_test(&self, source: usize) -> f32 {
        self.graph.read(source) * routing::FRAME_SCALE
    }

    pub fn routing(&self) -> &Routing {
        &self.routing
    }

    pub fn reset(&mut self) {
        let fs = self.sample_rate;
        *self = Self::new(fs);
    }

    fn owner_for(&self, mode: KeyMode, high: bool) -> Owner {
        let a = self.keyboard.assignment();
        match mode {
            KeyMode::OnePitch => a.high.unwrap(),
            KeyMode::TwoPitch => {
                if high {
                    a.high.unwrap()
                } else {
                    a.low.unwrap()
                }
            }
        }
    }
    /// The press the shared, per-channel controls follow: the high key, in either mode.
    fn shared_owner(&self) -> Owner {
        self.keyboard.assignment().high.unwrap()
    }
    fn bend_value(&self) -> f32 {
        self.bends[(self.shared_owner().id.channel & 15) as usize]
    }

    /// Whether a route exists at a depth that can carry anything.
    ///
    /// **A route at zero depth carries nothing** (mxm-kit's `docs/code-review-notes.md` §7): it
    /// neither keeps the plugin live nor owns a tail.
    fn reaches(&self, target: usize, source: usize) -> bool {
        self.routing.present[target][source] && self.routing.amounts[target][source] != 0.0
    }

    /// The current value of an event-driven source, for §6's *parked non-neutral* test.
    ///
    /// Read from the voice's own state rather than the frame, because `activity` is asked before
    /// the frame is published on some paths and would otherwise be answering with last sample's
    /// values. For a parked control that is the same number; for a guarantee about silence it is
    /// not a distinction worth resting on.
    fn parked_value(&self, source: usize) -> f32 {
        let owner = self.shared_owner();
        let channel = (owner.id.channel & 15) as usize;
        match source {
            // Neutral is middle C, not zero: this is the same expression the Key source publishes.
            routing::source::KEY => (self.previous_high_cv as f32 - 60.0) / 12.0,
            // **Velocity cannot open the VCA**: it is `v − 1`, never positive, and VCA level offers
            // it only a positive amount, so a route from it can only close the amplifier.
            routing::source::VELOCITY => {
                let _ = owner;
                0.0
            }
            routing::source::WHEEL => self.wheels[channel],
            routing::source::PRESSURE => self.pressures[channel],
            routing::source::BEND => self.bends[channel],
            routing::source::BEND_MAGNITUDE => self.bends[channel].abs(),
            routing::source::PEDAL => self.pedals[channel],
            routing::source::AUTO_BEND => self.auto_bend,
            _ => 0.0,
        }
    }

    /// Whether a source routed into the amplifier can sound with no key held (§6).
    fn source_can_sound(&self, source: usize) -> bool {
        match routing::SUSTAIN[source] {
            routing::Sustain::FreeRunning => true,
            routing::Sustain::Parked => self.parked_value(source) != 0.0,
            // An envelope owns a tail, not liveness.
            routing::Sustain::Envelope => false,
            // The module is live exactly while its own factors are. It cannot recur further: the
            // multiplier is skipped as a factor of itself, so this descends one level and stops.
            routing::Sustain::Module => (0..routing::SOURCES).any(|factor| {
                factor != routing::source::MULTIPLIER
                    && self.reaches(routing::target::MULTIPLIER, factor)
                    && self.source_can_sound(factor)
            }),
        }
    }

    fn envelope_is_active(&self, source: usize) -> bool {
        match source {
            routing::source::ENVELOPE_1 => self.env1.is_active(),
            routing::source::ENVELOPE_2 => self.env2.is_active(),
            _ => false,
        }
    }

    pub fn activity(&self, p: &Patch) -> Activity {
        // Panic wins over presses retained in the keyboard ledger. Those presses still exist so
        // their later releases match, but they must neither keep the host awake nor reopen a gate.
        if self.panic_latched {
            return Activity::Inert;
        }
        if self.keyboard.is_held() {
            return Activity::Live;
        }
        // **Generalised over the amplitude target's live routes** (§6), replacing the hand-written
        // `vca_lfo_amount > 0` and bend clauses. HOLD and the S&H gate stay independent: neither is
        // a route, and HOLD is the target's own base.
        let autonomous = p.gate_source == GateSource::SampleHold
            || p.hold > 0.0
            || (0..routing::SOURCES).any(|source| {
                self.reaches(routing::target::VCA_LEVEL, source) && self.source_can_sound(source)
            });
        let tailing = (0..routing::SOURCES).any(|source| {
            self.reaches(routing::target::VCA_LEVEL, source) && self.envelope_is_active(source)
        });
        if autonomous && p.volume > 0.0 {
            Activity::Live
        } else if tailing && p.volume > 0.0 {
            Activity::Tailing
        } else {
            Activity::Inert
        }
    }

    /// Remaining audible envelope tail: **the longest release among the envelopes routed into the
    /// amplifier** (§6).
    ///
    /// An envelope that reaches no amplitude route cannot open the VCA and therefore must not keep
    /// the host awake merely because its private release is still running. That was true of the
    /// *unselected* envelope before the conversion and is true of an *unrouted* one now — the same
    /// guarantee, stated over routes rather than over a selector.
    pub fn tail_samples(&self, p: &Patch) -> u32 {
        if p.volume <= 0.0 {
            return 0;
        }
        let mut longest = 0;
        for (source, release) in [
            (routing::source::ENVELOPE_1, p.env1.release),
            (routing::source::ENVELOPE_2, p.env2.release),
        ] {
            if !self.reaches(routing::target::VCA_LEVEL, source) {
                continue;
            }
            let envelope = if source == routing::source::ENVELOPE_1 {
                &self.env1
            } else {
                &self.env2
            };
            longest =
                longest.max(envelope.tail_samples(release.clamp(0.0, ENV_DECAY_RELEASE_MAX_S)));
        }
        longest
    }

    fn render_sources_with_factor(&mut self, config: SourceConfig, factor: usize) -> SourceFrame {
        let factor = factor.clamp(1, 128);
        let fraction = 1.0 / factor as f64;
        let rate_scale = 1.0 / factor as f32;
        let mut out = SourceFrame::default();
        for _ in 0..factor {
            let first = self.vco1.render_at_rate(
                config.osc1.range,
                config.osc1.waveform,
                config.width1,
                rate_scale,
            );
            let second = self.vco2.render_at_rate(
                config.osc2.range,
                config.osc2.waveform,
                config.width2,
                rate_scale,
            );
            let registers = self.vco1.registers_at_rate(rate_scale);

            out.osc1 += first;
            out.osc2 += second;
            out.ring += first * second;
            for (sum, sample) in out.registers.iter_mut().zip(registers) {
                *sum += sample;
            }

            let edges = self.vco1.advance_master_fraction(config.hz1, fraction);
            self.vco2
                .advance_slave_fraction(config.hz2, edges, config.sync, fraction);
        }
        let scale = rate_scale;
        out.osc1 *= scale;
        out.osc2 *= scale;
        out.ring *= scale;
        for sample in &mut out.registers {
            *sample *= scale;
        }
        out
    }

    fn render_sources(&mut self, config: SourceConfig) -> SourceFrame {
        let nonlinear_wave =
            |patch: OscPatch| matches!(patch.waveform, Waveform::Triangle | Waveform::Pulse);
        let factor =
            if nonlinear_wave(config.osc1) || nonlinear_wave(config.osc2) || config.ring_active {
                SOURCE_OVERSAMPLE
            } else {
                1
            };
        self.render_sources_with_factor(config, factor)
    }

    fn settle_idle(&mut self) {
        if self.idle_settled {
            return;
        }
        // The host may stop calling us as soon as the selected VCA envelope finishes. Silence both
        // envelopes at that transition so changing the selector after an arbitrary sleep cannot
        // reconnect a release frozen at its old level.
        self.env1.silence();
        self.env2.silence();
        self.high_lag.settle();
        self.low_lag.settle();
        self.lfo.settle();
        self.sh.settle();
        self.filter.reset();
        self.hpf.reset();
        self.idle_settled = true;
    }

    #[inline]
    pub fn process(&mut self, p: &Patch) -> f32 {
        self.telemetry = Telemetry::default();
        let hold_open = p.hold > 0.0;
        if (!self.previous_hold_open && hold_open)
            || (self.previous_gate_source != GateSource::SampleHold
                && p.gate_source == GateSource::SampleHold)
        {
            self.panic_latched = false;
        }
        self.previous_hold_open = hold_open;
        // Selector history still advances while panicked: leaving and then re-entering the S&H
        // gate is itself one of the explicit wake gestures.
        self.previous_gate_source = p.gate_source;
        // `all_sound_off` deliberately keeps the press ledger for event matching. Bail out before
        // gate evaluation or those retained presses would immediately retrigger the envelopes.
        if self.panic_latched {
            self.settle_idle();
            return 0.0;
        }

        // Opens the frame for this sample. A panicked or inert sample publishes nothing and returns
        // before here or below; the frame then holds each source's last value, which matches the
        // generators that also did not advance.
        self.graph.begin_sample();

        let high = self
            .high_lag
            .process(p.portamento_s, p.portamento_mode, self.sample_rate);
        let low = self
            .low_lag
            .process(p.portamento_s, p.portamento_mode, self.sample_rate);
        self.telemetry.high_target = self.high_lag.target as f32;
        self.telemetry.low_target = self.low_lag.target as f32;
        self.telemetry.high_cv = high as f32;
        self.telemetry.low_cv = low as f32;
        let assignment = self.keyboard.assignment();
        self.telemetry.high_owner = assignment.high.unwrap().into();
        self.telemetry.low_owner = assignment.low.unwrap().into();
        self.telemetry.shared_target = p.key_mode != KeyMode::TwoPitch
            || self.high_lag.target.to_bits() == self.low_lag.target.to_bits();

        if self.retrigger_armed {
            // The detector reads the post-portamento CV, not the target or host event.
            // A sufficiently slow RC can remain below the chosen threshold, preserving
            // the candidate delayed/suppressed-retrigger seam without claiming it as fact.
            if (high - self.previous_high_cv).abs() > RETRIGGER_DELTA_SEMITONES {
                self.pending_external_trigger = true;
                self.retrigger_armed = false;
                self.telemetry.retrigger = true;
            }
        }
        self.previous_high_cv = high;
        // **Key is the high lane's post-portamento CV in octaves**, the machine's KYBD line, and
        // the same expression the legacy filter tracking uses further down. Per-note tuning and the
        // lever stay out of it.
        self.graph.write(
            routing::source::KEY,
            standard::key(high as f32, routing::KEY_UNIT_SEMITONES),
        );

        let keyboard_gate_assertion = core::mem::take(&mut self.pending_keyboard_gate_assertion);
        let keyboard_depression = core::mem::take(&mut self.pending_keyboard_depression);
        let force_lfo = p.env1.trigger == TriggerMode::Lfo || p.env2.trigger == TriggerMode::Lfo;
        if keyboard_gate_assertion {
            // Only the shared gate's low-to-high assertion dumps the sine-delay capacitor.
            self.lfo.keyboard_gate(p.lfo_delay_s);
        }
        if keyboard_depression && (p.keyboard_trigger_lfo || force_lfo) {
            // KYBD TRIG is a key-depression line, so legato extremes, middle keys and duplicate
            // presses restart the LFO even though none asserts the already-high shared gate.
            self.lfo.keyboard_restart();
            self.telemetry.lfo_reset = true;
        }
        if keyboard_gate_assertion {
            self.auto_bend = 1.0;
            self.telemetry.auto_bend_started = true;
        }

        let raw_noise = self.rng.next_bipolar();
        // Firm passive approximation, loaded response unmeasured: two weighted
        // shelves use the printed parts' nominal 2.34 and 10.3 kHz corners. Keeping
        // a direct term avoids falsely turning the passive network into two
        // cascaded integrators with a 12 dB/octave terminal slope.
        let pink_fast = self.pink_a.lp(raw_noise, 10_300.0, self.sample_rate);
        let pink_slow = self.pink_b.lp(raw_noise, 2_340.0, self.sample_rate);
        let pink = (0.18 * raw_noise + 0.35 * pink_fast + 0.47 * pink_slow) * 1.25;
        let noise = if p.noise_colour == NoiseColour::White {
            raw_noise
        } else {
            pink
        };
        // The selected colour, as the mixer hears it — not the raw generator.
        self.graph.write(routing::source::NOISE, noise);
        let lfo = self
            .lfo
            .process(p.lfo_rate_hz, p.lfo_wave, p.lfo_delay_s, self.sample_rate);
        // **Three outputs, three sources** (D1). PLUS is what the VCO and VCF saw, 0-CENTER what the
        // VCA and the bender multiplier saw, and the undelayed triangle is the PWM line. One source
        // at PLUS would put a DC offset into the VCA and PWM that the machine never had there.
        self.graph.write(routing::source::LFO, lfo.plus);
        self.graph
            .write(routing::source::LFO_CENTRED, lfo.zero_center);
        self.graph
            .write(routing::source::LFO_TRIANGLE, lfo.pwm_triangle);
        let sh_source_value = match p.sh_source {
            ShSource::Saw => lfo.saw,
            ShSource::Triangle => lfo.triangle_bipolar,
            ShSource::Random => raw_noise,
        };
        let sh = self.sh.process(
            p.sh_sample_time_s,
            p.sh_lag_s,
            p.sh_source,
            lfo.saw,
            lfo.triangle_bipolar,
            raw_noise,
            self.sample_rate,
        );
        self.telemetry.sh_clock = sh.clock_high;
        self.telemetry.sh_source_value = sh_source_value;
        self.telemetry.sh_held = sh.held;
        self.telemetry.sh_out = sh.out;
        self.graph.write(routing::source::SAMPLE_HOLD, sh.out);

        // **The gate source is chosen in either key mode** (the owner, 2026-09-27): the machine
        // reached the S&H clock only through EXT CV/GATE's empty jack, a mode the plug-in retired.
        let (selected_gate_source, base_gate) = match p.gate_source {
            GateSource::Host => (GateSource::Host, self.keyboard.is_held()),
            GateSource::SampleHold => (GateSource::SampleHold, sh.clock_high),
        };
        self.telemetry.gate_source = selected_gate_source;
        self.telemetry.selected_gate = base_gate;
        let note_termination = core::mem::take(&mut self.pending_note_termination);
        if selected_gate_source == GateSource::Host {
            match note_termination {
                PendingNoteTermination::None => {}
                PendingNoteTermination::Release => {
                    self.env1.release();
                    self.env2.release();
                    self.env_gate = [false; 2];
                }
                PendingNoteTermination::Choke => {
                    self.env1.silence();
                    self.env2.silence();
                    self.env_gate = [false; 2];
                    self.trigger_pulse_release = [false; 2];
                }
            }
        }
        let modes = [p.env1.trigger, p.env2.trigger];
        for (i, release_pulse) in self.trigger_pulse_release.iter_mut().enumerate() {
            if core::mem::take(release_pulse) {
                if i == 0 {
                    self.env1.release();
                } else {
                    self.env2.release();
                }
            }
        }
        let triggering_velocity = self.shared_owner().velocity;
        for (i, mode) in modes.into_iter().enumerate() {
            let gate = base_gate && (mode != TriggerMode::Lfo || lfo.square_high);
            if gate && !self.env_gate[i] {
                if i == 0 {
                    self.env1.trigger()
                } else {
                    self.env2.trigger()
                }
                self.envelope_velocity = triggering_velocity;
            }
            if !gate && self.env_gate[i] {
                if i == 0 {
                    self.env1.release()
                } else {
                    self.env2.release()
                }
            }
            self.env_gate[i] = gate;
        }
        let retrig = core::mem::take(&mut self.pending_external_trigger);
        if retrig {
            if p.env1.trigger == TriggerMode::GateTrigger {
                self.env1.trigger();
                self.trigger_pulse_release[0] = !self.env_gate[0];
                self.envelope_velocity = triggering_velocity;
            }
            if p.env2.trigger == TriggerMode::GateTrigger {
                self.env2.trigger();
                self.trigger_pulse_release[1] = !self.env_gate[1];
                self.envelope_velocity = triggering_velocity;
            }
            self.telemetry.retrigger = true;
        }
        self.telemetry.env1_gate = self.env_gate[0];
        self.telemetry.env2_gate = self.env_gate[1];

        if self.activity(p) == Activity::Inert {
            self.settle_idle();
            return 0.0;
        }
        self.idle_settled = false;

        let e1 = self.env1.process(
            p.env1.attack.clamp(0.0, ENV_ATTACK_MAX_S),
            p.env1.decay.clamp(0.0, ENV_DECAY_RELEASE_MAX_S),
            p.env1.sustain,
            p.env1.release.clamp(0.0, ENV_DECAY_RELEASE_MAX_S),
        );
        let e2 = self.env2.process(
            p.env2.attack.clamp(0.0, ENV_ATTACK_MAX_S),
            p.env2.decay.clamp(0.0, ENV_DECAY_RELEASE_MAX_S),
            p.env2.sustain,
            p.env2.release.clamp(0.0, ENV_DECAY_RELEASE_MAX_S),
        );
        self.graph.write(routing::source::ENVELOPE_1, e1);
        self.graph.write(routing::source::ENVELOPE_2, e2);
        // An envelope routed to the amplifier can become idle on this sample. Settle now, before
        // the plugin reports `Sleep`; there may be no next process call on which to clear an
        // envelope that reaches no amplitude route.
        if self.activity(p) == Activity::Inert {
            self.settle_idle();
            return 0.0;
        }
        let tau = (p.auto_bend_time_s.clamp(0.02, 0.7) / 4.605_170_2).max(1e-6);
        self.auto_bend = flush(
            self.auto_bend
                * (-1.0f64 / (f64::from(tau) * f64::from(self.sample_rate))).exp() as f32,
        );
        let auto = self.auto_bend * if p.auto_bend_up { -1.0 } else { 1.0 };
        let bend = self.bend_value();
        let expression_owner = self.shared_owner();
        let expression_channel = (expression_owner.id.channel & 15) as usize;
        let pedal = self.pedals[expression_channel];
        self.graph.write(routing::source::AUTO_BEND, auto);
        // **The three new MIDI paths** (§3.1). Velocity belongs to the press, so it follows the
        // retained owner and a handoff cannot borrow a released press's; the wheel and pressure are
        // retained per channel. All three publish through `write` like every other source, which is
        // what applies `FRAME_UNIT` — and all three sit at zero depth on every target at Init, so a
        // fresh instance still sounds exactly like the machine that had none of them.
        // Through the collection's standard, each zero at its rest: Velocity is the press that last
        // triggered an envelope, `v − 1`.
        self.graph.write(
            routing::source::VELOCITY,
            standard::velocity(self.envelope_velocity),
        );
        self.graph.write(
            routing::source::WHEEL,
            standard::wheel(self.wheels[expression_channel]),
        );
        self.graph.write(
            routing::source::PRESSURE,
            standard::pressure(self.pressures[expression_channel]),
        );
        self.graph
            .write(routing::source::BEND, standard::bend(bend));
        // The multiplier's rectified control side, which is what the CA3080 sees (§3.4). Published
        // as its own source so a route can read the lever's magnitude without its direction.
        self.graph
            .write(routing::source::BEND_MAGNITUDE, bend.abs());
        // **The pedal is read here rather than at the filter.** It is a stored per-channel value,
        // not something computed, so reading it earlier costs nothing and keeps publication order
        // the declared source order.
        self.graph
            .write(routing::source::PEDAL, standard::wheel(pedal));
        // **The module is evaluated once every source it can read has published**, and its result
        // publishes as a source like any other (D4). `product` returns raw domain, so `write` is
        // what puts it into the frame; `write_in_frame_units` would make it eight times too large.
        let multiplier = self.graph.product(&self.routing);
        self.graph.write(routing::source::MULTIPLIER, multiplier);

        let high_owner = self.owner_for(p.key_mode, true);
        let low_owner = self.owner_for(p.key_mode, false);
        let (base1, base2) = match p.key_mode {
            KeyMode::TwoPitch => (high, low),
            KeyMode::OnePitch => (high, high),
        };
        let total_tune = p
            .total_tune_semitones
            .clamp(-TOTAL_TUNE_MAX_SEMITONES, TOTAL_TUNE_MAX_SEMITONES);
        let vco2_tune = p
            .vco2_tune_semitones
            .clamp(-VCO2_TUNE_MAX_SEMITONES, VCO2_TUNE_MAX_SEMITONES);
        // **The lane's base, then its routes.** Each lane's post-portamento CV, per-note tuning and
        // the two tune controls stay exactly where they were; routing adds on top. The bender's
        // term is a route now too — from the lever in Direct mode and from the multiplier in LFO
        // mode, which is why those carry different scale columns.
        let pitch1 = base1 as f32
            + high_owner.tuning_semitones
            + total_tune
            + self.graph.sum(routing::target::VCO_1_PITCH, &self.routing);
        let pitch2 = base2 as f32
            + low_owner.tuning_semitones
            + total_tune
            + vco2_tune
            + self.graph.sum(routing::target::VCO_2_PITCH, &self.routing);
        let hz1 = midi_hz(pitch1);
        let hz2 = midi_hz(pitch2);
        self.telemetry.vco1_hz = hz1;
        self.telemetry.vco2_hz = hz2;
        // **Base plus routes, and computed into locals before `render_sources`.** A closure that
        // captured `&self.graph` would hold an immutable borrow of `self` across that `&mut self`
        // call. `vconwidth` is the target's base and routes add to it (§5): in Manual the routes
        // sit at zero and the base is the knob, which is the legacy expression exactly.
        let pulse_width_of = |base: f32, routed: f32| {
            0.5 - routing::PULSE_WIDTH_SWING * (base + routed).clamp(0.0, 1.0)
        };
        let width1 = pulse_width_of(
            p.osc1.pulse_width,
            self.graph
                .sum(routing::target::VCO_1_PULSE_WIDTH, &self.routing),
        );
        let width2 = pulse_width_of(
            p.osc2.pulse_width,
            self.graph
                .sum(routing::target::VCO_2_PULSE_WIDTH, &self.routing),
        );
        let sources = self.render_sources(SourceConfig {
            osc1: p.osc1,
            osc2: p.osc2,
            hz1,
            hz2,
            width1,
            width2,
            sync: p.sync,
            ring_active: p.fifth_level > 0.0,
        });
        let osc1 = sources.osc1;
        let osc2 = sources.osc2;
        // **Published after the pitch that made them**, so every route from them into pitch or
        // pulse width is backward by one sample — this instrument's FM, and the declared cost of
        // putting the oscillators where the voice already computes them. Cutoff and amplitude sum
        // below, so they read these forward.
        self.graph.write(routing::source::VCO_1, osc1);
        self.graph.write(routing::source::VCO_2, osc2);

        let bank = register_bank_mix(&sources.registers, p.register_levels);
        // **The bank's Level is a target** (the owner, 2026-09-27): its routes add to the knob in
        // its own 0…1. Bit-identical to the knob alone while nothing is routed there.
        let bank_level = if self.graph.is_empty(routing::target::BANK_LEVEL) {
            p.bank_level.clamp(0.0, 1.0)
        } else {
            (p.bank_level + self.graph.sum(routing::target::BANK_LEVEL, &self.routing))
                .clamp(0.0, 1.0)
        };
        let mix = bank_level * BANK_MIX_GAIN * bank
            + p.osc1_level.clamp(0.0, 1.0) * osc1
            + p.osc2_level.clamp(0.0, 1.0) * osc2
            + p.noise_level.clamp(0.0, 1.0) * noise
            + p.fifth_level.clamp(0.0, 1.0) * RING_MIX_GAIN * sources.ring;
        self.telemetry.overload = mix.abs() > 2.0; // chosen detector calibration; warning only
        let highpass = self
            .hpf
            .hp(mix, p.hpf_hz.clamp(10.0, 10_000.0), self.sample_rate);

        // **One exponentiation, one sum, and the six alternatives become pairs.** The paired VCF
        // selectors are no longer a choice the voice makes: `legacy::presence` wires whichever the
        // panel selects, and Pedal mode wires Key at full beside the pedal at the slider, which is
        // what the manual's fixed 1 V/oct plus pedal always meant.
        //
        // **This is the target where bit-identity ends, and §3.3 says so in advance.** The envelope
        // reach folds `FILTER_ENV_OCTAVES × FILTER_ENV_WEIGHT` into one scale, which rounds
        // differently from multiplying them in turn; the polarity switches fold into the sign of
        // their own signed amount rather than a separate multiply; and the sum runs in source order
        // where this expression accumulated env, mod, tracking, audio and bend in its own.
        // §11 asks for the tolerance to be *reported*, not assumed, which is what
        // `the_bank_against_the_c0_dump` measures.
        let octaves = self.graph.sum(routing::target::CUTOFF, &self.routing);
        let cutoff = (p.cutoff_hz.max(5.0) * 2.0f32.powf(octaves))
            .clamp(5.0, 20_000.0f32.min(0.45 * self.sample_rate));
        self.telemetry.cutoff_hz = cutoff;
        let filtered = self
            .filter
            .process(highpass, cutoff, p.resonance, self.sample_rate);

        // **HOLD stays the target's base; everything else is a route** (§6). The selected envelope
        // is present at full because the VCA envelope switch is a connection with no attenuator,
        // the VCA LFO slider is a route from the 0-CENTER output, and the bender is a route from
        // the lever or the multiplier. `bend_term` retires with this, its last caller.
        let gain = (p.hold.clamp(0.0, 1.0)
            + self.graph.sum(routing::target::VCA_LEVEL, &self.routing))
        .clamp(0.0, 3.0);
        let out = filtered * gain * p.volume.clamp(0.0, 1.0);
        // **The collection's standard Amplitude**, a factor after the VCA: it cannot open a closed
        // VCA, so nothing about activity changes, and the output bound below still holds it.
        let out = if self.graph.is_empty(routing::target::AMPLITUDE) {
            out
        } else {
            out * standard::amplitude_factor(
                self.graph.sum(routing::target::AMPLITUDE, &self.routing),
            )
        };
        if out.is_finite() {
            out.clamp(-OUTPUT_BOUND, OUTPUT_BOUND)
        } else {
            0.0
        }
    }
}

#[inline]
pub fn midi_hz(note: f32) -> f32 {
    440.0 * 2.0f32.powf((note.clamp(-120.0, 240.0) - 69.0) / 12.0)
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)] // tests expose one routing change at a time
mod tests {
    use super::*;
    fn id(v: i32, ch: u8, key: u8) -> NoteId {
        NoteId {
            voice_id: Some(v),
            channel: ch,
            key,
        }
    }
    fn run(v: &mut Voice, p: &Patch, n: usize) -> Vec<f32> {
        (0..n).map(|_| v.process(p)).collect()
    }

    /// A routing with the bender wired into both pitch lanes, as the panel's mode switch means it.
    ///
    /// **From [`Routing::init`], not an empty grid.** LFO mode reads the multiplier, and the
    /// multiplier's own two factors have to be present at full or `product` returns its neutral one
    /// and the bend disappears entirely — which is the failure this helper exists to avoid
    /// reproducing in every test that needs it.
    fn wire(routing: &mut Routing, target: usize, source: usize, amount: f32) {
        routing.present[target][source] = true;
        routing.amounts[target][source] = amount;
    }

    /// Which source a bender mode reads, or `None` when it is `Off`.
    fn bender_source(mode: BendMode) -> Option<usize> {
        match mode {
            BendMode::Direct => Some(routing::source::BEND),
            BendMode::Lfo => Some(routing::source::MULTIPLIER),
            BendMode::Off => None,
        }
    }

    /// The bender wired into the destinations the conversion has routed, as its mode switches mean.
    ///
    /// **From [`Routing::init`], not an empty grid.** LFO mode reads the multiplier, whose own two
    /// factors must be present at full or `product` returns its neutral one and the bend disappears.
    fn bender(vco: (BendMode, f32), filter: (BendMode, f32), vca: (BendMode, f32)) -> Routing {
        let mut wired = Routing::init();
        // The panel's mode switches, which are what these tests hold: every one `Off` is no bender
        // route at all, so the Init's own ±2-semitone bend (the modulation standard's deviation)
        // comes off first.
        for (target, source) in routing::INIT_BEND {
            wired.present[target][source] = false;
            wired.amounts[target][source] = 0.0;
        }
        if let Some(from) = bender_source(vco.0) {
            wire(&mut wired, routing::target::VCO_1_PITCH, from, vco.1);
            wire(&mut wired, routing::target::VCO_2_PITCH, from, vco.1);
        }
        if let Some(from) = bender_source(filter.0) {
            wire(&mut wired, routing::target::CUTOFF, from, filter.1);
        }
        if let Some(from) = bender_source(vca.0) {
            wire(&mut wired, routing::target::VCA_LEVEL, from, vca.1);
        }
        wired
    }

    fn pitch_bender(mode: BendMode, depth: f32) -> Routing {
        bender((mode, depth), (BendMode::Off, 0.0), (BendMode::Off, 0.0))
    }

    /// Filter tracking as the panel wires it (§5): Keyboard scales the key line by the slider;
    /// Pedal keeps the key at full and puts the slider on the pedal beside it.
    fn tracking(source: TrackSource, amount: f32) -> Routing {
        let mut wired = Routing::init();
        match source {
            TrackSource::Keyboard => {
                wire(
                    &mut wired,
                    routing::target::CUTOFF,
                    routing::source::KEY,
                    amount,
                );
            }
            TrackSource::Pedal => {
                wire(
                    &mut wired,
                    routing::target::CUTOFF,
                    routing::source::KEY,
                    1.0,
                );
                wire(
                    &mut wired,
                    routing::target::CUTOFF,
                    routing::source::PEDAL,
                    amount,
                );
            }
        }
        wired
    }

    fn magnitudes(x: &[f32]) -> Vec<f64> {
        let n = x.len();
        (1..n / 2)
            .map(|bin| {
                let mut re = 0.0;
                let mut im = 0.0;
                for (i, &sample) in x.iter().enumerate() {
                    let phase = -std::f64::consts::TAU * bin as f64 * i as f64 / n as f64;
                    re += f64::from(sample) * phase.cos();
                    im += f64::from(sample) * phase.sin();
                }
                re.hypot(im)
            })
            .collect()
    }

    fn assert_reference_advantage(
        name: &str,
        candidate: &[f32],
        trivial: &[f32],
        reference: &[f32],
    ) {
        let candidate_spectrum = magnitudes(candidate);
        let trivial_spectrum = magnitudes(trivial);
        let reference_spectrum = magnitudes(reference);
        let error = |actual: &[f64]| {
            actual
                .iter()
                .zip(&reference_spectrum)
                .map(|(a, r)| (a - r) * (a - r))
                .sum::<f64>()
        };
        let candidate_error = error(&candidate_spectrum);
        let trivial_error = error(&trivial_spectrum);
        let advantage_db = 10.0 * (trivial_error / candidate_error.max(1e-30)).log10();
        assert!(
            advantage_db > 18.0,
            "{name}: 8x source path bought only {advantage_db:.2} dB over base-rate"
        );

        let energy =
            |spectrum: &[f64], first: usize| spectrum[first..].iter().map(|x| x * x).sum::<f64>();
        let total_ratio =
            energy(&candidate_spectrum, 0) / energy(&reference_spectrum, 0).max(1e-30);
        let high_start = candidate_spectrum.len() / 4;
        let high_ratio = energy(&candidate_spectrum, high_start)
            / energy(&reference_spectrum, high_start).max(1e-30);
        eprintln!(
            "{name}: 8x vs hard/base-rate advantage {advantage_db:.2} dB, wanted total {total_ratio:.3}, upper-band {high_ratio:.3}"
        );
        assert!(
            total_ratio > 0.90 && high_ratio > 0.80,
            "{name}: rejection dulled wanted content (total {total_ratio:.3}, high {high_ratio:.3})"
        );
    }

    fn source_config(
        osc1: OscPatch,
        osc2: OscPatch,
        hz1: f32,
        hz2: f32,
        sync: bool,
    ) -> SourceConfig {
        SourceConfig {
            osc1,
            osc2,
            hz1,
            hz2,
            width1: 0.5,
            width2: 0.5,
            sync,
            ring_active: true,
        }
    }

    fn source_series(
        factor: usize,
        n: usize,
        mut config: SourceConfig,
        mut widths: impl FnMut(usize) -> (f32, f32),
        select: impl Fn(SourceFrame) -> f32,
    ) -> Vec<f32> {
        let mut voice = Voice::default();
        (0..n)
            .map(|i| {
                (config.width1, config.width2) = widths(i);
                select(voice.render_sources_with_factor(config, factor))
            })
            .collect()
    }

    fn hard_wave(vco: &Vco, patch: OscPatch, width: f32) -> f32 {
        let (divider, _) = vco.test_divider_phase(patch.range);
        let (shaper, _) = vco.test_shaper_phase(patch.range);
        match patch.waveform {
            Waveform::Triangle => 1.0 - 2.0 * (2.0 * shaper - 1.0).abs(),
            Waveform::Saw => 2.0 * shaper - 1.0,
            Waveform::Square => {
                if divider < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            Waveform::Pulse => {
                if shaper < width {
                    1.0
                } else {
                    -1.0
                }
            }
        }
        .clamp(
            -crate::oscillator::OUTPUT_BOUND,
            crate::oscillator::OUTPUT_BOUND,
        )
    }

    fn trivial_source_series(
        n: usize,
        mut config: SourceConfig,
        mut widths: impl FnMut(usize) -> (f32, f32),
        select: impl Fn(SourceFrame) -> f32,
    ) -> Vec<f32> {
        let mut first = Vco::default();
        let mut second = Vco::default();
        (0..n)
            .map(|i| {
                (config.width1, config.width2) = widths(i);
                let a = hard_wave(&first, config.osc1, config.width1);
                let b = hard_wave(&second, config.osc2, config.width2);
                let frame = SourceFrame {
                    osc1: a,
                    osc2: b,
                    ring: a * b,
                    ..SourceFrame::default()
                };
                let edges = first.advance_master(config.hz1);
                second.advance_slave(config.hz2, edges, config.sync);
                select(frame)
            })
            .collect()
    }

    #[test]
    fn production_selects_oversampling_only_for_the_affected_source_paths() {
        let triangle = OscPatch {
            waveform: Waveform::Triangle,
            ..OscPatch::default()
        };
        let pulse = OscPatch {
            waveform: Waveform::Pulse,
            ..OscPatch::default()
        };
        let saw = OscPatch {
            waveform: Waveform::Saw,
            ..OscPatch::default()
        };
        let mut cases = [
            source_config(triangle, saw, 997.0, 1301.0, false),
            source_config(pulse, saw, 997.0, 1301.0, false),
            source_config(saw, saw, 997.0, 1301.0, false),
        ];
        cases[2].ring_active = true;
        for config in cases {
            let selected = Voice::default().render_sources(config);
            let reference = Voice::default().render_sources_with_factor(config, SOURCE_OVERSAMPLE);
            assert_eq!(selected.osc1.to_bits(), reference.osc1.to_bits());
            assert_eq!(selected.ring.to_bits(), reference.ring.to_bits());
        }

        let mut plain_saw = source_config(saw, saw, 997.0, 1301.0, false);
        plain_saw.ring_active = false;
        let selected = Voice::default().render_sources(plain_saw);
        let reference = Voice::default().render_sources_with_factor(plain_saw, 1);
        assert_eq!(selected.osc1.to_bits(), reference.osc1.to_bits());
        assert_eq!(selected.ring.to_bits(), reference.ring.to_bits());
    }

    #[test]
    fn oversampled_reference_harness_records_a_floor_below_the_alias_gate() {
        let n = 1024usize;
        let averaged_sine = |factor: usize| {
            (0..n)
                .map(|sample| {
                    (0..factor)
                        .map(|sub| {
                            let t = sample as f64 + (sub as f64 + 0.5) / factor as f64;
                            (std::f64::consts::TAU * 17.0 * t / n as f64).sin() as f32
                        })
                        .sum::<f32>()
                        / factor as f32
                })
                .collect::<Vec<_>>()
        };
        let candidate = magnitudes(&averaged_sine(SOURCE_OVERSAMPLE));
        let reference = magnitudes(&averaged_sine(64));
        let error = candidate
            .iter()
            .zip(&reference)
            .map(|(a, r)| (a - r) * (a - r))
            .sum::<f64>();
        let wanted = reference.iter().map(|x| x * x).sum::<f64>();
        let floor_db = 10.0 * (error / wanted.max(1e-30)).max(1e-30).log10();
        eprintln!("8x/64x known-clean sine reference floor: {floor_db:.2} dB");
        assert!(
            floor_db < -60.0,
            "the reference floor ({floor_db:.2} dB) is too high to audit an 18 dB advantage"
        );
    }

    #[test]
    fn every_waveshaper_wave_rejects_alias_without_dulling_its_wanted_spectrum() {
        let (n, fs) = (1024usize, 48_000.0f32);
        let hz = 17.0 * fs / n as f32;
        for waveform in [Waveform::Triangle, Waveform::Saw, Waveform::Pulse] {
            let osc = OscPatch {
                waveform,
                ..OscPatch::default()
            };
            let render = |factor| {
                source_series(
                    factor,
                    n,
                    source_config(osc, OscPatch::default(), hz, hz, false),
                    |_| (0.1, 0.5),
                    |frame| frame.osc1,
                )
            };
            let trivial = trivial_source_series(
                n,
                source_config(osc, OscPatch::default(), hz, hz, false),
                |_| (0.1, 0.5),
                |frame| frame.osc1,
            );
            assert_reference_advantage(
                match waveform {
                    Waveform::Triangle => "triangle",
                    Waveform::Saw => "saw",
                    Waveform::Pulse => "pulse",
                    Waveform::Square => unreachable!(),
                },
                &render(SOURCE_OVERSAMPLE),
                &trivial,
                &render(64),
            );
        }
    }

    #[test]
    fn modulated_narrow_pwm_rejects_alias_without_dulling_wanted_sidebands() {
        let (n, fs) = (1024usize, 48_000.0f32);
        let osc = OscPatch {
            waveform: Waveform::Pulse,
            ..OscPatch::default()
        };
        let render = |factor| {
            source_series(
                factor,
                n,
                source_config(
                    osc,
                    OscPatch::default(),
                    31.0 * fs / n as f32,
                    31.0 * fs / n as f32,
                    false,
                ),
                |i| {
                    let modulation = (std::f32::consts::TAU * 3.0 * i as f32 / n as f32).sin();
                    (0.12 + 0.02 * modulation, 0.5)
                },
                |frame| frame.osc1,
            )
        };
        let trivial = trivial_source_series(
            n,
            source_config(
                osc,
                OscPatch::default(),
                31.0 * fs / n as f32,
                31.0 * fs / n as f32,
                false,
            ),
            |i| {
                let modulation = (std::f32::consts::TAU * 3.0 * i as f32 / n as f32).sin();
                (0.12 + 0.02 * modulation, 0.5)
            },
            |frame| frame.osc1,
        );
        assert_reference_advantage(
            "modulated narrow PWM",
            &render(SOURCE_OVERSAMPLE),
            &trivial,
            &render(64),
        );
    }

    #[test]
    fn ring_modulation_rejects_alias_in_ordinary_and_synced_regimes() {
        let (n, fs) = (1024usize, 48_000.0f32);
        let saw = OscPatch {
            waveform: Waveform::Saw,
            ..OscPatch::default()
        };
        let triangle = OscPatch {
            waveform: Waveform::Triangle,
            ..OscPatch::default()
        };
        let cases = [
            ("ordinary ring", false, 17.0, 29.0),
            ("synced ring lock", true, 5.0, 41.0),
            ("synced ring no-divider-clock", true, 17.0, 7.0),
        ];
        for (name, sync, bin1, bin2) in cases {
            let config = source_config(
                saw,
                triangle,
                bin1 * fs / n as f32,
                bin2 * fs / n as f32,
                sync,
            );
            let render =
                |factor| source_series(factor, n, config, |_| (0.5, 0.5), |frame| frame.ring);
            let trivial = trivial_source_series(n, config, |_| (0.5, 0.5), |frame| frame.ring);
            assert_reference_advantage(name, &render(SOURCE_OVERSAMPLE), &trivial, &render(64));
        }
    }

    #[test]
    fn two_pitch_assignment_one_pitch_mode_and_collapse_are_a_shared_voice() {
        let mut v = Voice::default();
        let mut p = Patch::default();
        p.portamento_s = 0.0;
        v.note_on(id(1, 0, 48), 0.0);
        v.process(&p);
        v.note_on(id(2, 0, 72), 0.0);
        v.process(&p);
        let t = v.telemetry();
        assert_eq!((t.high_cv, t.low_cv), (72.0, 48.0));
        p.key_mode = KeyMode::OnePitch;
        v.process(&p);
        // The retained low lane remains 48, while oscillator routing uses high for both.
        assert_eq!(v.telemetry().low_cv, 48.0);
        p.key_mode = KeyMode::TwoPitch;
        v.note_off(Some(2), 0, 72);
        v.process(&p);
        assert_eq!(
            (v.telemetry().high_target, v.telemetry().low_target),
            (48.0, 48.0)
        );
        assert!(
            v.telemetry().retrigger,
            "high release must reach the post-portamento detector"
        );
    }

    #[test]
    fn collapse_target_is_immediate_but_each_directional_lag_obeys_its_diodes() {
        for mode in [
            PortamentoMode::Normal,
            PortamentoMode::Up,
            PortamentoMode::Down,
        ] {
            let mut v = Voice::default();
            let mut p = Patch::default();
            p.portamento_s = 0.2;
            p.portamento_mode = mode;
            v.note_on(id(1, 0, 48), 0.0);
            for _ in 0..48_000 {
                v.process(&p);
            }
            v.note_on(id(2, 0, 72), 0.0);
            v.process(&p);
            let rising = v.telemetry().high_cv;
            if mode == PortamentoMode::Down {
                assert_eq!(rising, 72.0);
            } else {
                assert!(rising < 72.0);
            }
            v.note_off(Some(2), 0, 72);
            v.process(&p);
            let falling = v.telemetry();
            assert_eq!(falling.high_target, 48.0);
            if mode == PortamentoMode::Up {
                assert_eq!(falling.high_cv, 48.0);
            } else {
                assert!(falling.high_cv > 48.0);
            }
        }
    }

    #[test]
    fn final_release_keeps_targets_and_lags_continue_then_idle_settles() {
        let mut v = Voice::default();
        let mut p = Patch::default();
        p.portamento_s = 0.5;
        p.env1.release = 0.1;
        v.note_on(id(1, 0, 72), 0.0);
        v.process(&p);
        v.note_off(Some(1), 0, 72);
        let first = v.telemetry().high_cv;
        for _ in 0..2000 {
            v.process(&p);
        }
        let later = v.telemetry().high_cv;
        assert!(later > first, "final release must not freeze a moving lag");
        for _ in 0..100_000 {
            v.process(&p);
        }
        assert_eq!(v.activity(&p), Activity::Inert);
        assert_eq!(v.process(&p), 0.0);
        assert_eq!(v.telemetry().high_cv, 72.0);
    }

    #[test]
    fn post_portamento_threshold_can_suppress_a_very_slow_retrigger() {
        let mut v = Voice::default();
        let p = Patch {
            portamento_s: 3.0,
            ..Patch::default()
        };
        v.note_on(id(1, 0, 60), 0.0);
        v.process(&p);
        v.note_on(id(2, 0, 61), 0.0);
        for _ in 0..48_000 {
            v.process(&p);
            assert!(!v.telemetry().retrigger);
        }
    }

    fn voice_gliding_to_duplicate_high_key() -> (Voice, Patch) {
        let mut v = Voice::default();
        let p = Patch {
            portamento_s: 0.1,
            ..Patch::default()
        };
        assert!(v.note_on(id(1, 0, 60), 0.0));
        v.process(&p);

        assert!(v.note_on(id(2, 1, 72), 0.0));
        v.process(&p);
        let legitimate_edge = v.telemetry();
        assert!(
            legitimate_edge.retrigger,
            "the changed high-key target must first consume its real edge"
        );
        assert!(legitimate_edge.high_cv < legitimate_edge.high_target);

        assert!(v.note_on(id(3, 2, 72), 0.0));
        v.process(&p);
        let duplicate_press = v.telemetry();
        assert!(!duplicate_press.retrigger);
        assert!(
            duplicate_press.high_cv < duplicate_press.high_target,
            "the equal-key handoff must happen while portamento is still moving"
        );
        (v, p)
    }

    fn assert_equal_key_handoff_does_not_retrigger(mut v: Voice, p: &Patch) {
        v.process(p);
        let handoff = v.telemetry();
        assert_eq!(handoff.high_owner.voice_id, Some(3));
        assert_eq!(handoff.high_target, 72.0);
        assert!(
            handoff.high_cv < handoff.high_target,
            "settled portamento would conceal a duplicate edge"
        );
        assert!(
            !handoff.retrigger,
            "changing equal-key owner identity must not create another pitch edge"
        );
    }

    #[test]
    fn equal_key_release_handoff_during_glide_does_not_retrigger() {
        let (mut v, p) = voice_gliding_to_duplicate_high_key();
        assert!(v.note_off(Some(2), 1, 72));
        assert_equal_key_handoff_does_not_retrigger(v, &p);
    }

    #[test]
    fn equal_key_choke_handoff_during_glide_does_not_retrigger() {
        let (mut v, p) = voice_gliding_to_duplicate_high_key();
        assert!(v.choke(Some(2), 1, 72));
        assert_equal_key_handoff_does_not_retrigger(v, &p);
    }

    #[test]
    fn pedal_tracking_and_bend_follow_the_current_high_owner_channel() {
        let mut v = Voice::default();
        let p = Patch {
            cutoff_hz: 200.0,
            ..Patch::default()
        };
        v.set_topology(&tracking(TrackSource::Pedal, 1.0));
        v.set_expression(1, 0.1);
        v.set_expression(2, 0.8);
        v.note_on(id(1, 1, 48), 0.0);
        v.note_on(id(2, 2, 72), 0.0);
        v.process(&p);
        let high_owner = v.telemetry().cutoff_hz;
        v.note_off(Some(2), 2, 72);
        v.process(&p);
        let handed_off = v.telemetry().cutoff_hz;
        assert!(
            high_owner > handed_off * 3.0,
            "tracking must hand from high owner's channel 2 pedal to channel 1"
        );
    }

    /// **The three new MIDI paths must be inaudible until a route reads them.**
    ///
    /// The machine had no velocity, no mod wheel and no aftertouch, so a fresh instance that hears
    /// any of them is not the copy it claims to be. Everything below is about what happens once a
    /// route does read one; this is about what happens when none does.
    #[test]
    fn the_new_expression_sources_carry_nothing_at_init() {
        let p = Patch::default();
        let mut untouched = Voice::default();
        untouched.set_topology(&Routing::init());
        untouched.note_on(id(1, 0, 60), 0.0);

        let mut leaned_on = Voice::default();
        leaned_on.set_topology(&Routing::init());
        leaned_on.set_mod_wheel(0, 1.0);
        leaned_on.set_channel_pressure(0, 1.0);
        leaned_on.note_on(id(1, 0, 60), 1.0);

        let reference = run(&mut untouched, &p, 512);
        // Two silent renders are equal too. Without this the test would pass on a voice that had
        // stopped sounding entirely, which is the failure it is least likely to notice.
        assert!(
            reference.iter().any(|s| s.abs() > 1e-4),
            "a silent reference would make the comparison below prove nothing"
        );
        assert_eq!(
            reference,
            run(&mut leaned_on, &p, 512),
            "at Init these three reach no target, so a full wheel, full pressure and a hard press \
             must render exactly what none of them renders"
        );
    }

    /// Velocity belongs to the press, so the retained owner carries it and a handoff cannot borrow
    /// a released press's — the same rule pitch already follows.
    ///
    /// Wired from an **empty** routing rather than Init: Init tracks the keyboard into the cutoff,
    /// so releasing the high key would move the cutoff through `Key` as well and the assertion
    /// would pass without velocity doing anything at all.
    #[test]
    fn velocity_follows_the_high_owner_and_release_hands_it_back() {
        let mut v = Voice::default();
        let p = Patch {
            cutoff_hz: 200.0,
            ..Patch::default()
        };
        let mut wired = Routing::new();
        wire(
            &mut wired,
            routing::target::CUTOFF,
            routing::source::VELOCITY,
            1.0,
        );
        v.set_topology(&wired);
        // The quiet press owns nothing while the loud one is held above it.
        v.note_on(id(1, 1, 48), 0.1);
        v.note_on(id(2, 2, 72), 0.9);
        v.process(&p);
        let loud_owner = v.telemetry().cutoff_hz;
        v.note_off(Some(2), 2, 72);
        v.process(&p);
        let handed_back = v.telemetry().cutoff_hz;
        assert!(
            loud_owner > handed_back * 1.5,
            "velocity must follow the retained high owner: {loud_owner} against {handed_back}"
        );
    }

    /// The wheel and pressure are retained **per channel**, whether or not a note sounds, so a note
    /// started while a key is already leaned on inherits the value rather than beginning at zero.
    #[test]
    fn the_wheel_and_pressure_are_retained_per_channel_and_follow_the_owner() {
        for source in [routing::source::WHEEL, routing::source::PRESSURE] {
            let mut v = Voice::default();
            let p = Patch {
                cutoff_hz: 200.0,
                ..Patch::default()
            };
            let mut wired = Routing::new();
            wire(&mut wired, routing::target::CUTOFF, source, 1.0);
            v.set_topology(&wired);
            let set = |v: &mut Voice, channel: u8, value: f32| {
                if source == routing::source::WHEEL {
                    v.set_mod_wheel(channel, value);
                } else {
                    v.set_channel_pressure(channel, value);
                }
            };
            // Both are parked before either key arrives, which is the inheritance itself.
            set(&mut v, 1, 0.0);
            set(&mut v, 2, 0.9);
            v.note_on(id(1, 1, 48), 0.0);
            v.note_on(id(2, 2, 72), 0.0);
            v.process(&p);
            let owned_by_channel_2 = v.telemetry().cutoff_hz;
            v.note_off(Some(2), 2, 72);
            v.process(&p);
            let handed_to_channel_1 = v.telemetry().cutoff_hz;
            assert!(
                owned_by_channel_2 > handed_to_channel_1 * 1.5,
                "source {source} must follow the high owner's channel: \
                 {owned_by_channel_2} against {handed_to_channel_1}"
            );
        }
    }

    #[test]
    fn tracking_amount_scales_keyboard_or_pedal_but_pedal_keeps_fixed_keyboard_tracking() {
        fn cutoff(source: TrackSource, amount: f32, expression: f32) -> f32 {
            let mut v = Voice::default();
            let p = Patch {
                cutoff_hz: 200.0,
                ..Patch::default()
            };
            v.set_topology(&tracking(source, amount));
            v.set_expression(0, expression);
            v.note_on(id(1, 0, 72), 0.0);
            v.process(&p);
            v.telemetry().cutoff_hz
        }
        fn assert_near(actual: f32, expected: f32) {
            assert!(
                (actual / expected - 1.0).abs() < 1e-5,
                "actual {actual}, expected {expected}"
            );
        }

        assert_near(cutoff(TrackSource::Keyboard, 0.0, 0.8), 200.0);
        assert_near(
            cutoff(TrackSource::Keyboard, 0.25, 0.8),
            200.0 * 2.0f32.powf(0.25),
        );
        assert_near(cutoff(TrackSource::Pedal, 0.0, 0.8), 400.0);
        assert_near(
            cutoff(TrackSource::Pedal, 0.25, 0.8),
            200.0 * 2.0f32.powf(1.0 + 0.8 * 0.25),
        );
    }

    #[test]
    fn non_finite_note_controls_are_ignored_without_losing_prior_state() {
        let mut v = Voice::default();
        let p = Patch {
            cutoff_hz: 200.0,
            ..Patch::default()
        };
        // Pedal tracking, the direct pitch bender and the VCA's envelope are all routes now, and
        // this test asserts on the pitch, the cutoff and that audio comes out at all.
        let mut wired = tracking(TrackSource::Pedal, 1.0);
        for lane in [routing::target::VCO_1_PITCH, routing::target::VCO_2_PITCH] {
            wire(&mut wired, lane, routing::source::BEND, 1.0);
        }
        v.set_topology(&wired);
        v.note_on(id(1, 3, 60), 0.0);
        assert!(v.set_poly_tuning(Some(1), 3, 60, 0.75));
        v.set_pitch_bend(3, 0.25);
        v.set_expression(3, 0.5);
        v.process(&p);
        let before = v.telemetry();

        assert!(!v.set_poly_tuning(Some(1), 3, 60, f32::NAN));
        v.set_pitch_bend(3, f32::NAN);
        v.set_expression(3, f32::NAN);
        let after = v.process(&p);
        let telemetry = v.telemetry();

        assert_eq!(
            v.assignment().high.unwrap().tuning_semitones,
            0.75,
            "bad tuning must preserve the accepted per-note value"
        );
        assert_eq!(v.bends[3], 0.25, "bad bend must preserve its prior value");
        assert_eq!(
            v.pedals[3], 0.5,
            "bad expression must preserve its prior value"
        );
        assert_eq!(telemetry.vco1_hz, before.vco1_hz);
        assert_eq!(telemetry.cutoff_hz, before.cutoff_hz);
        assert!(after.is_finite());
        assert!(
            run(&mut v, &p, 512).into_iter().any(|sample| sample != 0.0),
            "one bad controller event must not prevent subsequent finite audio"
        );
    }

    #[test]
    fn gate_reset_bend_and_retrigger_are_distinct_lines() {
        let mut v = Voice::default();
        let mut p = Patch::default();
        p.gate_source = GateSource::SampleHold;
        p.sh_sample_time_s = 0.013;
        // Autonomous clock eventually gates envelopes but does not reset the LFO or start auto bend.
        let mut gate_seen = false;
        for _ in 0..2000 {
            v.process(&p);
            let t = v.telemetry();
            gate_seen |= t.env1_gate;
            assert!(!t.lfo_reset && !t.auto_bend_started);
        }
        assert!(gate_seen);
        v.set_trigger_input(true);
        v.process(&p);
        let t = v.telemetry();
        assert!(t.retrigger);
        assert!(!t.auto_bend_started);
        // A key still drives the keyboard's own lines while the S&H clock holds the gate.
        v.note_on(id(1, 0, 60), 0.0);
        v.process(&p);
        assert!(v.telemetry().auto_bend_started);
    }

    #[test]
    fn trigger_input_does_not_latch_sustain_when_gate_is_low() {
        let mut v = Voice::default();
        let p = Patch {
            gate_source: GateSource::Host,
            env1: EnvelopePatch {
                attack: 0.001,
                decay: 0.01,
                sustain: 1.0,
                release: 0.02,
                trigger: TriggerMode::GateTrigger,
            },
            ..Patch::default()
        };
        v.set_trigger_input(true);
        let first = v.process(&p);
        assert!(v.telemetry().retrigger && !v.telemetry().env1_gate);
        assert!(first.is_finite());
        for _ in 0..20_000 {
            v.process(&p);
        }
        assert_eq!(v.activity(&p), Activity::Inert);
        assert_eq!(v.process(&p), 0.0);
    }

    #[test]
    fn lfo_envelope_mode_is_square_anded_with_its_selected_gate_and_forces_restart() {
        let mut v = Voice::default();
        let mut p = Patch::default();
        p.env1.trigger = TriggerMode::Lfo;
        p.keyboard_trigger_lfo = false;
        p.lfo_rate_hz = 2.0;
        v.note_on(id(1, 0, 60), 0.0);
        v.process(&p);
        assert!(v.telemetry().lfo_reset);
        assert!(v.telemetry().env1_gate);
        v.note_off(Some(1), 0, 60);
        v.process(&p);
        assert!(!v.telemetry().env1_gate);
    }

    #[test]
    fn held_extreme_middle_and_duplicate_depressions_restart_lfo_without_reasserting_gate() {
        for force_from_envelope in [false, true] {
            let mut v = Voice::default();
            let mut p = Patch {
                hold: 1.0,
                lfo_delay_s: 1.0,
                lfo_rate_hz: 2.0,
                keyboard_trigger_lfo: !force_from_envelope,
                ..Patch::default()
            };
            if force_from_envelope {
                p.env1.trigger = TriggerMode::Lfo;
            }

            assert!(v.note_on(id(1, 0, 48), 0.0));
            v.process(&p);
            assert!(v.telemetry().lfo_reset);
            assert!(v.telemetry().auto_bend_started);

            for (press, description) in [
                (id(2, 0, 72), "second extreme"),
                (id(3, 0, 60), "middle key"),
                (id(4, 0, 60), "duplicate key"),
            ] {
                run(&mut v, &p, 1_234);
                let mut free_running = v.lfo.clone();
                free_running.process(p.lfo_rate_hz, p.lfo_wave, p.lfo_delay_s, v.sample_rate);

                assert!(v.note_on(press, 0.0));
                v.process(&p);
                let telemetry = v.telemetry();
                assert!(telemetry.lfo_reset, "{description} missed KYBD TRIG");
                assert!(
                    !telemetry.auto_bend_started,
                    "{description} reasserted the shared gate and restarted auto bend"
                );
                assert_eq!(
                    v.lfo.fade().to_bits(),
                    free_running.fade().to_bits(),
                    "{description} dumped the gate-only sine-delay capacitor"
                );
                assert!(
                    v.lfo.phase() < 0.001,
                    "{description} did not restart LFO phase: {}",
                    v.lfo.phase()
                );
            }
        }
    }

    #[test]
    fn keyboard_gate_arms_sine_delay_without_keyboard_triggering_saw_or_pwm() {
        let mut v = Voice::default();
        let mut p = Patch {
            hold: 1.0,
            lfo_wave: LfoWave::Sine,
            lfo_delay_s: 1.0,
            keyboard_trigger_lfo: false,
            ..Patch::default()
        };
        run(&mut v, &p, 1234);
        let phase_before = v.lfo.phase();
        p.hold = 0.0;
        v.note_on(id(1, 0, 60), 0.0);
        v.process(&p);

        assert!(!v.telemetry().lfo_reset, "KYBD TRIG is off");
        assert!(
            v.lfo.phase() > phase_before,
            "arming delay must leave the free-running LFO phase alone"
        );
        assert!(v.lfo.fade() < 0.001, "ordinary gate must dump delay fade");

        let mut sine = v.lfo.clone();
        assert!(
            sine.process(5.0, LfoWave::Sine, 1.0, 48_000.0).plus.abs() < 0.001,
            "sine vibrato must begin delayed"
        );
        let mut delayed = v.lfo.clone();
        let mut undelayed = v.lfo.clone();
        let saw = delayed.process(5.0, LfoWave::Saw, 1.0, 48_000.0);
        let reference = undelayed.process(5.0, LfoWave::Saw, 0.0, 48_000.0);
        assert_eq!(saw.plus, reference.plus, "saw modulation is undelayed");
        assert_eq!(
            saw.pwm_triangle, reference.pwm_triangle,
            "PWM triangle is undelayed"
        );
    }

    #[test]
    fn bender_modes_are_signed_off_or_rectified_for_each_destination() {
        let base = Patch::default();
        for dest in 0..3 {
            let mut values = [0.0; 2];
            for (j, bend) in [-0.8, 0.8].into_iter().enumerate() {
                let mut v = Voice::default();
                let p = base.clone();
                v.note_on(id(1, 2, 60), 0.0);
                v.set_pitch_bend(2, bend);
                // **Every destination is a route now**, so the mode and its depth go straight to
                // the routing; the patch has no bender terms left to carry.
                let off = (BendMode::Off, 0.0);
                let lfo = (BendMode::Lfo, 1.0);
                v.set_topology(&match dest {
                    0 => bender(lfo, off, off),
                    1 => bender(off, lfo, off),
                    _ => bender(off, off, lfo),
                });
                for _ in 0..100 {
                    values[j] += v.process(&p).abs();
                }
            }
            assert!(
                (values[0] - values[1]).abs() < 1e-4,
                "LFO depth is rectified at destination {dest}"
            );
        }
        let mut off_negative = Voice::default();
        let mut off_positive = Voice::default();
        // Armed with the bender Off, so "discards bend entirely" is a claim about an armed voice
        // rather than one that has no pitch routes at all and would pass for the wrong reason.
        off_negative.set_topology(&pitch_bender(BendMode::Off, 0.0));
        off_positive.set_topology(&pitch_bender(BendMode::Off, 0.0));
        off_negative.note_on(id(1, 0, 60), 0.0);
        off_positive.note_on(id(1, 0, 60), 0.0);
        off_negative.set_pitch_bend(0, -0.8);
        off_positive.set_pitch_bend(0, 0.8);
        assert_eq!(
            run(&mut off_negative, &base, 256),
            run(&mut off_positive, &base, 256),
            "OFF must discard bend entirely"
        );

        let p = base.clone();
        let mut negative = Voice::default();
        let mut positive = Voice::default();
        // Both halves of this claim are routes, so both are armed here and the patch carries
        // neither.
        let direct = bender(
            (BendMode::Direct, 1.0),
            (BendMode::Direct, 1.0),
            (BendMode::Off, 0.0),
        );
        negative.set_topology(&direct);
        positive.set_topology(&direct);
        negative.note_on(id(1, 0, 60), 0.0);
        positive.note_on(id(1, 0, 60), 0.0);
        negative.set_pitch_bend(0, -0.8);
        positive.set_pitch_bend(0, 0.8);
        negative.process(&p);
        positive.process(&p);
        assert!(
            positive.telemetry().vco1_hz > negative.telemetry().vco1_hz,
            "direct VCO bend must retain sign"
        );
        assert!(
            positive.telemetry().cutoff_hz > negative.telemetry().cutoff_hz,
            "direct VCF bend must retain sign"
        );

        let mut negative = Voice::default();
        let mut positive = Voice::default();
        negative.note_on(id(1, 0, 60), 0.0);
        positive.note_on(id(1, 0, 60), 0.0);
        negative.set_pitch_bend(0, -0.8);
        positive.set_pitch_bend(0, 0.8);
        // The amplifier's bend is a route now; the VCA's own envelope comes from `Routing::init`.
        let vca_direct = bender(
            (BendMode::Off, 0.0),
            (BendMode::Off, 0.0),
            (BendMode::Direct, 1.0),
        );
        negative.set_topology(&vca_direct);
        positive.set_topology(&vca_direct);
        let plus: f32 = run(&mut positive, &p, 128).iter().map(|x| x.abs()).sum();
        let minus: f32 = run(&mut negative, &p, 128).iter().map(|x| x.abs()).sum();
        assert!(plus > minus, "direct VCA bend must retain sign");
    }

    #[test]
    fn vco_bender_cv_and_lfo_have_independent_full_and_partial_spans() {
        fn routed_span(mode: BendMode, lever: f32, depth: f32) -> f32 {
            let mut voice = Voice::default();
            let patch = Patch {
                lfo_wave: LfoWave::Saw, // phase zero is the full-scale +1 0-CENTER endpoint
                ..Patch::default()
            };
            voice.set_topology(&pitch_bender(mode, depth));
            voice.note_on(id(1, 0, 60), 0.0);
            voice.set_pitch_bend(0, lever);
            voice.process(&patch);
            12.0 * (voice.telemetry().vco1_hz / midi_hz(60.0)).log2()
        }
        fn assert_near(actual: f32, expected: f32) {
            assert!(
                (actual - expected).abs() < 1e-4,
                "actual {actual}, expected {expected}"
            );
        }

        // Literal expectations are independent of the production constants: the service spec
        // distinguishes ±15 semitones in CV mode from ±10 in LFO mode. A shared 15-semitone
        // scale for both modes must fail in the routed pitch at both sensitivity settings.
        for (depth, cv_span, lfo_span) in [(1.0, 15.0, 10.0), (0.4, 6.0, 4.0)] {
            assert_near(routed_span(BendMode::Direct, 1.0, depth), cv_span);
            assert_near(routed_span(BendMode::Direct, -1.0, depth), -cv_span);
            assert_near(routed_span(BendMode::Lfo, 1.0, depth), lfo_span);
            assert_near(routed_span(BendMode::Lfo, -1.0, depth), lfo_span);
        }
    }

    #[test]
    fn bender_lfo_uses_the_zero_center_square_for_vco_and_filter() {
        let mut v = Voice::default();
        let p = Patch {
            lfo_wave: LfoWave::Square,
            lfo_rate_hz: 10.0,
            cutoff_hz: 1_000.0,
            ..Patch::default()
        };
        // Both halves are routes now.
        v.set_topology(&bender(
            (BendMode::Lfo, 1.0),
            (BendMode::Lfo, 1.0),
            (BendMode::Off, 0.0),
        ));
        v.note_on(id(1, 0, 60), 0.0);
        v.set_pitch_bend(0, 1.0);

        v.process(&p);
        let high = v.telemetry();
        run(&mut v, &p, 2_400);
        let low = v.telemetry();

        let base_hz = midi_hz(60.0);
        let high_vco_st = 12.0 * (high.vco1_hz / base_hz).log2();
        let low_vco_st = 12.0 * (low.vco1_hz / base_hz).log2();
        assert!(high_vco_st > 0.0 && low_vco_st < 0.0);
        assert!(
            (high_vco_st + low_vco_st).abs() < 1e-4,
            "square VCO bend has a DC component: high={high_vco_st}, low={low_vco_st}"
        );

        let high_filter_oct = (high.cutoff_hz / p.cutoff_hz).log2();
        let low_filter_oct = (low.cutoff_hz / p.cutoff_hz).log2();
        assert!(high_filter_oct > 0.0 && low_filter_oct < 0.0);
        assert!(
            (high_filter_oct + low_filter_oct).abs() < 1e-5,
            "square filter bend has a DC component: high={high_filter_oct}, low={low_filter_oct}"
        );
    }

    #[test]
    fn env2_has_only_the_vca_route_and_zero_controls_preserve_the_click_pulse() {
        let mut v = Voice::default();
        let mut p = Patch::default();
        p.env1 = EnvelopePatch {
            attack: 0.0,
            decay: 0.0,
            sustain: 0.0,
            release: 0.0,
            trigger: TriggerMode::Gate,
        };
        p.cutoff_hz = 20_000.0;
        p.osc1_level = 1.0;
        // `Amplitude from Envelope 1` is what opens the VCA, and it is a route now, so a voice that
        // is never armed renders silence rather than the onset this measures.
        v.set_topology(&Routing::init());
        v.note_on(id(1, 0, 60), 0.0);
        let click = run(&mut v, &p, 192);
        let transient = |x: &[f32]| {
            x.windows(2)
                .take(96)
                .map(|pair| (pair[1] - pair[0]).abs())
                .fold(0.0f32, f32::max)
        };
        let mut declicked = Voice::default();
        let mut declicked_patch = p.clone();
        // This is the explicit sabotage oracle: a hidden 10 ms fade has the same shape as
        // lengthening the VCA envelope attack to 10 ms. It must erase the early click score.
        declicked_patch.env1.attack = 0.010;
        declicked.set_topology(&Routing::init());
        declicked.note_on(id(1, 0, 60), 0.0);
        let smoothed = run(&mut declicked, &declicked_patch, 192);
        let click_transient = transient(&click);
        let smoothed_transient = transient(&smoothed);
        let click_early_sum = click[0..96].iter().map(|x| x.abs()).sum::<f32>();
        let smoothed_early_sum = smoothed[0..96].iter().map(|x| x.abs()).sum::<f32>();
        eprintln!(
            "zero-control onset: transient {click_transient:.6} vs 10 ms sabotage {smoothed_transient:.6}; early sum {click_early_sum:.6} vs {smoothed_early_sum:.6}"
        );
        const MIN_CLICK_TRANSIENT: f32 = 0.05;
        const MIN_EARLY_SUM: f32 = 12.0;
        assert!(
            smoothed_transient < MIN_CLICK_TRANSIENT && smoothed_early_sum < MIN_EARLY_SUM,
            "the sabotage must cross both fixed click gates"
        );
        assert!(
            click_transient > MIN_CLICK_TRANSIENT && click_early_sum > MIN_EARLY_SUM,
            "a hidden 10 ms de-click must fail the fixed onset gates"
        );
        assert!(
            click_transient > smoothed_transient * 2.0,
            "the zero-control VCA pulse is not transiently distinct from a 10 ms de-click ({click_transient:.6} vs {smoothed_transient:.6})"
        );
        assert!(
            click_early_sum > smoothed_early_sum * 2.0,
            "the click gate must fail when an attack/de-click ramp is inserted"
        );

        let mut a = Voice::default();
        let mut b = Voice::default();
        let mut p2 = p.clone();
        p2.env2.attack = 4.0;
        a.note_on(id(1, 0, 60), 0.0);
        b.note_on(id(1, 0, 60), 0.0);
        let xa = run(&mut a, &p, 128);
        let xb = run(&mut b, &p2, 128);
        assert_eq!(
            xa, xb,
            "ENV-2 must not leak into filter or PWM when VCA selects ENV-1"
        );
    }

    #[test]
    fn register_loading_is_continuous_and_preserves_single_slider_level_changes() {
        let registers = [1.0; 5];
        let silent = register_bank_mix(&registers, [0.0; 5]);
        let tiny = register_bank_mix(&registers, [0.001, 0.0, 0.0, 0.0, 0.0]);
        let quarter = register_bank_mix(&registers, [0.25, 0.0, 0.0, 0.0, 0.0]);
        let full = register_bank_mix(&registers, [1.0, 0.0, 0.0, 0.0, 0.0]);
        let all = register_bank_mix(&registers, [1.0; 5]);

        assert_eq!(silent, 0.0);
        assert!(
            tiny > 0.0 && tiny < full * 0.003,
            "a slider crossing zero must rise continuously, not jump to full level"
        );
        assert!(
            quarter > full * 0.3 && quarter < full * 0.5,
            "one register must retain meaningful slider-level control"
        );
        assert_eq!(
            full, 1.0,
            "one fully raised register is the unity calibration"
        );
        assert!(
            all < full * 1.8,
            "dual-gang loading must prevent five raised registers from summing fivefold"
        );
    }

    #[test]
    fn register_mixer_is_level_normalised_and_ring_channel_is_heaviest_without_limiting() {
        let mut v = Voice::default();
        let mut p = Patch::default();
        p.hold = 1.0;
        p.cutoff_hz = 20_000.0;
        p.bank_level = 1.0;
        p.osc1_level = 0.0;
        p.register_levels = [0.0, 0.0, 1.0, 0.0, 0.0];
        let one = run(&mut v, &p, 4096);
        let r1 = one.iter().map(|x| x * x).sum::<f32>().sqrt();
        v.reset();
        p.register_levels = [1.0; 5];
        let all = run(&mut v, &p, 4096);
        let r2 = all.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            r2 / r1 < 1.8,
            "five registers must alter timbre, not multiply level fivefold"
        );
        const { assert!(RING_MIX_GAIN > BANK_MIX_GAIN && BANK_MIX_GAIN > 1.0) };
    }

    #[test]
    fn hpf_is_one_pole_and_loses_low_tone_as_corner_rises() {
        // A 32' triangle at the resting key, about 65 Hz: its harmonics fall 12 dB an octave, so
        // nearly all of its energy sits well below the higher corner.
        fn rms(corner: f32) -> f32 {
            let mut v = Voice::default();
            let mut p = Patch::default();
            p.hold = 1.0;
            p.osc1 = OscPatch {
                range: Range::Feet32,
                waveform: Waveform::Triangle,
                ..OscPatch::default()
            };
            p.hpf_hz = corner;
            p.cutoff_hz = 20_000.0;
            let mut s = 0.0;
            for i in 0..48_000 {
                let y = v.process(&p);
                if i > 8000 {
                    s += y * y
                }
            }
            (s / 40_000.0).sqrt()
        }
        assert!(rms(1000.0) < rms(10.0) * 0.2);
    }

    #[test]
    fn pink_network_is_darker_than_white_and_not_just_a_label() {
        fn roughness(colour: NoiseColour) -> f64 {
            let mut v = Voice::default();
            let p = Patch {
                hold: 1.0,
                osc1_level: 0.0,
                noise_level: 0.4,
                noise_colour: colour,
                cutoff_hz: 20_000.0,
                hpf_hz: 10.0,
                ..Patch::default()
            };
            let x = run(&mut v, &p, 48_000);
            let power: f64 = x.iter().map(|x| f64::from(*x) * f64::from(*x)).sum();
            let diff: f64 = x
                .windows(2)
                .map(|w| {
                    let d = f64::from(w[1] - w[0]);
                    d * d
                })
                .sum();
            diff / power.max(1e-30)
        }
        let pink = roughness(NoiseColour::Pink);
        let white = roughness(NoiseColour::White);
        assert!(
            pink < white * 0.7,
            "pink/white roughness = {}",
            pink / white
        );
    }

    #[test]
    fn panic_wakes_after_leaving_and_reentering_the_sample_hold_gate() {
        let mut v = Voice::default();
        let mut p = Patch {
            gate_source: GateSource::SampleHold,
            sh_sample_time_s: 0.013,
            ..Patch::default()
        };
        v.process(&p);
        assert_eq!(v.activity(&p), Activity::Live);

        v.all_sound_off();
        assert_eq!(v.process(&p), 0.0);
        assert_eq!(v.activity(&p), Activity::Inert);

        p.gate_source = GateSource::Host;
        assert_eq!(v.process(&p), 0.0);
        assert_eq!(v.activity(&p), Activity::Inert);

        // Re-entered in the other key mode: the gate source alone decides.
        p.key_mode = KeyMode::OnePitch;
        p.gate_source = GateSource::SampleHold;
        v.process(&p);
        assert_eq!(v.activity(&p), Activity::Live);
        assert_eq!(v.telemetry().gate_source, GateSource::SampleHold);
    }

    /// **The S&H clock gates the envelopes in either key mode** (the owner, 2026-09-27): with no key
    /// pressed and the Init routing, choosing the S&H as the gate source fires the envelopes and
    /// makes the voice play by itself in Two-pitch and in One-pitch alike; the host gate does
    /// neither.
    ///
    /// Falsified before trusted: honouring the gate source only outside the two key modes (as the
    /// retired External mode did) leaves both silent.
    #[test]
    fn the_sample_hold_clock_gates_the_envelopes_in_either_key_mode() {
        for key_mode in [KeyMode::TwoPitch, KeyMode::OnePitch] {
            for (gate_source, plays) in [(GateSource::SampleHold, true), (GateSource::Host, false)]
            {
                let mut v = Voice::default();
                v.set_topology(&Routing::init());
                let p = Patch {
                    key_mode,
                    gate_source,
                    sh_sample_time_s: 0.013,
                    ..Patch::default()
                };
                let mut gated = false;
                let mut sounded = false;
                for _ in 0..4_000 {
                    sounded |= v.process(&p) != 0.0;
                    gated |= v.telemetry().env1_gate;
                }
                assert_eq!(gated, plays, "{key_mode:?} with {gate_source:?}: the gate");
                assert_eq!(
                    sounded, plays,
                    "{key_mode:?} with {gate_source:?}: the sound"
                );
            }
        }
    }

    fn voice_at_sample_hold_clock(clock_high: bool) -> (Voice, Patch) {
        let mut v = Voice::default();
        let envelope = EnvelopePatch {
            attack: 0.001,
            decay: 0.001,
            sustain: 1.0,
            release: 1.0,
            trigger: TriggerMode::GateTrigger,
        };
        let p = Patch {
            gate_source: GateSource::SampleHold,
            sh_sample_time_s: 0.013,
            env1: envelope,
            env2: envelope,
            ..Patch::default()
        };
        assert!(v.note_on(id(1, 0, 60), 0.0));
        for _ in 0..2_000 {
            v.process(&p);
            let stage_ready = if clock_high {
                v.env_stage(0) == Stage::Sustain && v.env_stage(1) == Stage::Sustain
            } else {
                v.env_stage(0) == Stage::Release && v.env_stage(1) == Stage::Release
            };
            if v.telemetry().sh_clock == clock_high && stage_ready {
                return (v, p);
            }
        }
        panic!("the test did not reach the requested S&H clock phase");
    }

    fn assert_sample_hold_gate_ignores_note_termination(clock_high: bool, choke: bool) {
        let (mut actual, p) = voice_at_sample_hold_clock(clock_high);
        let mut reference = actual.clone();

        if choke {
            assert!(actual.choke(Some(1), 0, 60));
        } else {
            actual.all_notes_off();
        }
        assert!(
            !actual.keyboard.is_held(),
            "the ledger must still be cleared"
        );

        let expected = reference.process(&p);
        let observed = actual.process(&p);
        assert_eq!(observed.to_bits(), expected.to_bits());
        assert_eq!(actual.env_stage(0), reference.env_stage(0));
        assert_eq!(actual.env_stage(1), reference.env_stage(1));
        assert_eq!(
            actual.env1.level().to_bits(),
            reference.env1.level().to_bits()
        );
        assert_eq!(
            actual.env2.level().to_bits(),
            reference.env2.level().to_bits()
        );
        assert_eq!(actual.telemetry().selected_gate, clock_high);
        assert_eq!(actual.telemetry().env1_gate, clock_high);
        assert_eq!(actual.telemetry().env2_gate, clock_high);
        assert!(
            !actual.note_off(Some(1), 0, 60),
            "termination must retire the press even though S&H owns articulation"
        );
    }

    #[test]
    fn all_notes_off_does_not_touch_the_sample_hold_gate_at_either_clock_level() {
        for clock_high in [true, false] {
            assert_sample_hold_gate_ignores_note_termination(clock_high, false);
        }
    }

    #[test]
    fn final_choke_does_not_touch_the_sample_hold_gate_at_either_clock_level() {
        for clock_high in [true, false] {
            assert_sample_hold_gate_ignores_note_termination(clock_high, true);
        }
    }

    fn host_gated_voice_at_sustain() -> (Voice, Patch) {
        let mut v = Voice::default();
        // Armed, because this pair of tests is about *articulation* and articulation only survives
        // in a voice that is not asleep. A bare voice routes nothing into the amplifier, so after
        // the gate closes it is Inert and `process` settles the envelopes rather than releasing
        // them. Every real instance of this instrument has Envelope 1 on the amplifier.
        v.set_topology(&Routing::init());
        let envelope = EnvelopePatch {
            attack: 0.001,
            decay: 0.001,
            sustain: 1.0,
            release: 1.0,
            trigger: TriggerMode::GateTrigger,
        };
        let p = Patch {
            env1: envelope,
            env2: envelope,
            ..Patch::default()
        };
        assert!(v.note_on(id(1, 0, 60), 0.0));
        for _ in 0..256 {
            v.process(&p);
            if v.env_stage(0) == Stage::Sustain && v.env_stage(1) == Stage::Sustain {
                return (v, p);
            }
        }
        panic!("the Host-gated test voice did not reach sustain");
    }

    #[test]
    fn all_notes_off_still_releases_host_gated_articulation() {
        let (mut v, p) = host_gated_voice_at_sustain();
        v.all_notes_off();
        v.process(&p);
        assert_eq!(v.telemetry().gate_source, GateSource::Host);
        assert!(!v.telemetry().selected_gate);
        assert_eq!(v.env_stage(0), Stage::Release);
        assert_eq!(v.env_stage(1), Stage::Release);
    }

    #[test]
    fn final_choke_still_cuts_host_gated_articulation() {
        let (mut v, p) = host_gated_voice_at_sustain();
        assert!(v.choke(Some(1), 0, 60));
        assert_eq!(v.process(&p).to_bits(), 0.0f32.to_bits());
        assert_eq!(v.telemetry().gate_source, GateSource::Host);
        assert!(!v.telemetry().selected_gate);
        assert_eq!(v.env_stage(0), Stage::Idle);
        assert_eq!(v.env_stage(1), Stage::Idle);
    }

    #[test]
    fn note_terminations_cancel_same_sample_keyboard_assertions() {
        for final_choke in [false, true] {
            let mut v = Voice::default();
            let p = Patch {
                keyboard_trigger_lfo: true,
                ..Patch::default()
            };
            v.note_on(id(1, 0, 60), 0.0);
            if final_choke {
                v.choke(Some(1), 0, 60);
            } else {
                v.all_notes_off();
            }

            v.process(&p);
            let t = v.telemetry();
            assert!(!t.lfo_reset, "termination must cancel the keyboard reset");
            assert!(
                !t.auto_bend_started,
                "termination must cancel the keyboard auto-bend start"
            );
            assert_eq!(v.env_stage(0), Stage::Idle);
            assert_eq!(v.env_stage(1), Stage::Idle);
        }
    }

    #[test]
    fn note_terminations_cancel_armed_portamento_retriggers() {
        let mut all_notes = Voice::default();
        let mut p = Patch {
            portamento_s: 0.1,
            ..Patch::default()
        };
        all_notes.note_on(id(1, 0, 60), 0.0);
        all_notes.process(&p);
        all_notes.note_on(id(2, 0, 72), 0.0);
        all_notes.all_notes_off();
        all_notes.process(&p);
        assert!(
            !all_notes.telemetry().retrigger,
            "All Notes Off must disarm a pending post-portamento edge"
        );

        let mut choke = Voice::default();
        p.portamento_mode = PortamentoMode::Up;
        choke.note_on(id(1, 0, 48), 0.0);
        choke.process(&Patch::default());
        choke.note_on(id(2, 0, 72), 0.0);
        choke.process(&p);
        assert!(
            choke.telemetry().retrigger,
            "test setup needs a rising edge"
        );
        choke.choke(Some(2), 0, 72);
        choke.choke(Some(1), 0, 48);
        choke.process(&p);
        assert!(
            !choke.telemetry().retrigger,
            "a final choke must disarm the pending collapse edge"
        );
    }

    #[test]
    fn note_terminations_preserve_an_independent_external_trigger() {
        for final_choke in [false, true] {
            let mut v = Voice::default();
            let p = Patch::default();
            v.note_on(id(1, 0, 60), 0.0);
            v.set_trigger_input(true);
            if final_choke {
                v.choke(Some(1), 0, 60);
            } else {
                v.all_notes_off();
            }
            v.process(&p);
            assert!(
                v.telemetry().retrigger,
                "note termination must not consume the independent trigger input"
            );
        }
    }

    #[test]
    fn disconnected_long_release_does_not_extend_the_audible_tail() {
        let mut v = Voice::default();
        let p = Patch {
            env1: EnvelopePatch {
                release: 0.02,
                ..EnvelopePatch::default()
            },
            env2: EnvelopePatch {
                release: 8.0,
                ..EnvelopePatch::default()
            },
            ..Patch::default()
        };
        // `Routing::init` routes Envelope 1 into the amplifier and leaves Envelope 2 absent, which
        // is what "disconnected" means now: the guarantee is stated over routes rather than over
        // the `vca_env` selector, and it is the same guarantee.
        v.set_topology(&Routing::init());
        v.note_on(id(1, 0, 60), 0.0);
        run(&mut v, &p, 2_000);
        v.note_off(Some(1), 0, 60);
        assert!(v.tail_samples(&p) < 4_000);

        let mut samples = 0;
        while v.activity(&p) != Activity::Inert && samples < 4_000 {
            v.process(&p);
            samples += 1;
        }
        assert!(samples < 4_000, "the short selected envelope owns the tail");
        assert_eq!(v.env_stage(0), Stage::Idle);
        assert_eq!(
            v.env_stage(1),
            Stage::Idle,
            "entering inert state must silence the disconnected envelope"
        );
        assert_eq!(v.tail_samples(&p), 0);
    }

    #[test]
    fn selecting_a_disconnected_envelope_after_idle_cannot_revive_its_release() {
        let mut v = Voice::default();
        let p = Patch {
            env1: EnvelopePatch {
                release: 0.02,
                ..EnvelopePatch::default()
            },
            env2: EnvelopePatch {
                release: 8.0,
                ..EnvelopePatch::default()
            },
            ..Patch::default()
        };
        v.set_topology(&Routing::init());
        v.note_on(id(1, 0, 60), 0.0);
        run(&mut v, &p, 2_000);
        v.note_off(Some(1), 0, 60);
        while v.activity(&p) != Activity::Inert {
            v.process(&p);
        }

        // Simulate an arbitrarily long host sleep by doing no processing before reconnecting ENV-2.
        // **Reconnecting is a topology change now**, not a selector move, which is exactly what the
        // panel's switch retires into.
        let mut reconnected = Routing::init();
        reconnected.present[routing::target::VCA_LEVEL][routing::source::ENVELOPE_1] = false;
        wire(
            &mut reconnected,
            routing::target::VCA_LEVEL,
            routing::source::ENVELOPE_2,
            1.0,
        );
        v.set_topology(&reconnected);
        assert_eq!(v.activity(&p), Activity::Inert);
        for _ in 0..256 {
            assert_eq!(v.process(&p).to_bits(), 0.0f32.to_bits());
        }
        assert_eq!(v.env_stage(1), Stage::Idle);
    }

    #[test]
    fn panic_clears_pitch_pink_and_sample_hold_history_but_retains_event_ownership() {
        let mut long_history = Voice::default();
        let mut short_history = Voice::default();
        let patch = Patch {
            hold: 1.0,
            noise_level: 0.7,
            noise_colour: NoiseColour::Pink,
            sh_source: ShSource::Random,
            sh_sample_time_s: 0.013,
            sh_lag_s: 1.0,
            portamento_s: 2.0,
            ..Patch::default()
        };
        assert!(long_history.note_on(id(1, 2, 55), 0.0));
        assert!(short_history.note_on(id(1, 2, 55), 0.0));
        run(&mut long_history, &patch, 5_000);
        run(&mut short_history, &patch, 137);

        long_history.all_sound_off();
        short_history.all_sound_off();
        assert_eq!(long_history.activity(&patch), Activity::Inert);
        assert_eq!(short_history.activity(&patch), Activity::Inert);
        assert_eq!(long_history.assignment().high.unwrap().id, id(1, 2, 55));

        assert!(long_history.note_on(id(2, 3, 67), 0.0));
        assert!(short_history.note_on(id(2, 3, 67), 0.0));
        let long_wake = run(&mut long_history, &patch, 512);
        let short_wake = run(&mut short_history, &patch, 512);
        assert_eq!(
            long_wake, short_wake,
            "pink recursion, RNG, or S&H hold/lag leaked through All Sound Off"
        );
        let telemetry = long_history.telemetry();
        assert!(telemetry.sh_held.is_finite() && telemetry.sh_out.is_finite());

        assert!(long_history.note_off(Some(1), 2, 55));
        assert!(short_history.note_off(Some(1), 2, 55));
        assert_eq!(long_history.assignment(), short_history.assignment());
        assert_eq!(long_history.assignment().high.unwrap().id, id(2, 3, 67));
    }

    #[test]
    fn panic_discards_every_pending_event_before_an_autonomous_wake() {
        fn establish_history(v: &mut Voice, p: &mut Patch) {
            p.hold = 1.0;
            p.sh_sample_time_s = 0.013;
            for _ in 0..400 {
                v.process(p);
            }
            p.hold = 0.0;
        }
        fn wake_on_reset_sample_hold(v: &mut Voice, p: &mut Patch) {
            p.gate_source = GateSource::SampleHold;
            v.process(p);
            let t = v.telemetry();
            // Panic now clears S&H state, whose reset clock starts high and may legitimately gate
            // the envelopes. None of the discarded keyboard/retrigger lines may reappear.
            assert!(!t.retrigger && !t.lfo_reset && !t.auto_bend_started);
        }

        let mut keyboard = Voice::default();
        let mut keyboard_patch = Patch {
            keyboard_trigger_lfo: true,
            ..Patch::default()
        };
        establish_history(&mut keyboard, &mut keyboard_patch);
        keyboard.note_on(id(1, 0, 60), 0.0);
        keyboard.all_sound_off();
        assert_eq!(keyboard.assignment().high.unwrap().id.key, 60);
        wake_on_reset_sample_hold(&mut keyboard, &mut keyboard_patch);

        let mut external = Voice::default();
        let mut external_patch = Patch::default();
        establish_history(&mut external, &mut external_patch);
        external.set_trigger_input(true);
        external.all_sound_off();
        wake_on_reset_sample_hold(&mut external, &mut external_patch);

        let mut retrigger = Voice::default();
        let mut retrigger_patch = Patch::default();
        establish_history(&mut retrigger, &mut retrigger_patch);
        retrigger.note_on(id(1, 0, 60), 0.0);
        retrigger.process(&retrigger_patch);
        retrigger.note_on(id(2, 0, 72), 0.0);
        retrigger.all_sound_off();
        wake_on_reset_sample_hold(&mut retrigger, &mut retrigger_patch);
    }

    #[test]
    fn panic_reset_exact_silence_and_determinism_hold() {
        let mut a = Voice::default();
        let mut b = Voice::default();
        let mut p = Patch::default();
        p.hold = 1.0;
        p.noise_level = 0.2;
        assert_eq!(run(&mut a, &p, 1024), run(&mut b, &p, 1024));
        a.note_on(id(1, 0, 60), 0.0);
        run(&mut a, &p, 64);
        a.all_sound_off();
        assert_eq!(a.process(&p), 0.0);
        assert_eq!(a.activity(&p), Activity::Inert);
        for _ in 0..128 {
            assert_eq!(
                a.process(&p),
                0.0,
                "a retained press must not reopen the gate"
            );
        }
        assert!(a.note_on(id(2, 0, 67), 0.0));
        assert_ne!(
            a.activity(&p),
            Activity::Inert,
            "a fresh note must clear panic even while an old press remains held"
        );
        a.reset();
        assert_eq!(a.process(&Patch::default()), 0.0);
    }

    #[test]
    fn release_reaches_exact_idle_silence_and_denormals_do_not_survive() {
        let mut v = Voice::default();
        let mut p = Patch::default();
        p.env1.release = 0.05;
        v.note_on(id(1, 0, 60), 0.0);
        run(&mut v, &p, 5000);
        v.note_off(Some(1), 0, 60);
        run(&mut v, &p, 100_000);
        assert_eq!(v.activity(&p), Activity::Inert);
        for _ in 0..1000 {
            assert_eq!(v.process(&p).to_bits(), 0.0f32.to_bits());
        }
    }

    #[test]
    fn the_sample_rate_floor_is_safe_at_and_below_the_lowest_supported_rate() {
        let p = Patch {
            hold: 1.0,
            hpf_hz: f32::MAX,
            cutoff_hz: f32::MAX,
            ..Patch::default()
        };
        for requested in [
            crate::MIN_SAMPLE_RATE,
            crate::MIN_SAMPLE_RATE - 1.0,
            1.0,
            0.0,
            -1.0,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ] {
            let mut v = Voice::new(requested);
            assert!(v.sample_rate >= crate::MIN_SAMPLE_RATE);
            for _ in 0..256 {
                let y = v.process(&p);
                assert!(y.is_finite() && y.abs() <= OUTPUT_BOUND);
            }
            v.set_sample_rate(requested);
            assert!(v.sample_rate >= crate::MIN_SAMPLE_RATE);
            assert!(v.process(&p).is_finite());
        }
    }

    #[test]
    fn finite_and_bounded_under_extreme_sweeps_at_all_rates() {
        for fs in [1_000.0, 44_100.0, 48_000.0, 96_000.0, 192_000.0, 768_000.0] {
            let mut v = Voice::new(fs);
            let mut p = Patch::default();
            v.note_on(id(1, 15, 127), 0.0);
            p.hold = 1.0;
            p.register_levels = [1.0; 5];
            p.bank_level = 1.0;
            p.osc1_level = 1.0;
            p.osc2_level = 1.0;
            p.noise_level = 1.0;
            p.fifth_level = 1.0;
            p.resonance = 1.0;
            // **Every modulation extreme is a route now**, so the hostile patch has to arm them
            // or this stops being a hostile test at all.
            let mut hostile = Routing::init();
            for (target, source) in [
                (routing::target::CUTOFF, routing::source::VCO_2),
                (routing::target::CUTOFF, routing::source::ENVELOPE_1),
                (routing::target::VCA_LEVEL, routing::source::LFO_CENTRED),
            ] {
                wire(&mut hostile, target, source, 1.0);
            }
            v.set_topology(&hostile);
            p.sync = true;
            p.hpf_hz = 99_999.0;
            p.cutoff_hz = f32::MAX;
            for i in 0..20_000 {
                p.cutoff_hz = if i & 1 == 0 { 0.0 } else { f32::MAX };
                let y = v.process(&p);
                assert!(
                    y.is_finite() && y.abs() <= OUTPUT_BOUND,
                    "fs={fs} i={i} y={y}"
                );
            }
        }
    }
}
