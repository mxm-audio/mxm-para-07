//! **Every state the 28 retired controls could reach is still reachable** — decision 1.13 and D7 of
//! `plans/plan-mxm-para-07-modulation.md` §5, demonstrated as a property rather than asserted.
//!
//! The laws below are this voice's own arithmetic as it stood at `4471dab`, before the four sums
//! were converted. They are **written out here rather than called**, and that is not duplication
//! for its own sake: the panel controls they describe no longer exist, so there is no production
//! mapping left to drive. `mxm-mono-02` carries the same shape for the same reason.
//!
//! Each randomised legacy patch is translated into routes and the two are compared **at the values
//! the targets receive** — semitones of pitch, the pulse width's own control, octaves of cutoff and
//! the linear gain above HOLD — with the sources the voice would publish.
//!
//! **Twenty-six of them now.** `follower` and `followerpolarity` reached the cutoff from the external
//! input's envelope follower, and the owner removed the external input and everything on it on
//! 2026-09-26, so there is no longer a path for either to reach.

use mxm_para_07_dsp::Rng;
use mxm_para_07_dsp::routing::{Graph, Routing, source, target};
use mxm_para_07_dsp::voice::{
    AUTO_BEND_MAX_SEMITONES, BENDER_FILTER_OCTAVES, BENDER_VCO_CV_MAX_SEMITONES,
    BENDER_VCO_LFO_MAX_SEMITONES, FILTER_AUDIO_OCTAVES, FILTER_ENV_OCTAVES, FILTER_ENV_WEIGHT,
    FILTER_MOD_OCTAVES, VCO_LFO_SEMITONES, VCO_SH_SEMITONES,
};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Pwm {
    Manual,
    Lfo,
    Env1,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Bend {
    Off,
    Direct,
    Lfo,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Mod {
    Lfo,
    Sh,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Audio {
    Vco2,
    Noise,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Track {
    Keyboard,
    Pedal,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Env {
    One,
    Two,
}

/// The retired controls, and the two kept ones whose meaning depended on them.
#[derive(Clone, Copy, Debug)]
struct Legacy {
    vco1_lfo: f32,
    vco1_sh: f32,
    vco1_auto: f32,
    vco2_lfo: f32,
    vco2_sh: f32,
    vco2_auto: f32,
    width1: f32,
    pwm1: f32,
    pwm1_source: Pwm,
    width2: f32,
    pwm2: f32,
    pwm2_source: Pwm,
    filter_env: f32,
    env_positive: bool,
    filter_mod: f32,
    mod_source: Mod,
    key_track: f32,
    track_source: Track,
    filter_audio: f32,
    audio_source: Audio,
    vca_env: Env,
    vca_lfo: f32,
    bend_vco: f32,
    bend_vco_mode: Bend,
    bend_filter: f32,
    bend_filter_mode: Bend,
    bend_vca: f32,
    bend_vca_mode: Bend,
}

/// What the voice would publish on one sample.
#[derive(Clone, Copy, Debug)]
struct Sources {
    lfo_plus: f32,
    lfo_centred: f32,
    lfo_triangle: f32,
    sample_hold: f32,
    envelope_1: f32,
    envelope_2: f32,
    noise: f32,
    vco_2: f32,
    auto_bend: f32,
    bend: f32,
    key: f32,
    pedal: f32,
}

/// The bender's one law: Direct is signed, LFO rectifies the lever and gates the centred LFO.
fn bend_term(mode: Bend, s: &Sources, depth: f32, scale: f32) -> f32 {
    match mode {
        Bend::Direct => s.bend * depth.clamp(0.0, 1.0) * scale,
        Bend::Off => 0.0,
        Bend::Lfo => s.bend.abs() * depth.clamp(0.0, 1.0) * s.lfo_centred * scale,
    }
}

fn vco_bend(mode: Bend, s: &Sources, depth: f32) -> f32 {
    let scale = match mode {
        Bend::Direct => BENDER_VCO_CV_MAX_SEMITONES,
        Bend::Lfo => BENDER_VCO_LFO_MAX_SEMITONES,
        Bend::Off => 0.0,
    };
    bend_term(mode, s, depth, scale)
}

/// One lane's pitch modulation in semitones, above the lane's own base.
fn legacy_pitch(l: &Legacy, s: &Sources, lane_lfo: f32, lane_sh: f32, lane_auto: f32) -> f32 {
    vco_bend(l.bend_vco_mode, s, l.bend_vco)
        + lane_lfo.clamp(0.0, 1.0) * s.lfo_plus * VCO_LFO_SEMITONES
        + lane_sh.clamp(0.0, 1.0) * s.sample_hold * VCO_SH_SEMITONES
        + lane_auto.clamp(0.0, 1.0) * s.auto_bend * AUTO_BEND_MAX_SEMITONES
}

/// The legacy `m`: the 0…1 control the one-sided clamp consumed. **One field served two meanings** —
/// the width knob in Manual, the depth knob otherwise — which is the behaviour change §5 names.
fn legacy_width(source: Pwm, width: f32, depth: f32, s: &Sources) -> f32 {
    match source {
        Pwm::Manual => width,
        Pwm::Lfo => depth * s.lfo_triangle,
        Pwm::Env1 => depth * s.envelope_1,
    }
}

fn legacy_cutoff(l: &Legacy, s: &Sources) -> f32 {
    let env_sign = if l.env_positive { 1.0 } else { -1.0 };
    let mod_signal = match l.mod_source {
        Mod::Lfo => s.lfo_plus,
        Mod::Sh => s.sample_hold,
    };
    let audio = match l.audio_source {
        Audio::Vco2 => s.vco_2,
        Audio::Noise => s.noise,
    };
    let amount = l.key_track.clamp(0.0, 1.0);
    // Pedal mode kept the fixed 1 V/oct key line *and* added the pedal at the slider.
    let tracking = match l.track_source {
        Track::Keyboard => s.key * amount,
        Track::Pedal => s.key + s.pedal * amount,
    };
    env_sign * s.envelope_1 * l.filter_env.clamp(0.0, 1.0) * FILTER_ENV_OCTAVES * FILTER_ENV_WEIGHT
        + mod_signal * l.filter_mod.clamp(0.0, 1.0) * FILTER_MOD_OCTAVES
        + tracking
        + audio * l.filter_audio.clamp(0.0, 1.0) * FILTER_AUDIO_OCTAVES
        + bend_term(l.bend_filter_mode, s, l.bend_filter, BENDER_FILTER_OCTAVES)
}

/// The linear gain above HOLD, which stays the target's own base.
fn legacy_amplitude(l: &Legacy, s: &Sources) -> f32 {
    let env = match l.vca_env {
        Env::One => s.envelope_1,
        Env::Two => s.envelope_2,
    };
    env + l.vca_lfo.clamp(0.0, 1.0) * s.lfo_centred + bend_term(l.bend_vca_mode, s, l.bend_vca, 1.0)
}

/// Which source a bender route reads in each mode.
fn bender_source(mode: Bend) -> Option<usize> {
    match mode {
        Bend::Direct => Some(source::BEND),
        Bend::Lfo => Some(source::MULTIPLIER),
        Bend::Off => None,
    }
}

/// §5's table: one legacy state to exactly one route set, plus each width target's base.
fn translate(l: &Legacy) -> (Routing, f32, f32) {
    let mut r = Routing::new();
    let mut base = [0.0f32; 2];
    {
        let mut set = |t: usize, s: usize, a: f32| {
            assert!(
                (-1.0..=1.0).contains(&a),
                "the translated amount {a} into ({t}, {s}) is outside what a route can hold"
            );
            r.present[t][s] = true;
            r.amounts[t][s] = a;
        };

        for (pitch, lfo, sh, auto) in [
            (target::VCO_1_PITCH, l.vco1_lfo, l.vco1_sh, l.vco1_auto),
            (target::VCO_2_PITCH, l.vco2_lfo, l.vco2_sh, l.vco2_auto),
        ] {
            set(pitch, source::LFO, lfo);
            set(pitch, source::SAMPLE_HOLD, sh);
            set(pitch, source::AUTO_BEND, auto);
            if let Some(from) = bender_source(l.bend_vco_mode) {
                set(pitch, from, l.bend_vco);
            }
        }

        for (i, (width_target, source_of, width, depth)) in [
            (target::VCO_1_PULSE_WIDTH, l.pwm1_source, l.width1, l.pwm1),
            (target::VCO_2_PULSE_WIDTH, l.pwm2_source, l.width2, l.pwm2),
        ]
        .into_iter()
        .enumerate()
        {
            match source_of {
                // The knob is the base and no route carries anything.
                Pwm::Manual => base[i] = width,
                Pwm::Lfo => set(width_target, source::LFO_TRIANGLE, depth),
                Pwm::Env1 => set(width_target, source::ENVELOPE_1, depth),
            }
        }

        // The polarity switches are the sign of their own amount (D6).
        let signed = |positive: bool, depth: f32| if positive { depth } else { -depth };
        set(
            target::CUTOFF,
            source::ENVELOPE_1,
            signed(l.env_positive, l.filter_env),
        );
        match l.mod_source {
            Mod::Lfo => set(target::CUTOFF, source::LFO, l.filter_mod),
            Mod::Sh => set(target::CUTOFF, source::SAMPLE_HOLD, l.filter_mod),
        }
        match l.audio_source {
            Audio::Vco2 => set(target::CUTOFF, source::VCO_2, l.filter_audio),
            Audio::Noise => set(target::CUTOFF, source::NOISE, l.filter_audio),
        }
        // Pedal mode was two routes: the fixed 1 V/oct key line stays at full beside the pedal.
        match l.track_source {
            Track::Keyboard => set(target::CUTOFF, source::KEY, l.key_track),
            Track::Pedal => {
                set(target::CUTOFF, source::KEY, 1.0);
                set(target::CUTOFF, source::PEDAL, l.key_track);
            }
        }
        if let Some(from) = bender_source(l.bend_filter_mode) {
            set(target::CUTOFF, from, l.bend_filter);
        }

        match l.vca_env {
            Env::One => set(target::VCA_LEVEL, source::ENVELOPE_1, 1.0),
            Env::Two => set(target::VCA_LEVEL, source::ENVELOPE_2, 1.0),
        }
        set(target::VCA_LEVEL, source::LFO_CENTRED, l.vca_lfo);
        if let Some(from) = bender_source(l.bend_vca_mode) {
            set(target::VCA_LEVEL, from, l.bend_vca);
        }

        // `product`'s neutral is one, so a factor present at zero would make the module publish the
        // same thing whatever the bender did.
        set(target::MULTIPLIER, source::BEND_MAGNITUDE, 1.0);
        set(target::MULTIPLIER, source::LFO_CENTRED, 1.0);
    }
    r.compact();
    (r, base[0], base[1])
}

/// The routes, through the graph the voice uses, reading the sources the voice would publish.
fn routed(r: &Routing, s: &Sources) -> [f32; 5] {
    let mut graph = Graph::new();
    graph.set_topology(r);
    graph.begin_sample();
    graph.write(source::KEY, s.key);
    graph.write(source::PEDAL, s.pedal);
    graph.write(source::NOISE, s.noise);
    graph.write(source::LFO, s.lfo_plus);
    graph.write(source::LFO_CENTRED, s.lfo_centred);
    graph.write(source::LFO_TRIANGLE, s.lfo_triangle);
    graph.write(source::SAMPLE_HOLD, s.sample_hold);
    graph.write(source::ENVELOPE_1, s.envelope_1);
    graph.write(source::ENVELOPE_2, s.envelope_2);
    graph.write(source::AUTO_BEND, s.auto_bend);
    graph.write(source::BEND, s.bend);
    graph.write(source::BEND_MAGNITUDE, s.bend.abs());
    graph.write(source::VCO_2, s.vco_2);
    // The module is evaluated once every source it can read has published, exactly as the voice
    // does it, and its result publishes as a source like any other.
    let multiplier = graph.product(r);
    graph.write(source::MULTIPLIER, multiplier);
    [
        graph.sum(target::VCO_1_PITCH, r),
        graph.sum(target::VCO_2_PITCH, r),
        graph.sum(target::VCO_1_PULSE_WIDTH, r),
        graph.sum(target::CUTOFF, r),
        graph.sum(target::VCA_LEVEL, r),
    ]
}

#[test]
fn every_legacy_state_is_reachable_by_routes() {
    let mut rng = Rng::new(0x5eed_0007);
    let mut unit = move || 0.5 * (rng.next_bipolar() + 1.0);
    let pick = |u: f32, n: usize| ((u * n as f32) as usize).min(n - 1);
    let mut worst = [0.0f32; 5];

    for trial in 0..20_000 {
        let pwm = [Pwm::Manual, Pwm::Lfo, Pwm::Env1];
        let bends = [Bend::Off, Bend::Direct, Bend::Lfo];
        let l = Legacy {
            vco1_lfo: unit(),
            vco1_sh: unit(),
            vco1_auto: unit(),
            vco2_lfo: unit(),
            vco2_sh: unit(),
            vco2_auto: unit(),
            width1: unit(),
            pwm1: unit(),
            pwm1_source: pwm[pick(unit(), 3)],
            width2: unit(),
            pwm2: unit(),
            pwm2_source: pwm[pick(unit(), 3)],
            filter_env: unit(),
            env_positive: unit() < 0.5,
            filter_mod: unit(),
            mod_source: [Mod::Lfo, Mod::Sh][pick(unit(), 2)],
            key_track: unit(),
            track_source: [Track::Keyboard, Track::Pedal][pick(unit(), 2)],
            filter_audio: unit(),
            audio_source: [Audio::Vco2, Audio::Noise][pick(unit(), 2)],
            vca_env: [Env::One, Env::Two][pick(unit(), 2)],
            vca_lfo: unit(),
            bend_vco: unit(),
            bend_vco_mode: bends[pick(unit(), 3)],
            bend_filter: unit(),
            bend_filter_mode: bends[pick(unit(), 3)],
            bend_vca: unit(),
            bend_vca_mode: bends[pick(unit(), 3)],
        };
        let s = Sources {
            lfo_plus: unit(),
            lfo_centred: 2.0 * unit() - 1.0,
            lfo_triangle: unit(),
            sample_hold: 2.0 * unit() - 1.0,
            envelope_1: unit(),
            envelope_2: unit(),
            noise: 2.0 * unit() - 1.0,
            vco_2: 2.0 * unit() - 1.0,
            auto_bend: 2.0 * unit() - 1.0,
            bend: 2.0 * unit() - 1.0,
            key: 4.0 * unit() - 2.0,
            pedal: unit(),
        };

        let (r, base1, _base2) = translate(&l);
        let got = routed(&r, &s);
        let want = [
            legacy_pitch(&l, &s, l.vco1_lfo, l.vco1_sh, l.vco1_auto),
            legacy_pitch(&l, &s, l.vco2_lfo, l.vco2_sh, l.vco2_auto),
            legacy_width(l.pwm1_source, l.width1, l.pwm1, &s) - base1,
            legacy_cutoff(&l, &s),
            legacy_amplitude(&l, &s),
        ];
        let names = [
            "VCO-1 pitch",
            "VCO-2 pitch",
            "VCO-1 width",
            "cutoff",
            "gain",
        ];
        let tolerance = [1e-4, 1e-4, 1e-6, 1e-4, 1e-5];
        for i in 0..5 {
            let delta = (got[i] - want[i]).abs();
            assert!(
                delta < tolerance[i],
                "{trial}: {} {} against {} for {l:?} {s:?}",
                names[i],
                got[i],
                want[i]
            );
            worst[i] = worst[i].max(delta);
        }
    }
    println!(
        "20 000 legacy patches: worst pitch {:.2e}/{:.2e} st, width {:.2e}, cutoff {:.2e} oct, gain {:.2e}",
        worst[0], worst[1], worst[2], worst[3], worst[4]
    );
}
