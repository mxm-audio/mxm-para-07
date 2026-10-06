//! Permanent host parameters for mxm-para-07.
//!
//! The controls mirror the completed DSP's evidenced signal graph. Every `#[id]` is public
//! interface and must never be renamed or reused. Signals are smoothed; selectors, rates and
//! state-machine times are not.

use mxm_para_07_dsp::{
    lfo,
    oscillator::{Range, Waveform},
    sh,
    voice::{
        BendMode, EnvChoice, FilterAudioSource, FilterModSource, GateSource, KeyMode, NoiseColour,
        PortamentoMode, PwmSource, TrackSource, TriggerMode,
    },
};
use nice_plug::prelude::*;
use std::sync::{Arc, RwLock};

type ValueToString = Arc<dyn Fn(f32) -> String + Send + Sync>;
type StringToValue = Arc<dyn Fn(&str) -> Option<f32> + Send + Sync>;

fn v2s_percent() -> ValueToString {
    Arc::new(|v| format!("{:.0} %", v * 100.0))
}
fn s2v_percent() -> StringToValue {
    Arc::new(|s| {
        s.trim()
            .trim_end_matches('%')
            .trim()
            .parse::<f32>()
            .ok()
            .map(|v| v / 100.0)
    })
}
/// Whole milliseconds below a second, hundredths of a second from it — **the unit chosen from the
/// rounded milliseconds, not the raw value.** Chosen from the raw value, 0.9995 s printed `1000 ms`
/// and read back `1.00 s`, and where a range's inverse of one second lands just below it, `1.00 s`
/// read back `1000 ms`.
fn v2s_time() -> ValueToString {
    Arc::new(|s| {
        let ms = s * 1000.0;
        if ms.round() >= 1000.0 {
            format!("{s:.2} s")
        } else {
            format!("{ms:.0} ms")
        }
    })
}
fn s2v_time() -> StringToValue {
    Arc::new(|text| {
        let text = text.trim().to_lowercase();
        let (number, scale) = if let Some(v) = text.strip_suffix("ms") {
            (v, 0.001)
        } else if let Some(v) = text.strip_suffix('s') {
            (v, 1.0)
        } else {
            (text.as_str(), 0.001)
        };
        number.trim().parse::<f32>().ok().map(|v| v * scale)
    })
}
fn amount(name: &str, default: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min: 0.0, max: 1.0 })
        .with_smoother(SmoothingStyle::Linear(10.0))
        .with_value_to_string(v2s_percent())
        .with_string_to_value(s2v_percent())
}
fn time(name: &str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(
        name,
        default,
        FloatRange::Skewed {
            min,
            max,
            factor: FloatRange::skew_factor(-2.0),
        },
    )
    .with_value_to_string(v2s_time())
    .with_string_to_value(s2v_time())
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyModeKind {
    #[id = "two"]
    #[name = "Two-pitch"]
    TwoPitch,
    #[id = "one"]
    #[name = "One-pitch"]
    OnePitch,
    // `#[id = "external"]` retired with the External key mode (the owner, 2026-09-27) and is never
    // to be re-used: `retire_external_key_mode` reads it from an older session.
}
impl From<KeyModeKind> for KeyMode {
    fn from(v: KeyModeKind) -> Self {
        match v {
            KeyModeKind::TwoPitch => Self::TwoPitch,
            KeyModeKind::OnePitch => Self::OnePitch,
        }
    }
}

/// **A session that chose the retired External key mode restores as One-pitch** — both oscillators
/// on one note, which is what External played; its S&H gating is the Gate source, which the session
/// keeps. nice-plug stores a key mode by its variant id, and an id it no longer knows is only
/// logged, leaving the mode wherever it was, so the old id is rewritten before the state is read.
///
/// A preset needs no help: it stores the normalised value, and External's 1.0 and a three-way
/// One-pitch's 0.5 both round to One-pitch now there are two.
pub fn retire_external_key_mode(state: &mut PluginState) {
    if let Some(nice_plug::plugin::ParamValue::String(id)) = state.params.get_mut("keymode")
        && id == "external"
    {
        "one".clone_into(id);
    }
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateSourceKind {
    #[id = "host"]
    #[name = "Host gate"]
    Host,
    #[id = "samplehold"]
    #[name = "S&H clock"]
    SampleHold,
}
impl From<GateSourceKind> for GateSource {
    fn from(v: GateSourceKind) -> Self {
        match v {
            GateSourceKind::Host => Self::Host,
            GateSourceKind::SampleHold => Self::SampleHold,
        }
    }
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortamentoKind {
    #[id = "normal"]
    #[name = "Normal"]
    Normal,
    #[id = "up"]
    #[name = "Up only"]
    Up,
    #[id = "down"]
    #[name = "Down only"]
    Down,
}
impl From<PortamentoKind> for PortamentoMode {
    fn from(v: PortamentoKind) -> Self {
        match v {
            PortamentoKind::Normal => Self::Normal,
            PortamentoKind::Up => Self::Up,
            PortamentoKind::Down => Self::Down,
        }
    }
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeKind {
    #[id = "32"]
    #[name = "32'"]
    Feet32,
    #[id = "16"]
    #[name = "16'"]
    Feet16,
    #[id = "8"]
    #[name = "8'"]
    Feet8,
    #[id = "4"]
    #[name = "4'"]
    Feet4,
    #[id = "2"]
    #[name = "2'"]
    Feet2,
}
impl From<RangeKind> for Range {
    fn from(v: RangeKind) -> Self {
        match v {
            RangeKind::Feet32 => Self::Feet32,
            RangeKind::Feet16 => Self::Feet16,
            RangeKind::Feet8 => Self::Feet8,
            RangeKind::Feet4 => Self::Feet4,
            RangeKind::Feet2 => Self::Feet2,
        }
    }
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaveKind {
    #[id = "triangle"]
    #[name = "Triangle"]
    Triangle,
    #[id = "saw"]
    #[name = "Sawtooth"]
    Saw,
    #[id = "square"]
    #[name = "Square"]
    Square,
    #[id = "pulse"]
    #[name = "Pulse"]
    Pulse,
}
impl From<WaveKind> for Waveform {
    fn from(v: WaveKind) -> Self {
        match v {
            WaveKind::Triangle => Self::Triangle,
            WaveKind::Saw => Self::Saw,
            WaveKind::Square => Self::Square,
            WaveKind::Pulse => Self::Pulse,
        }
    }
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum PwmKind {
    #[id = "lfo"]
    #[name = "LFO"]
    Lfo,
    #[id = "manual"]
    #[name = "Manual"]
    Manual,
    #[id = "env1"]
    #[name = "Envelope 1"]
    Env1,
}
impl From<PwmKind> for PwmSource {
    fn from(v: PwmKind) -> Self {
        match v {
            PwmKind::Lfo => Self::Lfo,
            PwmKind::Manual => Self::Manual,
            PwmKind::Env1 => Self::Env1,
        }
    }
}

macro_rules! enum_map {
    ($name:ident, $target:ty, $($variant:ident => $to:ident, $id:literal, $label:literal);+ $(;)?) => {
        #[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $name { $(#[id = $id] #[name = $label] $variant),+ }
        impl From<$name> for $target { fn from(v: $name) -> Self { match v { $($name::$variant => <$target>::$to),+ } } }
    };
}

enum_map!(TriggerKind, TriggerMode,
    GateTrigger => GateTrigger, "gatetrigger", "Gate + trig";
    Gate => Gate, "gate", "Gate";
    Lfo => Lfo, "lfo", "LFO");
enum_map!(EnvChoiceKind, EnvChoice, Env1 => Env1, "env1", "Envelope 1"; Env2 => Env2, "env2", "Envelope 2");
enum_map!(LfoKind, lfo::Waveform, Saw => Saw, "saw", "Sawtooth"; Square => Square, "square", "Square"; Sine => Sine, "sine", "Sine");
enum_map!(ShSourceKind, sh::Source, Saw => Saw, "saw", "LFO saw"; Triangle => Triangle, "triangle", "LFO triangle"; Random => Random, "random", "Random");
enum_map!(BendKind, BendMode, Direct => Direct, "direct", "Direct"; Off => Off, "off", "Off"; Lfo => Lfo, "lfo", "LFO depth");
enum_map!(NoiseKind, NoiseColour, White => White, "white", "White"; Pink => Pink, "pink", "Pink");
enum_map!(FilterModKind, FilterModSource, Lfo => Lfo, "lfo", "LFO"; SampleHold => SampleHold, "samplehold", "Sample and hold");
enum_map!(FilterAudioKind, FilterAudioSource, Vco2 => Vco2, "vco2", "VCO-2"; Noise => Noise, "noise", "Noise");
enum_map!(TrackKind, TrackSource, Keyboard => Keyboard, "keyboard", "Keyboard"; Pedal => Pedal, "pedal", "Pedal");

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectionKind {
    #[id = "down"]
    #[name = "Down"]
    Down,
    #[id = "up"]
    #[name = "Up"]
    Up,
}

/// **The LFO rate's tempo sync** (`plans/plan-tempo-sync-controls.md`): every LFO's ladder, 1/32 to
/// four bars, the top the fastest.
pub const LFO_SYNC: mxm_tempo::Ladder =
    mxm_tempo::Ladder::new(mxm_tempo::Span::LFO, mxm_tempo::Direction::Rate);

/// **The sample time's tempo sync**: 1/64 to a whole note, the slice of the ladder the clock's
/// 13 ms – 2 s holds at 120 bpm, the top the longest.
pub const SH_SYNC: mxm_tempo::Ladder = mxm_tempo::Ladder::new(
    mxm_tempo::Span::new(mxm_tempo::Division::SixtyFourth, mxm_tempo::Division::Whole),
    mxm_tempo::Direction::Time,
);

#[derive(Params)]
pub struct MxmPara07Params {
    #[id = "keymode"]
    pub key_mode: EnumParam<KeyModeKind>,
    #[id = "gatesource"]
    pub gate_source: EnumParam<GateSourceKind>,
    #[id = "triggerinput"]
    pub trigger_input: BoolParam,
    #[id = "portamento"]
    pub portamento: FloatParam,
    #[id = "portamentomode"]
    pub portamento_mode: EnumParam<PortamentoKind>,
    #[id = "tune"]
    pub tune: FloatParam,

    #[id = "vco1range"]
    pub vco1_range: EnumParam<RangeKind>,
    #[id = "vco1wave"]
    pub vco1_wave: EnumParam<WaveKind>,
    #[id = "vco1width"]
    pub vco1_width: FloatParam,

    #[id = "vco2range"]
    pub vco2_range: EnumParam<RangeKind>,
    #[id = "vco2wave"]
    pub vco2_wave: EnumParam<WaveKind>,
    #[id = "vco2width"]
    pub vco2_width: FloatParam,
    #[id = "vco2tune"]
    pub vco2_tune: FloatParam,
    #[id = "sync"]
    pub sync: BoolParam,

    #[id = "reg32"]
    pub reg32: FloatParam,
    #[id = "reg16"]
    pub reg16: FloatParam,
    #[id = "reg8"]
    pub reg8: FloatParam,
    #[id = "reg4"]
    pub reg4: FloatParam,
    #[id = "reg2"]
    pub reg2: FloatParam,
    #[id = "bank"]
    pub bank: FloatParam,
    #[id = "vco1"]
    pub vco1: FloatParam,
    #[id = "vco2"]
    pub vco2: FloatParam,
    #[id = "noise"]
    pub noise: FloatParam,
    /// The mixer's fifth channel: the ring modulator, VCO-1 times VCO-2. Its source switch
    /// (`ringinput`, `fifthinput`) and `extsensitivity` retired with the external input.
    #[id = "fifth"]
    pub fifth: FloatParam,
    #[id = "noisecolour"]
    pub noise_colour: EnumParam<NoiseKind>,

    #[id = "hpf"]
    pub hpf: FloatParam,
    #[id = "cutoff"]
    pub cutoff: FloatParam,
    #[id = "resonance"]
    pub resonance: FloatParam,

    #[id = "env1attack"]
    pub env1_attack: FloatParam,
    #[id = "env1decay"]
    pub env1_decay: FloatParam,
    #[id = "env1sustain"]
    pub env1_sustain: FloatParam,
    #[id = "env1release"]
    pub env1_release: FloatParam,
    #[id = "env1trigger"]
    pub env1_trigger: EnumParam<TriggerKind>,
    #[id = "env2attack"]
    pub env2_attack: FloatParam,
    #[id = "env2decay"]
    pub env2_decay: FloatParam,
    #[id = "env2sustain"]
    pub env2_sustain: FloatParam,
    #[id = "env2release"]
    pub env2_release: FloatParam,
    #[id = "env2trigger"]
    pub env2_trigger: EnumParam<TriggerKind>,

    #[id = "hold"]
    pub hold: FloatParam,
    #[id = "lfoshape"]
    pub lfo_shape: EnumParam<LfoKind>,
    #[id = "lforate"]
    pub lfo_rate: FloatParam,
    /// The LFO rate's tempo sync: its position picks a division of the host's tempo.
    #[id = "lfosync"]
    pub lfo_sync: BoolParam,
    #[id = "lfodelay"]
    pub lfo_delay: FloatParam,
    #[id = "lfokeytrigger"]
    pub lfo_key_trigger: BoolParam,
    #[id = "shsource"]
    pub sh_source: EnumParam<ShSourceKind>,
    #[id = "shtime"]
    pub sh_time: FloatParam,
    /// The sample time's tempo sync: its position picks a division of the host's tempo.
    #[id = "shsync"]
    pub sh_sync: BoolParam,
    #[id = "shlag"]
    pub sh_lag: FloatParam,
    #[id = "autobendtime"]
    pub auto_bend_time: FloatParam,
    #[id = "autobenddirection"]
    pub auto_bend_direction: EnumParam<DirectionKind>,

    #[id = "volume"]
    pub volume: FloatParam,

    /// One presence and one signed amount per routing pair, nine targets by eighteen sources.
    #[nested(group = "Modulation")]
    pub routes: crate::routes::Routes,

    #[persist = "preset"]
    pub preset: RwLock<mxm_preset::PresetIdentity>,
}

impl Default for MxmPara07Params {
    fn default() -> Self {
        let width = |name| amount(name, 0.0);
        Self {
            key_mode: EnumParam::new("Key mode", KeyModeKind::TwoPitch),
            gate_source: EnumParam::new("Gate source", GateSourceKind::Host),
            trigger_input: BoolParam::new("Trigger input", false),
            portamento: time("Portamento", 0.0, 0.0, 3.0),
            portamento_mode: EnumParam::new("Portamento direction", PortamentoKind::Normal),
            // The instrument's master tune: VCO-2 follows it with its own offset (the owner,
            // 2026-09-27: *Master tune — it is more correct*).
            tune: FloatParam::new(
                "Master tune",
                0.0,
                FloatRange::Linear {
                    min: -3.5,
                    max: 3.5,
                },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_unit(" st")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),

            vco1_range: EnumParam::new("VCO-1 range", RangeKind::Feet8),
            vco1_wave: EnumParam::new("VCO-1 wave", WaveKind::Saw),
            vco1_width: width("VCO-1 pulse width"),

            vco2_range: EnumParam::new("VCO-2 range", RangeKind::Feet8),
            vco2_wave: EnumParam::new("VCO-2 wave", WaveKind::Square),
            vco2_width: width("VCO-2 pulse width"),
            vco2_tune: FloatParam::new(
                "VCO-2 tune",
                0.07,
                FloatRange::Linear {
                    min: -7.5,
                    max: 7.5,
                },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_unit(" st")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            sync: BoolParam::new("Sync", false),

            reg32: amount("Register 32'", 0.0),
            reg16: amount("Register 16'", 0.0),
            reg8: amount("Register 8'", 0.0),
            reg4: amount("Register 4'", 0.0),
            reg2: amount("Register 2'", 0.0),
            bank: amount("Register bank", 0.0),
            vco1: amount("VCO-1", 0.7),
            vco2: amount("VCO-2", 0.0),
            noise: amount("Noise", 0.0),
            fifth: amount("Ring", 0.0),
            noise_colour: EnumParam::new("Noise colour", NoiseKind::White),

            hpf: FloatParam::new(
                "High-pass cutoff",
                10.0,
                FloatRange::Skewed {
                    min: 10.0,
                    max: 10_000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_smoother(SmoothingStyle::Linear(10.0))
            .with_unit(" Hz")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            cutoff: FloatParam::new(
                "Cutoff",
                18_000.0,
                FloatRange::Skewed {
                    min: 5.0,
                    max: 20_000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_smoother(SmoothingStyle::Linear(10.0))
            .with_unit(" Hz")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            resonance: amount("Resonance", 0.0),

            env1_attack: time("Envelope 1 attack", 0.005, 0.0, 4.0),
            env1_decay: time("Envelope 1 decay", 0.2, 0.0, 8.0),
            env1_sustain: amount("Envelope 1 sustain", 0.7),
            env1_release: time("Envelope 1 release", 0.2, 0.0, 8.0),
            env1_trigger: EnumParam::new("Envelope 1 trigger", TriggerKind::GateTrigger),
            env2_attack: time("Envelope 2 attack", 0.005, 0.0, 4.0),
            env2_decay: time("Envelope 2 decay", 0.2, 0.0, 8.0),
            env2_sustain: amount("Envelope 2 sustain", 0.7),
            env2_release: time("Envelope 2 release", 0.2, 0.0, 8.0),
            env2_trigger: EnumParam::new("Envelope 2 trigger", TriggerKind::GateTrigger),

            hold: amount("Hold", 0.0),
            lfo_shape: EnumParam::new("LFO shape", LfoKind::Sine),
            lfo_rate: FloatParam::new(
                "LFO rate",
                5.0,
                FloatRange::Skewed {
                    min: lfo::RATE_MIN_HZ,
                    max: lfo::RATE_MAX_HZ,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_unit(" Hz")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            lfo_sync: BoolParam::new("LFO sync", false),
            lfo_delay: time("LFO delay", 0.0, 0.0, lfo::DELAY_MAX_S),
            lfo_key_trigger: BoolParam::new("LFO keyboard trigger", false),
            sh_source: EnumParam::new("Sample and hold source", ShSourceKind::Random),
            sh_time: time(
                "Sample time",
                0.2,
                sh::SAMPLE_TIME_MIN_S,
                sh::SAMPLE_TIME_MAX_S,
            ),
            sh_sync: BoolParam::new("Sample time sync", false),
            sh_lag: time("Sample lag", 0.0, 0.0, 2.0),
            auto_bend_time: time("Auto bend time", 0.2, 0.02, 0.7),
            auto_bend_direction: EnumParam::new("Auto bend direction", DirectionKind::Down),

            volume: FloatParam::new(
                "Volume",
                0.8,
                FloatRange::Skewed {
                    min: util::db_to_gain(-60.0),
                    max: util::db_to_gain(0.0),
                    factor: FloatRange::gain_skew_factor(-60.0, 0.0),
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(20.0))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(1))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),
            routes: crate::routes::Routes::new(),
            preset: RwLock::new(mxm_preset::PresetIdentity::none()),
        }
    }
}

/// Every parameter in declaration order. Presets and the editor's binding lookup share this list
/// so the two surfaces cannot drift.
impl MxmPara07Params {
    /// The LFO rate while its sync follows the host, or `None` for its free value: the modulated
    /// position picks a division on [`LFO_SYNC`]. Resolved once a buffer by the plugin.
    pub fn synced_lfo_rate(&self, tempo: Option<f64>) -> Option<f32> {
        let param = &self.lfo_rate;
        LFO_SYNC
            .resolve(
                self.lfo_sync.value(),
                tempo,
                param.modulated_normalized_value(),
                f64::from(param.preview_plain(0.0)),
                f64::from(param.preview_plain(1.0)),
            )
            .map(|hz| hz as f32)
    }

    /// The sample time while its sync follows the host, or `None` for its free value: the modulated
    /// position picks a division on [`SH_SYNC`]. Resolved once a buffer by the plugin.
    pub fn synced_sh_time(&self, tempo: Option<f64>) -> Option<f32> {
        let param = &self.sh_time;
        SH_SYNC
            .resolve(
                self.sh_sync.value(),
                tempo,
                param.modulated_normalized_value(),
                f64::from(param.preview_plain(0.0)),
                f64::from(param.preview_plain(1.0)),
            )
            .map(|seconds| seconds as f32)
    }
}

/// Every permanent id this plugin has retired. **None may ever be re-used**: a saved session or
/// preset naming one is skipped for it, and a new parameter under an old name would be handed a value
/// that meant something else.
///
/// First the twenty-eight controls the routing conversion replaced (`plans/plan-mxm-para-07-modulation.md`
/// §5), each now a route; then, with the external input and everything on it (the owner,
/// 2026-09-26), its two source switches and sensitivity and the follower's route pair on every
/// target.
#[cfg(test)]
pub const RETIRED_IDS: [&str; 47] = [
    "vco1pwm",
    "vco1pwmsrc",
    "vco1lfo",
    "vco1sh",
    "vco1auto",
    "vco2pwm",
    "vco2pwmsrc",
    "vco2lfo",
    "vco2sh",
    "vco2auto",
    "filterenv",
    "envpolarity",
    "filtermodsrc",
    "filtermod",
    "keysource",
    "keytrack",
    "filteraudiosrc",
    "filteraudio",
    "follower",
    "followerpolarity",
    "vcaenv",
    "vcalfo",
    "bendvcomode",
    "bendvco",
    "bendfiltermode",
    "bendfilter",
    "bendvcamode",
    "bendvca",
    // The external input's, 2026-09-26.
    "ringinput",
    "fifthinput",
    "extsensitivity",
    "mod_vco1pitch_follow",
    "mod_vco1pitch_followon",
    "mod_vco2pitch_follow",
    "mod_vco2pitch_followon",
    "mod_vco1pw_follow",
    "mod_vco1pw_followon",
    "mod_vco2pw_follow",
    "mod_vco2pw_followon",
    "mod_cutoff_follow",
    "mod_cutoff_followon",
    "mod_amp_follow",
    "mod_amp_followon",
    "mod_mult_follow",
    "mod_mult_followon",
    // VCA level ← Key, refused by the modulation standard (2026-09-26): a key parked off middle C
    // latches the VCA open.
    "mod_amp_key",
    "mod_amp_keyon",
];

pub fn all_parameters(p: &MxmPara07Params) -> Vec<(&'static str, &dyn mxm_preset::ErasedParam)> {
    macro_rules! list { ($($id:literal => $field:ident),+ $(,)?) => { vec![$(($id, &p.$field as &dyn mxm_preset::ErasedParam)),+] }; }
    list![
        "keymode"=>key_mode, "gatesource"=>gate_source, "triggerinput"=>trigger_input,
        "portamento"=>portamento, "portamentomode"=>portamento_mode, "tune"=>tune,
        "vco1range"=>vco1_range, "vco1wave"=>vco1_wave, "vco1width"=>vco1_width,
        "vco2range"=>vco2_range, "vco2wave"=>vco2_wave, "vco2width"=>vco2_width,
        "vco2tune"=>vco2_tune, "sync"=>sync,
        "reg32"=>reg32, "reg16"=>reg16, "reg8"=>reg8, "reg4"=>reg4, "reg2"=>reg2,
        "bank"=>bank, "vco1"=>vco1, "vco2"=>vco2, "noise"=>noise, "fifth"=>fifth,
        "noisecolour"=>noise_colour, "hpf"=>hpf, "cutoff"=>cutoff,
        "resonance"=>resonance, "env1attack"=>env1_attack, "env1decay"=>env1_decay, "env1sustain"=>env1_sustain,
        "env1release"=>env1_release, "env1trigger"=>env1_trigger,
        "env2attack"=>env2_attack, "env2decay"=>env2_decay, "env2sustain"=>env2_sustain,
        "env2release"=>env2_release, "env2trigger"=>env2_trigger,
        "hold"=>hold, "lfoshape"=>lfo_shape,
        "lforate"=>lfo_rate, "lfosync"=>lfo_sync, "lfodelay"=>lfo_delay,
        "lfokeytrigger"=>lfo_key_trigger,
        "shsource"=>sh_source, "shtime"=>sh_time, "shsync"=>sh_sync, "shlag"=>sh_lag,
        "autobendtime"=>auto_bend_time, "autobenddirection"=>auto_bend_direction,
        "volume"=>volume,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **No retired id is back**, under any spelling the derive produces.
    #[test]
    fn no_retired_id_is_back() {
        let params = MxmPara07Params::default();
        let ids: Vec<String> = params
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        for retired in RETIRED_IDS {
            assert!(!ids.iter().any(|id| id == retired), "`{retired}` is back");
        }
    }

    /// **The LFO sync picks a division and is inert without a tempo**
    /// (`plans/plan-tempo-sync-controls.md`): off, or with no tempo, the knob's own hertz stand; on
    /// at 120 bpm the ends are the ladder's ends that the range can hold, the top the fastest.
    #[test]
    fn lfo_sync_picks_a_division_and_is_inert_without_a_tempo() {
        use nice_plug::params::InternalParamMut;
        fn set<P: InternalParamMut>(param: &P, normalized: f32) {
            unsafe {
                let _ = param._internal_set_normalized_value(normalized);
            }
        }
        let p = MxmPara07Params::default();
        set(&p.lfo_rate, 1.0);
        assert_eq!(p.synced_lfo_rate(Some(120.0)), None, "off is the free rate");
        set(&p.lfo_sync, 1.0);
        assert_eq!(p.synced_lfo_rate(None), None, "no tempo is the free rate");

        let top = p.synced_lfo_rate(Some(120.0)).expect("synced at a tempo");
        set(&p.lfo_rate, 0.0);
        let bottom = p.synced_lfo_rate(Some(120.0)).expect("synced at a tempo");
        let (lo, hi) = (
            f64::from(p.lfo_rate.preview_plain(0.0)),
            f64::from(p.lfo_rate.preview_plain(1.0)),
        );
        assert!(
            top > bottom,
            "the top of a rate is the fastest: {bottom} to {top}"
        );
        let reach = LFO_SYNC.reachable(120.0, lo, hi).divisions();
        let fastest = reach[0].hz(120.0) as f32;
        let slowest = reach[reach.len() - 1].hz(120.0) as f32;
        assert!((top - fastest).abs() < 1e-4, "{top} against {fastest}");
        assert!(
            (bottom - slowest).abs() < 1e-4,
            "{bottom} against {slowest}"
        );
    }

    /// **The sample time's sync picks a division and is inert without a tempo**
    /// (`plans/plan-tempo-sync-controls.md`): off, or with no tempo, the knob's own time stands; on
    /// at 120 bpm the ends are the ladder's ends that the range can hold, the top the longest.
    #[test]
    fn sample_time_sync_picks_a_division_and_is_inert_without_a_tempo() {
        use nice_plug::params::InternalParamMut;
        fn set<P: InternalParamMut>(param: &P, normalized: f32) {
            unsafe {
                let _ = param._internal_set_normalized_value(normalized);
            }
        }
        let p = MxmPara07Params::default();
        set(&p.sh_time, 1.0);
        assert_eq!(p.synced_sh_time(Some(120.0)), None, "off is the free time");
        set(&p.sh_sync, 1.0);
        assert_eq!(p.synced_sh_time(None), None, "no tempo is the free time");

        let top = p.synced_sh_time(Some(120.0)).expect("synced at a tempo");
        set(&p.sh_time, 0.0);
        let bottom = p.synced_sh_time(Some(120.0)).expect("synced at a tempo");
        let (lo, hi) = (
            f64::from(p.sh_time.preview_plain(0.0)),
            f64::from(p.sh_time.preview_plain(1.0)),
        );
        assert!(
            top > bottom,
            "the top of a time is the longest: {bottom} to {top}"
        );
        let reach = SH_SYNC.reachable(120.0, lo, hi).divisions();
        let shortest = reach[0].seconds(120.0) as f32;
        let longest = reach[reach.len() - 1].seconds(120.0) as f32;
        assert!(
            (bottom - shortest).abs() < 1e-5,
            "{bottom} against {shortest}"
        );
        assert!((top - longest).abs() < 1e-5, "{top} against {longest}");
    }

    /// Plain values either side of every point where a formatter here changes unit, precision or
    /// sign. Each parameter clamps what lies outside its own range, so one list serves them all.
    const BOUNDARIES: [f32; 19] = [
        // Tune and VCO-2 tune cross zero: a ten-thousandth and a thousandth of a semitone, and the
        // half-hundredth where two decimals tie.
        -0.005, -1.0e-3, -1.0e-4, 0.0, 1.0e-4, 1.0e-3, 0.005,
        // Portamento, the envelope times, LFO delay, sample time and lag read whole milliseconds
        // below a second and hundredths of a second above: both rounding edges below one second
        // and the `1.00 s` bucket above it.
        0.9994, 0.9995, 0.9996, 0.99995, 1.0, 1.004, 1.005, 1.006,
        // Volume reads tenths of a decibel of a linear gain, and -0.05 dB is where the reading
        // rounds to zero at the top of its range.
        0.99425, 0.99426, 0.99427, 0.9999,
    ];

    /// **Every parameter's text survives the host's own conversion.** The CLAP wrapper formats a
    /// normalised value, parses the text back to a normalised value and formats that again, so a
    /// reading that chooses its unit or its sign from the raw value can print one text, parse to the
    /// other side of its own switch and print another — which `clap-validator`'s
    /// `param-conversions` fails only when its values land in that sliver, so a clean run proves
    /// nothing (mxm-kit's `docs/code-review-notes.md` §6). This walks every parameter, with the unit
    /// on as the host sees it, across clap-validator 0.4.1's own grid, the collection's `i / 19`
    /// grid, and the normalised neighbours of every value in [`BOUNDARIES`].
    #[test]
    fn every_parameter_text_is_idempotent_through_the_hosts_conversion() {
        let params = MxmPara07Params::default();
        let map = params.param_map();
        let validator_values = 4_000_usize.div_ceil(map.len()).clamp(5, 100);
        let mut failures: Vec<String> = Vec::new();
        for (id, ptr, _group) in &map {
            // SAFETY: `params` owns every parameter these pointers refer to and outlives the loop;
            // this is the same access `param-conversions` makes through CLAP.
            unsafe {
                // The wrapper hands CLAP `normalised × step count` and divides by it on the way in.
                let steps = ptr.step_count().unwrap_or(1) as f64;
                let from_clap = |value: f64| value as f32 / steps as f32;
                let grid = (0..=19).map(|i| (i as f32 / 19.0, "grid"));
                let validator = (0..validator_values).map(|i| {
                    let value = steps * (i as f64 / (validator_values - 1) as f64);
                    (from_clap(value), "validator grid")
                });
                let boundary = BOUNDARIES.into_iter().flat_map(|plain| {
                    let at = ptr.preview_normalized(plain).clamp(0.0, 1.0);
                    [at.next_down().max(0.0), at, at.next_up().min(1.0)].map(|n| (n, "boundary"))
                });
                for (value, from) in grid.chain(validator).chain(boundary) {
                    let first = ptr.normalized_value_to_string(value, true);
                    let second = ptr.string_to_normalized_value(&first).map(|parsed| {
                        ptr.normalized_value_to_string(from_clap(parsed as f64 * steps), true)
                    });
                    if second.as_deref() != Some(first.as_str()) {
                        let failure = format!("{id}: {first:?} reads back as {second:?}");
                        if failures
                            .last()
                            .is_none_or(|last| !last.starts_with(&failure))
                        {
                            let plain = ptr.preview_plain(value);
                            failures.push(format!("{failure} ({from}, plain {plain})"));
                        }
                    }
                }
            }
        }
        assert!(
            failures.is_empty(),
            "{} parameter texts changed through the host's conversion:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}
