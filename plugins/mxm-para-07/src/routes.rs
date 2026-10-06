//! mxm-para-07's routing parameters: one presence and one amount per *(target, source)* pair.
//!
//! `plans/plan-mxm-para-07-modulation.md` §5, under `plans/plan-modulation-routing.md` §4.3. The
//! derive needs concrete fields and this instrument's source list is its own, so the struct is
//! declared here rather than generated — the shape `mxm-mono-01`, `mxm-mono-03` and `mxm-poly-06`
//! use. What is shared is everything around these fields: [`mxm_modulation_params`] reads them, and
//! [`mxm_para_07_dsp::routing`] evaluates them.
//!
//! # Permanent ids
//!
//! One `#[nested(id_prefix = …)]` per target, so a pair's ids are `mod_<target>_<source>` and
//! `mod_<target>_<source>on`. **Permanent from here on**, like every id in this collection.
//!
//! # One pair is refused
//!
//! Nine targets offer all eighteen sources, the multiplier included — it may read itself, one
//! sample late like any backward route — **except VCA level ← Key**, which the modulation standard
//! refuses: a key parked off middle C is a constant gain that latches the VCA open
//! (`plans/plan-modulation-standard.md`). [`ROUTE_IDS`] stays rectangular, and [`TargetRoutes`]
//! registers only the pairs its target offers, so the refused pair's two ids are minted nowhere and
//! are retired (`crate::params::RETIRED_IDS`).
//!
//! **Fourteen ids are retired with the external input** (the owner, 2026-09-26): the follower
//! source's pair on every target, `mod_<target>_follow` and `mod_<target>_followon`. Never re-use
//! them.

use mxm_modulation::standard::Offer;
use mxm_modulation_params::Route;
use mxm_modulation_params::reading::{self, Fader, Reach};
use mxm_para_07_dsp::routing::{
    FULL_SCALE, Routing, SOURCE_NAMES, SOURCE_PEAK, SOURCES, TARGET_NAMES, TARGETS, init_amount,
    init_present, offer, source, target,
};
use nice_plug::prelude::*;

/// Every routing pair's two permanent ids, `(amount, presence)`, in `[target][source]` order.
///
/// **Written out rather than derived at runtime**, because a preset's parameter list is
/// `&'static str` and because these are permanent ids: they belong in the source where they can be
/// read, grepped and diffed. `tests::the_id_table_is_what_the_derive_actually_produces` holds this
/// table to what nice-plug actually emits.
pub const ROUTE_IDS: [[(&str, &str); SOURCES]; TARGETS] = [
    [
        ("mod_vco1pitch_key", "mod_vco1pitch_keyon"),
        ("mod_vco1pitch_vel", "mod_vco1pitch_velon"),
        ("mod_vco1pitch_wheel", "mod_vco1pitch_wheelon"),
        ("mod_vco1pitch_press", "mod_vco1pitch_presson"),
        ("mod_vco1pitch_bend", "mod_vco1pitch_bendon"),
        ("mod_vco1pitch_bendmag", "mod_vco1pitch_bendmagon"),
        ("mod_vco1pitch_pedal", "mod_vco1pitch_pedalon"),
        ("mod_vco1pitch_noise", "mod_vco1pitch_noiseon"),
        ("mod_vco1pitch_lfo", "mod_vco1pitch_lfoon"),
        ("mod_vco1pitch_lfoc", "mod_vco1pitch_lfocon"),
        ("mod_vco1pitch_lfotri", "mod_vco1pitch_lfotrion"),
        ("mod_vco1pitch_sh", "mod_vco1pitch_shon"),
        ("mod_vco1pitch_env1", "mod_vco1pitch_env1on"),
        ("mod_vco1pitch_env2", "mod_vco1pitch_env2on"),
        ("mod_vco1pitch_auto", "mod_vco1pitch_autoon"),
        ("mod_vco1pitch_mult", "mod_vco1pitch_multon"),
        ("mod_vco1pitch_vco1", "mod_vco1pitch_vco1on"),
        ("mod_vco1pitch_vco2", "mod_vco1pitch_vco2on"),
    ],
    [
        ("mod_vco2pitch_key", "mod_vco2pitch_keyon"),
        ("mod_vco2pitch_vel", "mod_vco2pitch_velon"),
        ("mod_vco2pitch_wheel", "mod_vco2pitch_wheelon"),
        ("mod_vco2pitch_press", "mod_vco2pitch_presson"),
        ("mod_vco2pitch_bend", "mod_vco2pitch_bendon"),
        ("mod_vco2pitch_bendmag", "mod_vco2pitch_bendmagon"),
        ("mod_vco2pitch_pedal", "mod_vco2pitch_pedalon"),
        ("mod_vco2pitch_noise", "mod_vco2pitch_noiseon"),
        ("mod_vco2pitch_lfo", "mod_vco2pitch_lfoon"),
        ("mod_vco2pitch_lfoc", "mod_vco2pitch_lfocon"),
        ("mod_vco2pitch_lfotri", "mod_vco2pitch_lfotrion"),
        ("mod_vco2pitch_sh", "mod_vco2pitch_shon"),
        ("mod_vco2pitch_env1", "mod_vco2pitch_env1on"),
        ("mod_vco2pitch_env2", "mod_vco2pitch_env2on"),
        ("mod_vco2pitch_auto", "mod_vco2pitch_autoon"),
        ("mod_vco2pitch_mult", "mod_vco2pitch_multon"),
        ("mod_vco2pitch_vco1", "mod_vco2pitch_vco1on"),
        ("mod_vco2pitch_vco2", "mod_vco2pitch_vco2on"),
    ],
    [
        ("mod_vco1pw_key", "mod_vco1pw_keyon"),
        ("mod_vco1pw_vel", "mod_vco1pw_velon"),
        ("mod_vco1pw_wheel", "mod_vco1pw_wheelon"),
        ("mod_vco1pw_press", "mod_vco1pw_presson"),
        ("mod_vco1pw_bend", "mod_vco1pw_bendon"),
        ("mod_vco1pw_bendmag", "mod_vco1pw_bendmagon"),
        ("mod_vco1pw_pedal", "mod_vco1pw_pedalon"),
        ("mod_vco1pw_noise", "mod_vco1pw_noiseon"),
        ("mod_vco1pw_lfo", "mod_vco1pw_lfoon"),
        ("mod_vco1pw_lfoc", "mod_vco1pw_lfocon"),
        ("mod_vco1pw_lfotri", "mod_vco1pw_lfotrion"),
        ("mod_vco1pw_sh", "mod_vco1pw_shon"),
        ("mod_vco1pw_env1", "mod_vco1pw_env1on"),
        ("mod_vco1pw_env2", "mod_vco1pw_env2on"),
        ("mod_vco1pw_auto", "mod_vco1pw_autoon"),
        ("mod_vco1pw_mult", "mod_vco1pw_multon"),
        ("mod_vco1pw_vco1", "mod_vco1pw_vco1on"),
        ("mod_vco1pw_vco2", "mod_vco1pw_vco2on"),
    ],
    [
        ("mod_vco2pw_key", "mod_vco2pw_keyon"),
        ("mod_vco2pw_vel", "mod_vco2pw_velon"),
        ("mod_vco2pw_wheel", "mod_vco2pw_wheelon"),
        ("mod_vco2pw_press", "mod_vco2pw_presson"),
        ("mod_vco2pw_bend", "mod_vco2pw_bendon"),
        ("mod_vco2pw_bendmag", "mod_vco2pw_bendmagon"),
        ("mod_vco2pw_pedal", "mod_vco2pw_pedalon"),
        ("mod_vco2pw_noise", "mod_vco2pw_noiseon"),
        ("mod_vco2pw_lfo", "mod_vco2pw_lfoon"),
        ("mod_vco2pw_lfoc", "mod_vco2pw_lfocon"),
        ("mod_vco2pw_lfotri", "mod_vco2pw_lfotrion"),
        ("mod_vco2pw_sh", "mod_vco2pw_shon"),
        ("mod_vco2pw_env1", "mod_vco2pw_env1on"),
        ("mod_vco2pw_env2", "mod_vco2pw_env2on"),
        ("mod_vco2pw_auto", "mod_vco2pw_autoon"),
        ("mod_vco2pw_mult", "mod_vco2pw_multon"),
        ("mod_vco2pw_vco1", "mod_vco2pw_vco1on"),
        ("mod_vco2pw_vco2", "mod_vco2pw_vco2on"),
    ],
    [
        ("mod_cutoff_key", "mod_cutoff_keyon"),
        ("mod_cutoff_vel", "mod_cutoff_velon"),
        ("mod_cutoff_wheel", "mod_cutoff_wheelon"),
        ("mod_cutoff_press", "mod_cutoff_presson"),
        ("mod_cutoff_bend", "mod_cutoff_bendon"),
        ("mod_cutoff_bendmag", "mod_cutoff_bendmagon"),
        ("mod_cutoff_pedal", "mod_cutoff_pedalon"),
        ("mod_cutoff_noise", "mod_cutoff_noiseon"),
        ("mod_cutoff_lfo", "mod_cutoff_lfoon"),
        ("mod_cutoff_lfoc", "mod_cutoff_lfocon"),
        ("mod_cutoff_lfotri", "mod_cutoff_lfotrion"),
        ("mod_cutoff_sh", "mod_cutoff_shon"),
        ("mod_cutoff_env1", "mod_cutoff_env1on"),
        ("mod_cutoff_env2", "mod_cutoff_env2on"),
        ("mod_cutoff_auto", "mod_cutoff_autoon"),
        ("mod_cutoff_mult", "mod_cutoff_multon"),
        ("mod_cutoff_vco1", "mod_cutoff_vco1on"),
        ("mod_cutoff_vco2", "mod_cutoff_vco2on"),
    ],
    [
        ("mod_amp_key", "mod_amp_keyon"),
        ("mod_amp_vel", "mod_amp_velon"),
        ("mod_amp_wheel", "mod_amp_wheelon"),
        ("mod_amp_press", "mod_amp_presson"),
        ("mod_amp_bend", "mod_amp_bendon"),
        ("mod_amp_bendmag", "mod_amp_bendmagon"),
        ("mod_amp_pedal", "mod_amp_pedalon"),
        ("mod_amp_noise", "mod_amp_noiseon"),
        ("mod_amp_lfo", "mod_amp_lfoon"),
        ("mod_amp_lfoc", "mod_amp_lfocon"),
        ("mod_amp_lfotri", "mod_amp_lfotrion"),
        ("mod_amp_sh", "mod_amp_shon"),
        ("mod_amp_env1", "mod_amp_env1on"),
        ("mod_amp_env2", "mod_amp_env2on"),
        ("mod_amp_auto", "mod_amp_autoon"),
        ("mod_amp_mult", "mod_amp_multon"),
        ("mod_amp_vco1", "mod_amp_vco1on"),
        ("mod_amp_vco2", "mod_amp_vco2on"),
    ],
    [
        ("mod_mult_key", "mod_mult_keyon"),
        ("mod_mult_vel", "mod_mult_velon"),
        ("mod_mult_wheel", "mod_mult_wheelon"),
        ("mod_mult_press", "mod_mult_presson"),
        ("mod_mult_bend", "mod_mult_bendon"),
        ("mod_mult_bendmag", "mod_mult_bendmagon"),
        ("mod_mult_pedal", "mod_mult_pedalon"),
        ("mod_mult_noise", "mod_mult_noiseon"),
        ("mod_mult_lfo", "mod_mult_lfoon"),
        ("mod_mult_lfoc", "mod_mult_lfocon"),
        ("mod_mult_lfotri", "mod_mult_lfotrion"),
        ("mod_mult_sh", "mod_mult_shon"),
        ("mod_mult_env1", "mod_mult_env1on"),
        ("mod_mult_env2", "mod_mult_env2on"),
        ("mod_mult_auto", "mod_mult_autoon"),
        ("mod_mult_mult", "mod_mult_multon"),
        ("mod_mult_vco1", "mod_mult_vco1on"),
        ("mod_mult_vco2", "mod_mult_vco2on"),
    ],
    [
        ("mod_amplitude_key", "mod_amplitude_keyon"),
        ("mod_amplitude_vel", "mod_amplitude_velon"),
        ("mod_amplitude_wheel", "mod_amplitude_wheelon"),
        ("mod_amplitude_press", "mod_amplitude_presson"),
        ("mod_amplitude_bend", "mod_amplitude_bendon"),
        ("mod_amplitude_bendmag", "mod_amplitude_bendmagon"),
        ("mod_amplitude_pedal", "mod_amplitude_pedalon"),
        ("mod_amplitude_noise", "mod_amplitude_noiseon"),
        ("mod_amplitude_lfo", "mod_amplitude_lfoon"),
        ("mod_amplitude_lfoc", "mod_amplitude_lfocon"),
        ("mod_amplitude_lfotri", "mod_amplitude_lfotrion"),
        ("mod_amplitude_sh", "mod_amplitude_shon"),
        ("mod_amplitude_env1", "mod_amplitude_env1on"),
        ("mod_amplitude_env2", "mod_amplitude_env2on"),
        ("mod_amplitude_auto", "mod_amplitude_autoon"),
        ("mod_amplitude_mult", "mod_amplitude_multon"),
        ("mod_amplitude_vco1", "mod_amplitude_vco1on"),
        ("mod_amplitude_vco2", "mod_amplitude_vco2on"),
    ],
    [
        ("mod_bank_key", "mod_bank_keyon"),
        ("mod_bank_vel", "mod_bank_velon"),
        ("mod_bank_wheel", "mod_bank_wheelon"),
        ("mod_bank_press", "mod_bank_presson"),
        ("mod_bank_bend", "mod_bank_bendon"),
        ("mod_bank_bendmag", "mod_bank_bendmagon"),
        ("mod_bank_pedal", "mod_bank_pedalon"),
        ("mod_bank_noise", "mod_bank_noiseon"),
        ("mod_bank_lfo", "mod_bank_lfoon"),
        ("mod_bank_lfoc", "mod_bank_lfocon"),
        ("mod_bank_lfotri", "mod_bank_lfotrion"),
        ("mod_bank_sh", "mod_bank_shon"),
        ("mod_bank_env1", "mod_bank_env1on"),
        ("mod_bank_env2", "mod_bank_env2on"),
        ("mod_bank_auto", "mod_bank_autoon"),
        ("mod_bank_mult", "mod_bank_multon"),
        ("mod_bank_vco1", "mod_bank_vco1on"),
        ("mod_bank_vco2", "mod_bank_vco2on"),
    ],
];

/// Each source's two ids inside a target's group, `(presence, amount)`, in declared source order —
/// what the group's `#[nested(id_prefix = …)]` prefixes.
pub const SHORT_IDS: [(&str, &str); SOURCES] = [
    ("keyon", "key"),
    ("velon", "vel"),
    ("wheelon", "wheel"),
    ("presson", "press"),
    ("bendon", "bend"),
    ("bendmagon", "bendmag"),
    ("pedalon", "pedal"),
    ("noiseon", "noise"),
    ("lfoon", "lfo"),
    ("lfocon", "lfoc"),
    ("lfotrion", "lfotri"),
    ("shon", "sh"),
    ("env1on", "env1"),
    ("env2on", "env2"),
    ("autoon", "auto"),
    ("multon", "mult"),
    ("vco1on", "vco1"),
    ("vco2on", "vco2"),
];

/// One target's routes: a presence and a signed amount for every source the instrument declares.
///
/// **Presence is the enable and the amount is the depth**, and nothing else: no selector, because a
/// pair *is* its source; no polarity switch, because the amount is signed — which is what the
/// retired `envpolarity` and `followerpolarity` became (D6). Declared in source order.
pub struct TargetRoutes {
    /// Which target this group is, for its offer. Not a parameter.
    target: usize,
    pub key_on: BoolParam,
    pub key: FloatParam,
    pub vel_on: BoolParam,
    pub vel: FloatParam,
    pub wheel_on: BoolParam,
    pub wheel: FloatParam,
    pub press_on: BoolParam,
    pub press: FloatParam,
    pub bend_on: BoolParam,
    pub bend: FloatParam,
    pub bendmag_on: BoolParam,
    pub bendmag: FloatParam,
    pub pedal_on: BoolParam,
    pub pedal: FloatParam,
    pub noise_on: BoolParam,
    pub noise: FloatParam,
    pub lfo_on: BoolParam,
    pub lfo: FloatParam,
    pub lfoc_on: BoolParam,
    pub lfoc: FloatParam,
    pub lfotri_on: BoolParam,
    pub lfotri: FloatParam,
    pub sh_on: BoolParam,
    pub sh: FloatParam,
    pub env1_on: BoolParam,
    pub env1: FloatParam,
    pub env2_on: BoolParam,
    pub env2: FloatParam,
    pub auto_on: BoolParam,
    pub auto: FloatParam,
    pub mult_on: BoolParam,
    pub mult: FloatParam,
    pub vco1_on: BoolParam,
    pub vco1: FloatParam,
    pub vco2_on: BoolParam,
    pub vco2: FloatParam,
}

/// A route amount: signed, and starting where the init patch puts it — zero for every route but the
/// three at full and the bender's two semitones (`mxm_para_07_dsp::routing::init_amount`).
///
/// Smoothed at this instrument's own 10 ms, the smoothing its depth controls already have. It is
/// the collection's one route parameter (`mxm_modulation_params::reading`), on the travel the pair's
/// offer allows — VCA level ← Velocity closes only, so it travels 0…+100 % — and it reads as
/// [`reach`] says.
fn amount(target: usize, source: usize) -> FloatParam {
    reading::amount_param_at(
        format!("{} from {}", TARGET_NAMES[target], SOURCE_NAMES[source]),
        init_amount(target, source),
        reach(target, source),
        Fader::for_offer(offer(target, source), false),
        10.0,
    )
}

/// What a route reads: **what its pair delivers at this amount with its source at its peak, in the
/// target's own unit** — semitones of pitch, octaves of cutoff, and a percentage of the control for
/// the pulse width, VCA level, the multiplier's factor and Amplitude. So the machine's own routes
/// read what they always delivered: +12.00 st of LFO pitch, +15.00 st from the bender in Direct,
/// +10.21 oct of filter envelope, and +100 % of the VCA from Envelope 1, which has no attenuator;
/// and a route the machine never had reads the collection's standard reach, +12.00 st.
///
/// **A Key route reads per octave of keyboard**, never at the keyboard's peak: five and a half
/// octaves times a reach is a number no player can act on. Key is published in octaves already,
/// so its column *is* the per-octave figure.
fn reach(target: usize, source: usize) -> Reach {
    use mxm_para_07_dsp::routing::{
        ADDED_SCALE, AMPLITUDE_SCALE, BANK_LEVEL_SCALE, FRAME_SCALE, PULSE_WIDTH_SWING,
    };
    let unit = match target {
        target::CUTOFF => reading::OCTAVES,
        target::VCO_1_PITCH | target::VCO_2_PITCH => reading::SEMITONES,
        _ => reading::PERCENT,
    };
    let full = match (target, ADDED_SCALE[target][source]) {
        (target::AMPLITUDE, _) => AMPLITUDE_SCALE[source] / FRAME_SCALE,
        (target::BANK_LEVEL, _) => BANK_LEVEL_SCALE[source] / FRAME_SCALE,
        (_, Some(added)) => added / FRAME_SCALE,
        _ => FULL_SCALE[target][source],
    };
    // **A pulse width reads in percent of the cycle**, the standard width's unit: `m` moves it by
    // `PULSE_WIDTH_SWING` a unit.
    let full = if matches!(
        target,
        target::VCO_1_PULSE_WIDTH | target::VCO_2_PULSE_WIDTH
    ) {
        full * PULSE_WIDTH_SWING
    } else {
        full
    };
    if source == source::KEY {
        Reach::per_octave(full, unit)
    } else {
        Reach::new(full * SOURCE_PEAK[source], unit)
    }
}

/// Whether a route exists. **Configuration, not an amount**, so its default is the machine's own
/// wiring: the thirteen pairs the panel's own sliders are, and the three the machine wires at full.
///
/// **Both lists, because on this instrument they are disjoint** — unlike `mxm-mono-03`, where the
/// full-amount routes are a subset. Consulting only one would leave Init without its amplifier
/// envelope and without either of the multiplier's factors.
fn present(target: usize, source: usize) -> BoolParam {
    BoolParam::new(
        format!("{} from {} on", TARGET_NAMES[target], SOURCE_NAMES[source]),
        init_present(target, source),
    )
}

/// **Only the pairs a target offers are parameters.** Written out rather than derived, because the
/// derive registers every field and the modulation standard refuses VCA level ← Key: its two
/// fields exist in memory, and no host, state or preset ever sees them. Everything else is what the
/// derive produced — the same ids, in the same order.
unsafe impl Params for TargetRoutes {
    fn param_map(&self) -> Vec<(String, ParamPtr, String)> {
        let mut map = Vec::with_capacity(SOURCES * 2);
        for (source, (presence, amount)) in SHORT_IDS.iter().enumerate() {
            if offer(self.target, source) == Offer::Refused {
                continue;
            }
            map.push((
                (*presence).to_owned(),
                self.presence_param(source).as_ptr(),
                String::new(),
            ));
            map.push((
                (*amount).to_owned(),
                self.amount_param(source).as_ptr(),
                String::new(),
            ));
        }
        map
    }
}

impl TargetRoutes {
    /// Every pair for one target, at the init patch.
    pub fn new(target: usize) -> Self {
        Self {
            target,
            key_on: present(target, 0),
            key: amount(target, 0),
            vel_on: present(target, 1),
            vel: amount(target, 1),
            wheel_on: present(target, 2),
            wheel: amount(target, 2),
            press_on: present(target, 3),
            press: amount(target, 3),
            bend_on: present(target, 4),
            bend: amount(target, 4),
            bendmag_on: present(target, 5),
            bendmag: amount(target, 5),
            pedal_on: present(target, 6),
            pedal: amount(target, 6),
            noise_on: present(target, 7),
            noise: amount(target, 7),
            lfo_on: present(target, 8),
            lfo: amount(target, 8),
            lfoc_on: present(target, 9),
            lfoc: amount(target, 9),
            lfotri_on: present(target, 10),
            lfotri: amount(target, 10),
            sh_on: present(target, 11),
            sh: amount(target, 11),
            env1_on: present(target, 12),
            env1: amount(target, 12),
            env2_on: present(target, 13),
            env2: amount(target, 13),
            auto_on: present(target, 14),
            auto: amount(target, 14),
            mult_on: present(target, 15),
            mult: amount(target, 15),
            vco1_on: present(target, 16),
            vco1: amount(target, 16),
            vco2_on: present(target, 17),
            vco2: amount(target, 17),
        }
    }

    /// This target's routes in **declared source order**. `target` is its index, because each row's
    /// keyboard scope is its parameter's permanent id and those live in [`ROUTE_IDS`], keyed by
    /// target.
    pub fn routes(&self, target: usize) -> Vec<Route<'_>> {
        let ids = ROUTE_IDS[target];
        let pairs: [(&BoolParam, &FloatParam); SOURCES] = [
            (&self.key_on, &self.key),
            (&self.vel_on, &self.vel),
            (&self.wheel_on, &self.wheel),
            (&self.press_on, &self.press),
            (&self.bend_on, &self.bend),
            (&self.bendmag_on, &self.bendmag),
            (&self.pedal_on, &self.pedal),
            (&self.noise_on, &self.noise),
            (&self.lfo_on, &self.lfo),
            (&self.lfoc_on, &self.lfoc),
            (&self.lfotri_on, &self.lfotri),
            (&self.sh_on, &self.sh),
            (&self.env1_on, &self.env1),
            (&self.env2_on, &self.env2),
            (&self.auto_on, &self.auto),
            (&self.mult_on, &self.mult),
            (&self.vco1_on, &self.vco1),
            (&self.vco2_on, &self.vco2),
        ];
        // A refused pair is not a route: the stack never lists it and its menu never offers it.
        (0..SOURCES)
            .filter(|&s| offer(target, s) != Offer::Refused)
            .map(|s| Route {
                source: SOURCE_NAMES[s],
                present: pairs[s].0,
                amount: pairs[s].1,
                present_id: ids[s].1,
                amount_id: ids[s].0,
            })
            .collect()
    }

    /// Whether each of this target's routes exists, by source — a refused pair never. Read **once per
    /// interval**, never per sample.
    pub fn presences(&self, target: usize) -> [bool; SOURCES] {
        std::array::from_fn(|s| {
            offer(target, s) != Offer::Refused && self.presence_param(s).value()
        })
    }

    /// One source's amount parameter, by index, in declared source order — a `match` rather than an
    /// array of references, so a source nothing reads costs a branch and no pointer stores.
    #[inline]
    pub fn amount_param(&self, source: usize) -> &FloatParam {
        match source {
            0 => &self.key,
            1 => &self.vel,
            2 => &self.wheel,
            3 => &self.press,
            4 => &self.bend,
            5 => &self.bendmag,
            6 => &self.pedal,
            7 => &self.noise,
            8 => &self.lfo,
            9 => &self.lfoc,
            10 => &self.lfotri,
            11 => &self.sh,
            12 => &self.env1,
            13 => &self.env2,
            14 => &self.auto,
            15 => &self.mult,
            16 => &self.vco1,
            _ => &self.vco2,
        }
    }

    /// One source's presence parameter, by index — [`TargetRoutes::amount_param`]'s mirror.
    ///
    /// A [`Route`] carries its presence as an `ErasedParam`, which is enough to read and to draw
    /// but not to set, so a caller that has to *change* a presence needs the concrete parameter.
    #[inline]
    pub fn presence_param(&self, source: usize) -> &BoolParam {
        match source {
            0 => &self.key_on,
            1 => &self.vel_on,
            2 => &self.wheel_on,
            3 => &self.press_on,
            4 => &self.bend_on,
            5 => &self.bendmag_on,
            6 => &self.pedal_on,
            7 => &self.noise_on,
            8 => &self.lfo_on,
            9 => &self.lfoc_on,
            10 => &self.lfotri_on,
            11 => &self.sh_on,
            12 => &self.env1_on,
            13 => &self.env2_on,
            14 => &self.auto_on,
            15 => &self.mult_on,
            16 => &self.vco1_on,
            _ => &self.vco2_on,
        }
    }

    /// Snaps a newly present route's smoother to its stored value.
    ///
    /// **An absent route's smoother is not advanced, so it must not be resumed either.** While the
    /// pair was absent nothing called `next()`, but the parameter stayed editable: a host automating
    /// it, or a preset load, moves the *target* and leaves the smoother wherever the last live sample
    /// left it. Resuming from there ramps the route in from a stale number.
    pub fn arm(&self, newly_present: &[bool; SOURCES]) {
        for (source, &now) in newly_present.iter().enumerate() {
            if now {
                let param = self.amount_param(source);
                param.smoothed.reset(param.value());
            }
        }
    }
}

/// All nine targets' routes.
#[derive(Params)]
pub struct Routes {
    #[nested(id_prefix = "mod_vco1pitch", group = "Modulation - VCO-1 pitch")]
    pub vco1pitch: TargetRoutes,
    #[nested(id_prefix = "mod_vco2pitch", group = "Modulation - VCO-2 pitch")]
    pub vco2pitch: TargetRoutes,
    #[nested(id_prefix = "mod_vco1pw", group = "Modulation - VCO-1 pulse width")]
    pub vco1pw: TargetRoutes,
    #[nested(id_prefix = "mod_vco2pw", group = "Modulation - VCO-2 pulse width")]
    pub vco2pw: TargetRoutes,
    #[nested(id_prefix = "mod_cutoff", group = "Modulation - Cutoff")]
    pub cutoff: TargetRoutes,
    /// **VCA level**: the machine's own gain CV. Its ids keep the `mod_amp` its first name gave them.
    #[nested(id_prefix = "mod_amp", group = "Modulation - VCA level")]
    pub amp: TargetRoutes,
    #[nested(id_prefix = "mod_mult", group = "Modulation - Multiplier")]
    pub mult: TargetRoutes,
    /// **The collection's standard Amplitude**, after the VCA — added by the modulation standard
    /// (2026-09-26), every pair absent at Init.
    #[nested(id_prefix = "mod_amplitude", group = "Modulation - Amplitude")]
    pub amplitude: TargetRoutes,
    /// **The register bank's Level** — not on the machine; the owner's ask of 2026-09-27. Every pair
    /// absent at Init.
    #[nested(id_prefix = "mod_bank", group = "Modulation - Register bank level")]
    pub bank: TargetRoutes,
}

impl Default for Routes {
    fn default() -> Self {
        Self::new()
    }
}

impl Routes {
    /// The init patch: the panel's own sliders present at zero, and the three the machine wires at
    /// full — the amplifier's envelope and the multiplier's two factors.
    pub fn new() -> Self {
        Self {
            vco1pitch: TargetRoutes::new(target::VCO_1_PITCH),
            vco2pitch: TargetRoutes::new(target::VCO_2_PITCH),
            vco1pw: TargetRoutes::new(target::VCO_1_PULSE_WIDTH),
            vco2pw: TargetRoutes::new(target::VCO_2_PULSE_WIDTH),
            cutoff: TargetRoutes::new(target::CUTOFF),
            amp: TargetRoutes::new(target::VCA_LEVEL),
            mult: TargetRoutes::new(target::MULTIPLIER),
            amplitude: TargetRoutes::new(target::AMPLITUDE),
            bank: TargetRoutes::new(target::BANK_LEVEL),
        }
    }

    /// The nine targets, in declared target order.
    pub fn each(&self) -> [&TargetRoutes; TARGETS] {
        [
            &self.vco1pitch,
            &self.vco2pitch,
            &self.vco1pw,
            &self.vco2pw,
            &self.cutoff,
            &self.amp,
            &self.mult,
            &self.amplitude,
            &self.bank,
        ]
    }

    /// Which routes are live, for the whole instrument. Once per interval.
    pub fn topology(&self) -> Routing {
        let mut routing = Routing::new();
        for (index, (slot, group)) in routing.present.iter_mut().zip(self.each()).enumerate() {
            *slot = group.presences(index);
        }
        routing.compact();
        routing
    }

    /// The topology for this interval, with every **newly present** route's smoother snapped to its
    /// stored value. `previous` is the topology the last interval ran, which the caller keeps.
    pub fn topology_from(&self, previous: &Routing) -> Routing {
        let routing = self.topology();
        for (index, group) in self.each().into_iter().enumerate() {
            let mut newly = [false; SOURCES];
            for (slot, (&now, &before)) in newly.iter_mut().zip(
                routing.present[index]
                    .iter()
                    .zip(previous.present[index].iter()),
            ) {
                *slot = now && !before;
            }
            group.arm(&newly);
        }
        routing
    }

    /// Fills this sample's amounts into an already-topologised [`Routing`]: **each live route's
    /// smoother advanced once**, and an absent route's left exactly where the player put it.
    #[inline]
    pub fn advance(&self, routing: &mut Routing) {
        let targets = self.each();
        for i in 0..routing.live().len() {
            let (t, s) = routing.live()[i];
            routing.amounts[t as usize][s as usize] =
                targets[t as usize].amount_param(s as usize).smoothed.next();
        }
    }

    /// Every routing parameter, named by its permanent id, for the preset layer. **Presets carry
    /// routing**, presences included, or a sound would load with somebody else's routes still in it.
    pub fn parameters(&self) -> Vec<(&'static str, &dyn mxm_preset::ErasedParam)> {
        let mut out = Vec::with_capacity(TARGETS * SOURCES * 2);
        // A refused pair is no parameter, so it is not in a preset either.
        for (index, (group, ids)) in self.each().into_iter().zip(ROUTE_IDS).enumerate() {
            for (source, (amount, presence)) in ids.into_iter().enumerate() {
                if offer(index, source) == Offer::Refused {
                    continue;
                }
                out.push((
                    amount,
                    group.amount_param(source) as &dyn mxm_preset::ErasedParam,
                ));
                out.push((presence, group.presence_param(source)));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nice_plug::params::Params;

    use mxm_plugin_test::routing_checks;

    /// **Every route parameter says what the DSP does** — the modulation standard's plugin half:
    /// a pair has a registered parameter exactly where it is offered (VCA level ← Key nowhere), its
    /// travel is its offer's, its reading carries its target's unit and states what
    /// `mxm_para_07_dsp::conformance` measures the graph delivering, and every reading survives the
    /// host's round trip.
    ///
    /// Falsified before trusted: with the added pitch pairs read at twice the DSP's reach, it names
    /// all ten from a performance source; with the refused pair registered, it names that one.
    #[test]
    fn every_route_parameter_says_what_the_dsp_does() {
        let params = crate::params::MxmPara07Params::default();
        let registered: std::collections::BTreeSet<String> = params
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        let groups = params.routes.each();
        if let Err(failures) =
            routing_checks::amounts(&mxm_para_07_dsp::conformance::Declared, |target, source| {
                registered
                    .contains(ROUTE_IDS[target][source].0)
                    .then(|| groups[target].amount_param(source))
            })
        {
            panic!(
                "{} failure(s):
{}",
                failures.len(),
                failures.join(
                    "
"
                )
            );
        }
    }

    /// [`ROUTE_IDS`] names exactly what the derive produces, and nothing else.
    #[test]
    fn the_id_table_is_what_the_derive_actually_produces() {
        let params = crate::params::MxmPara07Params::default();
        let real: std::collections::BTreeSet<String> = params
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .filter(|id| id.starts_with("mod_"))
            .collect();
        let named: std::collections::BTreeSet<String> = ROUTE_IDS
            .iter()
            .enumerate()
            .flat_map(|(t, row)| {
                row.iter()
                    .enumerate()
                    .filter(move |&(s, _)| offer(t, s) != Offer::Refused)
                    .map(|(_, ids)| *ids)
            })
            .flat_map(|(amount, presence)| [amount.to_owned(), presence.to_owned()])
            .collect();
        assert_eq!(named, real, "ROUTE_IDS has drifted from the registered ids");
        // One pair refused: VCA level ← Key.
        assert_eq!(named.len(), (TARGETS * SOURCES - 1) * 2);
    }

    /// **The panel's init wiring and the DSP's declaration are the same sixteen routes.**
    #[test]
    fn a_default_panel_is_the_declared_init_routing() {
        let routes = Routes::new();
        let topology = routes.topology();
        let declared = Routing::init();
        assert_eq!(topology.present, declared.present);
        assert_eq!(topology.live(), declared.live());
        assert_eq!(
            topology.live().len(),
            mxm_para_07_dsp::routing::INIT_PRESENT.len()
                + mxm_para_07_dsp::routing::INIT_AT_FULL.len()
                + mxm_para_07_dsp::routing::INIT_BEND.len()
        );
    }

    /// **A route reads what its pair delivers**, in the target's unit and against its source's peak —
    /// and a reading typed back in lands on the amount it came from. The one defect a player meets on
    /// the first knob they turn, and no audio assertion can see it.
    #[test]
    fn a_route_reads_what_its_pair_delivers_and_reads_back() {
        use mxm_preset::ErasedParam;
        let r = Routes::new();
        let cases: [(&FloatParam, f32, &str); 10] = [
            (&r.vco1pitch.lfo, 1.0, "+12.00 st"),
            (&r.vco1pitch.sh, 1.0, "+24.00 st"),
            (&r.vco1pitch.bend, 1.0, "+15.00 st"),
            (&r.vco1pitch.mult, 1.0, "+10.00 st"),
            (&r.vco1pitch.lfo, 0.5, "+0.00 st"),
            (&r.cutoff.env1, 1.0, "+10.21 oct"),
            (&r.cutoff.key, 1.0, "+1.00 oct/oct"),
            (&r.amp.env1, 1.0, "+100 %"),
            // The machine's own PWM at full narrows the pulse from square to a tenth: 40 % of the
            // cycle; the wheel, a pair it never had, the standard width's 45 %.
            (&r.vco1pw.lfotri, 1.0, "+40 %"),
            (&r.vco1pw.wheel, 1.0, "+45 %"),
        ];
        for (param, normalised, expected) in cases {
            let text = ErasedParam::format(param, normalised);
            assert_eq!(text, expected, "{}", ErasedParam::name(param));
            let back = param
                .string_to_normalized_value(&text)
                .expect("the reading parses");
            assert!(
                (back - normalised).abs() < 1e-3,
                "{}: {text} read back as {back}",
                ErasedParam::name(param)
            );
        }
    }

    /// **Every route's reading reads back as itself, amounts that round to zero included.** A value
    /// that rounded to zero printed `-0 %`, which parses to zero and prints `+0 %`: text that is not
    /// idempotent through a host's conversion, which `clap-validator`'s `param-conversions` rejects.
    #[test]
    fn every_reading_survives_the_hosts_round_trip_a_rounded_zero_included() {
        use mxm_preset::ErasedParam;
        let r = Routes::new();
        for group in r.each() {
            for s in 0..SOURCES {
                let param = group.amount_param(s);
                for normalised in [
                    0.0f32, 0.25, 0.4999, 0.49999, 0.5, 0.50001, 0.5001, 0.75, 1.0,
                ] {
                    let text = ErasedParam::format(param, normalised);
                    let back = param
                        .string_to_normalized_value(&text)
                        .expect("the reading parses");
                    assert_eq!(
                        text,
                        ErasedParam::format(param, back),
                        "{} at {normalised}",
                        ErasedParam::name(param)
                    );
                }
            }
        }
    }

    /// **Remove, edit while absent, re-add: the route arrives at the depth the player set.**
    #[test]
    fn a_re_added_route_arrives_at_its_stored_depth_rather_than_ramping_from_a_stale_one() {
        use nice_plug::params::InternalParamMut;

        let routes = Routes::new();
        assert!(!routes.cutoff.wheel_on.value(), "this pair starts absent");

        const RATE: f32 = 48_000.0;
        unsafe {
            routes.cutoff.wheel_on._internal_set_plain_value(true);
            routes.cutoff.wheel._internal_set_plain_value(0.9);
            routes.cutoff.wheel._internal_update_smoother(RATE, true);
        }
        let routing = routes.topology_from(&Routing::new());
        let mut amounts = routing;
        routes.advance(&mut amounts);
        assert_eq!(amounts.amounts[target::CUTOFF][source::WHEEL], 0.9);

        unsafe {
            routes.cutoff.wheel_on._internal_set_plain_value(false);
        }
        let absent = routes.topology_from(&routing);
        unsafe {
            routes.cutoff.wheel._internal_set_plain_value(-0.4);
            routes.cutoff.wheel._internal_update_smoother(RATE, false);
        }
        assert!(
            routes.cutoff.wheel.smoothed.is_smoothing(),
            "the edit must leave the smoother mid-ramp, or this proves nothing"
        );

        unsafe {
            routes.cutoff.wheel_on._internal_set_plain_value(true);
        }
        let mut back = routes.topology_from(&absent);
        routes.advance(&mut back);
        assert_eq!(
            back.amounts[target::CUTOFF][source::WHEEL],
            -0.4,
            "a re-added route must arrive at its stored depth"
        );
    }
}
