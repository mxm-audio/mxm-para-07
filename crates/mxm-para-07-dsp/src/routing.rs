//! What mxm-para-07 can route, and with what.
//!
//! `plans/plan-mxm-para-07-modulation.md` §3, under `plans/plan-modulation-routing.md`. The shared
//! machinery is [`mxm_modulation`]; this module is the instrument's own declaration — its **source
//! list**, its **target list**, the **scales**, the **laws** and which routes the init patch holds.
//! Sources and targets are finite lists, not a generic patchbay.
//!
//! # The frame unit is ⅛, and every scale carries its inverse
//!
//! Four sources are not unit-bounded — Key, Noise, VCO-1 and VCO-2 — so this is
//! `mxm-mono-pr1`'s case rather than `mxm-mono-08`'s. The unit is sized by the widest value a slot
//! holds (Key, 5.58 octaves) and **⅛ is the smallest power of two that fits**: ¼ gives 1.395, and
//! [`mxm_modulation::SourceFrame::write`] clamps to unit magnitude, so an under-sized unit would not
//! error — it would quietly flatten the top of the keyboard.
//!
//! A power of two commutes with `f32` rounding, so a single-term legacy expression
//! `(depth × source) × SCALE` and the routed `(amount × source/8) × (SCALE × 8)` are the same bits.
//! That is what buys bit-identity with the pre-conversion renders in `plugins/mxm-para-07/BASELINE-C0.md`.
//!
//! # Evaluation order
//!
//! Declared source order **is** publication order, and it follows where the voice already computes
//! each value. Sources 1–15 are published before the multiplier module is evaluated; the multiplier
//! publishes as source 16; the pitch and pulse-width targets sum there, so they read 17–18 from the
//! preceding sample — **VCO audio into pitch is FM one sample late**, which is the declared cost of
//! putting the oscillators after the pitch that makes them. The oscillators' audio then publishes,
//! and cutoff and amplitude sum reading everything forward.
//!
//! **The multiplier may read itself**, one sample late like any backward route. Nothing refuses it:
//! the frame's unit bound is what keeps that loop finite, which is the property
//! [`mxm_modulation::product`] exists to preserve.
//!
//! # Presence, not depth
//!
//! An absent route contributes nothing whatever its amount holds, and presence changes only on a
//! parameter event, so [`Graph::set_topology`] compacts once per processing interval and the
//! per-sample sums run over the live lists.
//!
//! # The collection's standard
//!
//! Key, Velocity, Wheel, Pressure, Bend and the pedal (a wheel) mean what they mean on every
//! instrument, and a route the machine never had reaches what it reaches on every instrument
//! ([`mxm_modulation::standard`]; `plans/plan-modulation-standard.md`). The machine's own gain CV
//! stays its own law, named for what it is — **VCA level** — with the one pair that can only latch
//! it refused and Velocity offered on the half that closes it; **the standard Amplitude** is a new
//! target after it. The multiplier is `mxm_modulation::product_with_tops`, and the pulse widths sum
//! through `mxm_modulation::sum_split`.

use mxm_modulation::standard::{self, AMPLITUDE_SUM_BOUND, Law, Offer, Performance, reach};
use mxm_modulation::{Compacted, SourceFrame};

use crate::voice::{
    AUTO_BEND_MAX_SEMITONES, BENDER_FILTER_OCTAVES, BENDER_VCO_CV_MAX_SEMITONES,
    BENDER_VCO_LFO_MAX_SEMITONES, FILTER_AUDIO_OCTAVES, FILTER_ENV_OCTAVES, FILTER_ENV_WEIGHT,
    FILTER_MOD_OCTAVES, VCO_LFO_SEMITONES, VCO_SH_SEMITONES,
};

/// How many sources the instrument declares.
pub const SOURCES: usize = 18;
/// How many targets it declares.
pub const TARGETS: usize = 9;

/// Every source a route can read, in **publication order** (§3.1).
pub mod source {
    /// The high lane's post-portamento CV in octaves about middle C — the machine's KYBD line
    /// (`standard::key` over a twelve-semitone unit).
    pub const KEY: usize = 0;
    /// The velocity of the press that last triggered an envelope, `v − 1`
    /// (`standard::velocity`): zero at the hardest note. **A new MIDI path.**
    pub const VELOCITY: usize = 1;
    /// CC 1 on the owning channel. **New**: today the plugin accepts and discards it.
    pub const WHEEL: usize = 2;
    /// Channel pressure on the owning channel. **New**: today there is no pressure path at all.
    pub const PRESSURE: usize = 3;
    /// The bender's signed position.
    pub const BEND: usize = 4;
    /// `|bend|` — the multiplier's rectified control side, which is what the CA3080 sees (§3.4).
    pub const BEND_MAGNITUDE: usize = 5;
    /// CC 11, the pedal voltage.
    pub const PEDAL: usize = 6;
    /// The selected noise colour, as the mixer hears it.
    pub const NOISE: usize = 7;
    /// The LFO's PLUS output, which is what the VCO and VCF saw.
    pub const LFO: usize = 8;
    /// The LFO's 0-CENTER output, which is what the VCA and the bender multiplier saw.
    pub const LFO_CENTRED: usize = 9;
    /// The LFO's undelayed PWM triangle.
    pub const LFO_TRIANGLE: usize = 10;
    /// The sample and hold's lagged output.
    pub const SAMPLE_HOLD: usize = 11;
    pub const ENVELOPE_1: usize = 12;
    pub const ENVELOPE_2: usize = 13;
    /// The auto bend, already signed by its direction switch.
    pub const AUTO_BEND: usize = 14;
    /// The multiplier module's output, published like any other source (§3.4).
    pub const MULTIPLIER: usize = 15;
    /// VCO-1's decimated audio sample.
    pub const VCO_1: usize = 16;
    /// VCO-2's decimated audio sample.
    pub const VCO_2: usize = 17;
}

/// Their names, in source order, for the interface and for accessibility.
pub const SOURCE_NAMES: [&str; SOURCES] = [
    "Key",
    "Velocity",
    "Wheel",
    "Pressure",
    "Bend",
    "Bend magnitude",
    "Pedal",
    "Noise",
    "LFO",
    "LFO centred",
    "LFO triangle",
    "Sample and hold",
    "Envelope 1",
    "Envelope 2",
    "Auto bend",
    "Multiplier",
    "VCO-1",
    "VCO-2",
];

/// What a source can do to the instrument's activity when it reaches the amplifier.
///
/// **Only the amplitude target can make this instrument live or keep it tailing** (§6): pitch,
/// pulse width and cutoff all sit upstream of the VCA and cannot sustain output on their own. So
/// this classification is consulted for exactly one target, and nothing else asks routing about
/// activity at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sustain {
    /// Runs with no key and can sound unaided: the LFO's three outputs, the sample and hold, the
    /// noise and both oscillators' audio.
    FreeRunning,
    /// Sounds only while its **own value is parked away from neutral**: the keyboard's CV, the
    /// lever and its magnitude, the pedal, and the decaying auto bend.
    ///
    /// The distinction is what keeps exact silence. A lever at rest contributes nothing, so a route
    /// from it cannot hold the host awake; a lever parked off centre genuinely does sound, because
    /// the oscillators free-run and any gain above zero is audible. **The key is in this class and
    /// its neutral is middle C**, not zero: a Key route into the amplifier with the keyboard parked
    /// an octave up is a constant gain, and treating it as always-silent would cut a real sound
    /// while treating it as always-live would never sleep.
    Parked,
    /// Owns a **tail** rather than keeping the voice live: the two envelopes.
    Envelope,
    /// The multiplier, which is live exactly while its own factors are.
    Module,
}

/// Each source's class, in source order.
pub const SUSTAIN: [Sustain; SOURCES] = {
    let mut table = [Sustain::Parked; SOURCES];
    table[source::NOISE] = Sustain::FreeRunning;
    table[source::LFO] = Sustain::FreeRunning;
    table[source::LFO_CENTRED] = Sustain::FreeRunning;
    table[source::LFO_TRIANGLE] = Sustain::FreeRunning;
    table[source::SAMPLE_HOLD] = Sustain::FreeRunning;
    table[source::VCO_1] = Sustain::FreeRunning;
    table[source::VCO_2] = Sustain::FreeRunning;
    table[source::ENVELOPE_1] = Sustain::Envelope;
    table[source::ENVELOPE_2] = Sustain::Envelope;
    table[source::MULTIPLIER] = Sustain::Module;
    table
};

/// Every target a route can reach, in declared order (D5).
///
/// **Declaration order is not evaluation order**: the module is declared last and evaluated first,
/// exactly as `mxm-mono-pr1` declares its two buses last. The module header states the order the
/// voice runs.
pub mod target {
    /// VCO-1's pitch, summed in semitones on top of its lane's base.
    pub const VCO_1_PITCH: usize = 0;
    /// VCO-2's pitch, summed in semitones on top of its lane's base.
    pub const VCO_2_PITCH: usize = 1;
    /// VCO-1's pulse width — the `m` the one-sided clamp consumes, with `vco1width` as its base.
    pub const VCO_1_PULSE_WIDTH: usize = 2;
    /// VCO-2's pulse width, likewise.
    pub const VCO_2_PULSE_WIDTH: usize = 3;
    /// Filter cutoff, summed in octaves and exponentiated once.
    pub const CUTOFF: usize = 4;
    /// **VCA level**: the machine's own gain CV, summed with HOLD and clamped to `0…3` — a machine
    /// amplifier, whose law is the standard's `MachineAmplifier`. Its permanent ids stay `mod_amp_*`.
    pub const VCA_LEVEL: usize = 5;
    /// The bender's multiplier, whose law is product and whose result publishes as
    /// [`source::MULTIPLIER`].
    pub const MULTIPLIER: usize = 6;
    /// **The collection's standard Amplitude**: `standard::amplitude_factor` on the output after the
    /// VCA, silence to double. Added by the modulation standard (the owner, 2026-09-26), absent at
    /// Init. Its permanent ids are `mod_amplitude_*`.
    pub const AMPLITUDE: usize = 7;
    /// **The register bank's Level**, summed onto the `bank` knob in its own 0…1 and clamped there —
    /// the standard's control. Not on the machine (the owner, 2026-09-27: *I know it is not on the
    /// original, but the level here should be a modulation target*), so every pair takes the
    /// standard reach; absent at Init. Its permanent ids are `mod_bank_*`.
    pub const BANK_LEVEL: usize = 8;
}

/// Their names, in target order.
pub const TARGET_NAMES: [&str; TARGETS] = [
    "VCO-1 pitch",
    "VCO-2 pitch",
    "VCO-1 pulse width",
    "VCO-2 pulse width",
    "Cutoff",
    "VCA level",
    "Multiplier",
    "Amplitude",
    "Bank level",
];

/// The key source's unit, in semitones: a voice publishes the key in octaves.
pub const KEY_UNIT_SEMITONES: f32 = 12.0;

/// Which of the standard's performance sources each source is. The pedal is a wheel — a held 0…1
/// control; the lever's magnitude, the LFO outputs, the envelopes, the auto bend, the module and
/// the oscillators keep their own meaning.
pub const PERFORMANCE: [Option<Performance>; SOURCES] = {
    let mut table = [None; SOURCES];
    table[source::KEY] = Some(Performance::Key);
    table[source::VELOCITY] = Some(Performance::Velocity);
    table[source::WHEEL] = Some(Performance::Wheel);
    table[source::PRESSURE] = Some(Performance::Pressure);
    table[source::BEND] = Some(Performance::Bend);
    table[source::PEDAL] = Some(Performance::Wheel);
    table
};

/// Each target's law, for the standard's offer.
pub const LAW: [Law; TARGETS] = {
    let mut law = [Law::Sum; TARGETS];
    law[target::VCA_LEVEL] = Law::MachineAmplifier;
    law[target::MULTIPLIER] = Law::Product;
    law[target::AMPLITUDE] = Law::Factor;
    law
};

/// Whether **the machine itself** had this path, so its reach is the machine's: the panel's
/// sliders, the bender's modes, the pedal's and the audio's paths into the cutoff, the VCA's
/// envelopes, LFO, lever and multiplier, and everything into the multiplier, which is the machine's.
#[must_use]
pub const fn machine(target: usize, source: usize) -> bool {
    match target {
        target::VCO_1_PITCH | target::VCO_2_PITCH => matches!(
            source,
            source::LFO
                | source::SAMPLE_HOLD
                | source::AUTO_BEND
                | source::BEND
                | source::MULTIPLIER
        ),
        target::VCO_1_PULSE_WIDTH | target::VCO_2_PULSE_WIDTH => {
            matches!(
                source,
                source::LFO_TRIANGLE | source::ENVELOPE_1 | source::ENVELOPE_2
            )
        }
        target::CUTOFF => matches!(
            source,
            source::ENVELOPE_1
                | source::ENVELOPE_2
                | source::LFO
                | source::SAMPLE_HOLD
                | source::KEY
                | source::PEDAL
                | source::VCO_1
                | source::VCO_2
                | source::NOISE
                | source::BEND
                | source::MULTIPLIER
        ),
        target::VCA_LEVEL => matches!(
            source,
            source::ENVELOPE_1
                | source::ENVELOPE_2
                | source::LFO_CENTRED
                | source::BEND
                | source::MULTIPLIER
        ),
        target::MULTIPLIER => true,
        _ => false,
    }
}

/// Whether and how a pair is offered — `standard::offer`. **VCA level ← Key is refused**: the key
/// parked off middle C is a constant gain that latches the VCA open, and on this instrument it
/// could never mean anything else. **VCA level ← Velocity is offered on its positive half only**:
/// `v − 1` is never positive, so a route can only close the VCA for softer notes.
#[must_use]
pub const fn offer(target: usize, source: usize) -> Offer {
    standard::offer(LAW[target], PERFORMANCE[source], machine(target, source))
}

/// Each factor's **top** in the multiplier: one raw unit, and zero for the standard Velocity.
pub const PRODUCT_TOPS: [f32; SOURCES] = {
    let mut tops = [1.0; SOURCES];
    tops[source::VELOCITY] = 0.0;
    tops
};

/// The pitch the bender reaches at Init, in semitones — **the owner's ruling of 2026-09-26**: the
/// lever bends both oscillators ±2 semitones on a fresh instance, where every bender mode shipped
/// `Off` and the lever did nothing. A recorded init deviation.
pub const INIT_BEND_SEMITONES: f32 = 2.0;

/// The pitch routes the bender takes at Init, at [`INIT_BEND_SEMITONES`] of the Direct mode's reach.
pub const INIT_BEND: [(usize, usize); 2] = [
    (target::VCO_1_PITCH, source::BEND),
    (target::VCO_2_PITCH, source::BEND),
];

/// The one target whose law is **product** rather than sum — [`Graph::product`], not [`Graph::sum`].
pub const PRODUCT_TARGET: usize = target::MULTIPLIER;

/// The frame unit: every source is published as its raw value times this.
///
/// A power of two, so the division and the target scale that undoes it are both exact.
pub const FRAME_UNIT: f32 = 0.125;
/// The inverse of [`FRAME_UNIT`], which every target scale carries.
pub const FRAME_SCALE: f32 = 8.0;

/// The keyboard's own span in octaves either side of middle C, which sizes [`FRAME_UNIT`].
///
/// **Asymmetric**, because MIDI runs 0…127 against key 60: five octaves below, 67 semitones above.
/// The larger is what the unit has to hold, and 5.583 × ⅛ is 0.698.
pub const KEY_OCTAVES_BELOW: f32 = 5.0;
/// See [`KEY_OCTAVES_BELOW`].
pub const KEY_OCTAVES_ABOVE: f32 = 67.0 / 12.0;

/// What a full-amount route buys at a target whose column is uniform.
///
/// `Some(k)` takes [`Graph::sum_uniform`], which applies `FRAME_SCALE × k` **outside**
/// [`mxm_modulation::sum`] — the single multiply the legacy graph performed on an already-summed
/// bus. `mxm-mono-pr1` falsified the alternative: per-route [`mxm_modulation::sum_scaled`] over a
/// uniform column distributes a multiply over a sum and does not round alike.
///
/// `None` means two different things, and [`Graph::sum`]'s assertion is what stops them crossing:
/// the pitch and cutoff targets have genuinely non-uniform columns ([`PITCH_SCALE`],
/// [`CUTOFF_SCALE`]), while the multiplier has no scale at all because its law is product.
pub const UNIFORM_SCALE: [Option<f32>; TARGETS] = {
    let mut table = [None; TARGETS];
    // Pulse width sums into the `m` of the one-sided clamp, which is already a 0…1 control.
    table[target::VCO_1_PULSE_WIDTH] = Some(1.0);
    table[target::VCO_2_PULSE_WIDTH] = Some(1.0);
    // VCA level sums a linear gain CV, and `VCA level ← Envelope 1` at full must come out as
    // exactly `e1`: `(1 × e1/8) × (8 × 1) = e1`.
    table[target::VCA_LEVEL] = Some(1.0);
    // Amplitude and the bank's level have per-route columns ([`AMPLITUDE_SCALE`],
    // [`BANK_LEVEL_SCALE`]).
    table
};

/// What a route a **player** adds reaches, per target — **the collection's standard reach**
/// (`standard::reach`). It was the machine's own largest reach (D9), 24 semitones, until the
/// modulation standard. A route the machine itself wires keeps the scale it always had, in the
/// columns below.
pub const DECLARED_PITCH_SEMITONES: f32 = reach::PITCH_SEMITONES;
/// See [`DECLARED_PITCH_SEMITONES`].
pub const DECLARED_CUTOFF_OCTAVES: f32 = reach::OCTAVES;

/// How far one unit of a pulse width's control `m` moves the pulse, as a fraction of the cycle: the
/// voice's `0.5 − 0.4·m`, square to a tenth.
pub const PULSE_WIDTH_SWING: f32 = 0.4;

/// The **added half** of each pulse width's split sum, in frame-scaled units of `m`: **the standard
/// width** for every performance pair the machine did not have — 45 % of the cycle, 9 % per octave
/// from Key — which the machine's one-edge pulse, narrowing only to a tenth, reaches at 89 % of the
/// travel. The machine's own pairs (the LFO triangle and both envelopes) stay on `m`'s unit column.
pub const ADDED_SCALE: [[Option<f32>; SOURCES]; TARGETS] = {
    let mut table = [[None; SOURCES]; TARGETS];
    let widths = [target::VCO_1_PULSE_WIDTH, target::VCO_2_PULSE_WIDTH];
    let mut w = 0;
    while w < widths.len() {
        let t = widths[w];
        let mut s = 0;
        while s < SOURCES {
            if PERFORMANCE[s].is_some() && !machine(t, s) {
                table[t][s] = Some(FRAME_SCALE * reach::WIDTH / PULSE_WIDTH_SWING);
            }
            s += 1;
        }
        table[t][source::KEY] = Some(
            FRAME_SCALE
                * standard::key_scale(
                    reach::WIDTH * reach::KEY_LINEAR_FRACTION_PER_OCTAVE / PULSE_WIDTH_SWING,
                    KEY_UNIT_SEMITONES,
                ),
        );
        w += 1;
    }
    table
};

/// The standard Amplitude's per-route column, in frame-scaled units: **the factor's whole swing
/// with each source at its own peak** — the noise's 1.25 and the oscillators' `OUTPUT_BOUND`
/// included, which a unit column would drive into the factor's ±1 clamp at 80 % and 50 % while the
/// reading said 125 % and 200 % — and a fifth of it per octave from Key.
pub const AMPLITUDE_SCALE: [f32; SOURCES] = {
    let mut table = [FRAME_SCALE * reach::AMPLITUDE; SOURCES];
    let mut s = 0;
    while s < SOURCES {
        table[s] = FRAME_SCALE * reach::AMPLITUDE / SOURCE_PEAK[s];
        s += 1;
    }
    table[source::KEY] = FRAME_SCALE
        * standard::key_scale(
            reach::AMPLITUDE * reach::KEY_LINEAR_FRACTION_PER_OCTAVE,
            KEY_UNIT_SEMITONES,
        );
    table
};

/// The register bank's Level column, in frame-scaled units: **the standard control's whole reach —
/// the level's full 0…1 — with each source at its own peak**, as [`AMPLITUDE_SCALE`] is for the
/// factor, and a fifth of it per octave from Key.
pub const BANK_LEVEL_SCALE: [f32; SOURCES] = {
    let mut table = [FRAME_SCALE * reach::CONTROL; SOURCES];
    let mut s = 0;
    while s < SOURCES {
        table[s] = FRAME_SCALE * reach::CONTROL / SOURCE_PEAK[s];
        s += 1;
    }
    table[source::KEY] = FRAME_SCALE
        * standard::key_scale(
            reach::CONTROL * reach::KEY_LINEAR_FRACTION_PER_OCTAVE,
            KEY_UNIT_SEMITONES,
        );
    table
};

/// The pitch targets' per-route scale column, in semitones times [`FRAME_SCALE`].
///
/// Both VCO pitch targets share it: the hardware's two slider sets are the same three reaches, and
/// the bender's one depth fans out to both lanes (§5). Every entry that is not one of the machine's
/// own wired reaches takes the standard's [`DECLARED_PITCH_SEMITONES`] — Key its 12 st/oct.
///
/// **The bender occupies two entries with different reaches**, because its modes were two different
/// spans in the service spec: Direct is [`source::BEND`] at 15 semitones, and LFO mode is
/// [`source::MULTIPLIER`] at 10 — which is exactly what `voice.rs`'s `vco_bend_term` encodes, now
/// held as two pairs with their own scale columns rather than one term with a mode switch.
pub const PITCH_SCALE: [f32; SOURCES] = {
    let mut table = [FRAME_SCALE * DECLARED_PITCH_SEMITONES; SOURCES];
    // The key's per-octave standard: twelve semitones per octave, and the key is in octaves.
    table[source::KEY] = FRAME_SCALE
        * standard::key_scale(reach::KEY_PITCH_SEMITONES_PER_OCTAVE, KEY_UNIT_SEMITONES);
    table[source::LFO] = FRAME_SCALE * VCO_LFO_SEMITONES;
    table[source::SAMPLE_HOLD] = FRAME_SCALE * VCO_SH_SEMITONES;
    table[source::AUTO_BEND] = FRAME_SCALE * AUTO_BEND_MAX_SEMITONES;
    table[source::BEND] = FRAME_SCALE * BENDER_VCO_CV_MAX_SEMITONES;
    table[source::MULTIPLIER] = FRAME_SCALE * BENDER_VCO_LFO_MAX_SEMITONES;
    table
};

/// Cutoff's per-route scale column, in octaves times [`FRAME_SCALE`].
///
/// Four entries differ from the standard's added reach ([`DECLARED_CUTOFF_OCTAVES`]), and all four
/// are arithmetic rather than taste:
///
/// - **Envelope 1 carries `FILTER_ENV_OCTAVES × FILTER_ENV_WEIGHT`**, the firm 120 k / 47 k input
///   weight the cutoff summing node applies to ENV-1 (`voice.rs:1241-1245`). Folding the two
///   constants into one scale is the one place §3.3 says bit-identity is *not* expected, because a
///   product of two constants rounds differently from multiplying them in turn.
/// - **Key carries one octave of cutoff per octave of keyboard**, which is what keyboard tracking
///   is, and what the retired `keytrack` meant in Keyboard mode.
/// - **The pedal carries one octave** as well: Pedal mode is the manual's fixed 1 V/oct key
///   tracking *plus* the pedal at the amount, so the two arrive as separate pairs (§5).
/// - **The audio sources carry `FILTER_AUDIO_OCTAVES`**, half the modulation reach, as the legacy
///   audio path did.
pub const CUTOFF_SCALE: [f32; SOURCES] = {
    let mut table = [FRAME_SCALE * DECLARED_CUTOFF_OCTAVES; SOURCES];
    table[source::ENVELOPE_1] = FRAME_SCALE * FILTER_ENV_OCTAVES * FILTER_ENV_WEIGHT;
    table[source::LFO] = FRAME_SCALE * FILTER_MOD_OCTAVES;
    table[source::SAMPLE_HOLD] = FRAME_SCALE * FILTER_MOD_OCTAVES;
    table[source::KEY] = FRAME_SCALE;
    table[source::PEDAL] = FRAME_SCALE;
    table[source::VCO_1] = FRAME_SCALE * FILTER_AUDIO_OCTAVES;
    table[source::VCO_2] = FRAME_SCALE * FILTER_AUDIO_OCTAVES;
    table[source::NOISE] = FRAME_SCALE * FILTER_AUDIO_OCTAVES;
    table[source::BEND] = FRAME_SCALE * BENDER_FILTER_OCTAVES;
    table[source::MULTIPLIER] = FRAME_SCALE * BENDER_FILTER_OCTAVES;
    table
};

/// The generous per-target bound. **It exists to keep a runaway finite, not to shape a sound**, and
/// every target applies its real limit downstream: `midi_hz` clamps the note, the voice clamps
/// cutoff octaves and the gain CV, the oscillator clamps width.
///
/// **It is not in one domain for every target, which is the trap.** A uniform column clamps in
/// frame units, before [`Graph::sum_uniform`]'s outside multiply; a per-route column clamps *after*
/// its scales, so for the pitch targets this number is **semitones**. The machine's own wiring
/// already reaches 12 + 24 + 24 + 15 = 75 of them, so the 64 this started at would have clipped
/// patches the panel makes today — silently, because a clamp does not complain.
/// [`tests::the_bound_clears_every_legacy_reachable_pitch_sum`] is what holds it.
pub const SUM_BOUND: f32 = 512.0;

/// The routes **the machine itself wires**, present in the init patch **at zero depth** (§4).
///
/// These are the panel's own sliders at their Init switch positions: each VCO's LFO depth, the two
/// pulse-width depths whose amounts `vco1pwm`/`vco2pwm` claim, four of the five VCF sliders each at
/// its Init selector — the fifth, the follower's, retired with the external input — and the VCA's
/// LFO slider. Every amount here is zero, so a revealed route is audible as
/// nothing and Init sounds exactly as it always did.
///
/// **Each VCO's Sample and hold and Auto bend depths are not here** (the owner, 2026-09-27: *too
/// many sources on the pitch by default; just leave LFO*): a recorded Init deviation beside
/// [`INIT_BEND`], so a fresh Pitch stack shows LFO and Bend. Both pairs stay offered.
pub const INIT_PRESENT: [(usize, usize); 9] = [
    (target::VCO_1_PITCH, source::LFO),
    (target::VCO_2_PITCH, source::LFO),
    (target::VCO_1_PULSE_WIDTH, source::LFO_TRIANGLE),
    (target::VCO_2_PULSE_WIDTH, source::LFO_TRIANGLE),
    (target::CUTOFF, source::ENVELOPE_1),
    (target::CUTOFF, source::LFO),
    (target::CUTOFF, source::KEY),
    (target::CUTOFF, source::VCO_2),
    (target::VCA_LEVEL, source::LFO_CENTRED),
];

/// The routes wired **at full amount**, which are this instrument's three init deviations (§4).
///
/// `VCA level ← Envelope 1` is the VCA envelope switch: a connection with no attenuator, and the
/// brief's *"ENV-1 opens the VCA"*. The multiplier's two factors are present at full for the reason
/// `product` gives — a factor present at zero contributes a constant one, so the module would
/// publish the same thing whatever the bender did and the route's presence would be a lie.
///
/// The multiplier's pair changes no Init sound, because nothing routes the module at Init: all three
/// bender mode switches ship `Off`, which is absence.
pub const INIT_AT_FULL: [(usize, usize); 3] = [
    (target::VCA_LEVEL, source::ENVELOPE_1),
    (target::MULTIPLIER, source::BEND_MAGNITUDE),
    (target::MULTIPLIER, source::LFO_CENTRED),
];

/// A route's amount in the init patch: one for [`INIT_AT_FULL`], the bender's two semitones of its
/// Direct reach for [`INIT_BEND`], zero for everything else.
#[must_use]
pub const fn init_amount(target: usize, source: usize) -> f32 {
    let mut i = 0;
    while i < INIT_AT_FULL.len() {
        if INIT_AT_FULL[i].0 == target && INIT_AT_FULL[i].1 == source {
            return 1.0;
        }
        i += 1;
    }
    let mut j = 0;
    while j < INIT_BEND.len() {
        if INIT_BEND[j].0 == target && INIT_BEND[j].1 == source {
            return INIT_BEND_SEMITONES / crate::voice::BENDER_VCO_CV_MAX_SEMITONES;
        }
        j += 1;
    }
    0.0
}

/// Whether the init patch wires this pair at all — [`INIT_PRESENT`], [`INIT_AT_FULL`] or
/// [`INIT_BEND`].
#[must_use]
pub const fn init_present(target: usize, source: usize) -> bool {
    let mut i = 0;
    while i < INIT_PRESENT.len() {
        if INIT_PRESENT[i].0 == target && INIT_PRESENT[i].1 == source {
            return true;
        }
        i += 1;
    }
    init_amount(target, source) != 0.0
}

/// Each source's peak magnitude, from §3.1's raw bounds.
///
/// **What a reading is measured against**: a route's amount reads as what its pair delivers with its
/// source at full swing, so a source that never exceeds a half would otherwise read twice what it can
/// actually do. Four sources are not unit-bounded, which is what sized [`FRAME_UNIT`] in the first
/// place — and every one of them still fits the frame's ±1 after the ⅛: the widest, Key, lands at
/// 0.698.
pub const SOURCE_PEAK: [f32; SOURCES] = {
    let mut table = [1.0; SOURCES];
    // Asymmetric, and the larger side is what a reading has to survive: 67 semitones above key 60.
    table[source::KEY] = KEY_OCTAVES_ABOVE;
    // The pink weights sum past unity; the mixer hears the selected colour at this bound.
    table[source::NOISE] = 1.25;
    table[source::VCO_1] = crate::oscillator::OUTPUT_BOUND;
    table[source::VCO_2] = crate::oscillator::OUTPUT_BOUND;
    table
};

/// What a **full-amount** route delivers per unit of source, in the target's own unit.
///
/// Derived rather than declared a second time: the pitch and cutoff columns are the per-route scales
/// with [`FRAME_SCALE`] divided back out, and the two pulse-width targets, the amplifier and the
/// multiplier are all unit columns. Declaring these numbers again by hand is how a reading comes to
/// say one thing while the sum does another.
pub const FULL_SCALE: [[f32; SOURCES]; TARGETS] = {
    let mut table = [[1.0; SOURCES]; TARGETS];
    let mut s = 0;
    while s < SOURCES {
        let pitch = PITCH_SCALE[s] / FRAME_SCALE;
        table[target::VCO_1_PITCH][s] = pitch;
        table[target::VCO_2_PITCH][s] = pitch;
        table[target::CUTOFF][s] = CUTOFF_SCALE[s] / FRAME_SCALE;
        s += 1;
    }
    table
};

/// What a route delivers at this amount with its source at its peak, in the target's own unit —
/// what the interface reads, so the number a player sees is the number the pair moves.
#[inline]
#[must_use]
pub fn reach(target: usize, source: usize, amount: f32) -> f32 {
    amount * FULL_SCALE[target][source] * SOURCE_PEAK[source]
}

/// Which sources are live into which targets, and how much of each.
///
/// **Presence is what the DSP reads.** An absent route contributes nothing whatever its amount
/// holds, which is what makes removing a source one parameter write and re-adding it restore the
/// depth the player last set.
///
/// **It travels beside the patch, never inside it**: `voice::Patch` is rebuilt every sample and is
/// `Clone`, so a 162-float grid carried there would be a memcpy per sample for values that change
/// only on a parameter event.
/// **Deliberately not `PartialEq`.** Two gates' worth of wiring can be identical while their
/// compacted lists differ in length only because one has not been compacted yet, so a structural
/// comparison answers a question nobody meant to ask. Compare `present` when the question is "did
/// the topology change".
#[derive(Debug, Clone, Copy)]
pub struct Routing {
    /// Per target, per source: whether that route exists.
    pub present: [[bool; SOURCES]; TARGETS],
    /// Per target, per source: how much, signed, as a **fraction of that route's scale**.
    ///
    /// The scale is applied by [`Graph::sum`] rather than folded in here, because
    /// `(amount × source) × scale` is the instruction sequence this voice executed before it had
    /// routing and the other order does not round the same way.
    pub amounts: [[f32; SOURCES]; TARGETS],
    live: [(u8, u8); TARGETS * SOURCES],
    live_len: usize,
}

impl Default for Routing {
    fn default() -> Self {
        Self::new()
    }
}

impl Routing {
    /// Nothing routed anywhere.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            present: [[false; SOURCES]; TARGETS],
            amounts: [[0.0; SOURCES]; TARGETS],
            live: [(0, 0); TARGETS * SOURCES],
            live_len: 0,
        }
    }

    /// Rebuilds the live list from `present`, in declared order.
    ///
    /// Once per processing interval, never per sample: topology is discrete and changes only on a
    /// parameter event.
    pub fn compact(&mut self) {
        self.live_len = 0;
        for (t, target) in self.present.iter().enumerate() {
            for (s, &on) in target.iter().enumerate() {
                if on {
                    self.live[self.live_len] = (t as u8, s as u8);
                    self.live_len += 1;
                }
            }
        }
    }

    /// The live pairs, `(target, source)`.
    #[inline]
    #[must_use]
    pub fn live(&self) -> &[(u8, u8)] {
        &self.live[..self.live_len]
    }

    /// The machine's own wiring: [`INIT_PRESENT`] at zero depth and [`INIT_AT_FULL`] at one.
    #[must_use]
    pub const fn init() -> Self {
        let mut routing = Self::new();
        let mut i = 0;
        while i < INIT_PRESENT.len() {
            let (t, s) = INIT_PRESENT[i];
            routing.present[t][s] = true;
            i += 1;
        }
        let mut j = 0;
        while j < INIT_AT_FULL.len() {
            let (t, s) = INIT_AT_FULL[j];
            routing.present[t][s] = true;
            routing.amounts[t][s] = 1.0;
            j += 1;
        }
        let mut k = 0;
        while k < INIT_BEND.len() {
            let (t, s) = INIT_BEND[k];
            routing.present[t][s] = true;
            routing.amounts[t][s] = init_amount(t, s);
            k += 1;
        }
        // In declared order, as `compact` builds it — written out because `compact` is not const.
        let mut len = 0;
        let mut t = 0;
        while t < TARGETS {
            let mut s = 0;
            while s < SOURCES {
                if routing.present[t][s] {
                    routing.live[len] = (t as u8, s as u8);
                    len += 1;
                }
                s += 1;
            }
            t += 1;
        }
        routing.live_len = len;
        routing
    }
}

/// The voice's routing state: one frame, and one compacted live list per target.
#[derive(Debug, Clone)]
pub struct Graph {
    frame: SourceFrame<SOURCES>,
    live: [Compacted<SOURCES>; TARGETS],
    /// Each uniform target's two halves for `mxm_modulation::sum_split`.
    uniform: [Compacted<SOURCES>; TARGETS],
    added: [Compacted<SOURCES>; TARGETS],
    /// Which sources any live route actually reads, cached at [`Graph::set_topology`].
    ///
    /// **A source nothing reads is not published.** Publishing all eighteen would cost a finite
    /// check, a clamp and two array stores each for values no sum would look at, and this voice
    /// computes several of them only because something downstream might want them.
    needed: [bool; SOURCES],
    /// Whether any route at all is live, cached at [`Graph::set_topology`].
    any: bool,
}

impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}

impl Graph {
    /// An empty graph.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            frame: SourceFrame::new(),
            live: [const { Compacted::new() }; TARGETS],
            uniform: [const { Compacted::new() }; TARGETS],
            added: [const { Compacted::new() }; TARGETS],
            needed: [false; SOURCES],
            any: false,
        }
    }

    /// Rebuilds which routes are live. **Once per processing interval, never per sample.**
    ///
    /// **Every path that can reach the render loop owes this.** The sampler's conversion shipped a
    /// voice that was never armed with the topology, so the first note after any allocation
    /// rendered with nothing routed.
    ///
    /// **A source that becomes needed starts from silence.** While nothing read it, nothing
    /// published it, so its slot still holds whatever it held the last time something did — which
    /// may be from a different phrase. Clearing makes the first sample a deterministic zero instead.
    pub fn set_topology(&mut self, routing: &Routing) {
        for (target, present) in routing.present.iter().enumerate() {
            self.live[target].build(present);
            let mut uniform = [false; SOURCES];
            let mut added = [false; SOURCES];
            for (source, &on) in present.iter().enumerate() {
                let split = ADDED_SCALE[target][source].is_some();
                uniform[source] = on && !split;
                added[source] = on && split;
            }
            self.uniform[target].build(&uniform);
            self.added[target].build(&added);
        }
        self.any = self.live.iter().any(|l| !l.is_empty());
        let was_needed = self.needed;
        self.needed = [false; SOURCES];
        for present in routing.present.iter() {
            for (needed, &on) in self.needed.iter_mut().zip(present.iter()) {
                *needed |= on;
            }
        }
        for (source, (&needed, &before)) in self.needed.iter().zip(was_needed.iter()).enumerate() {
            if needed && !before {
                self.frame.clear(source);
            }
        }
    }

    /// Whether anything is routed at all, which is what lets the voice skip the work.
    #[inline]
    #[must_use]
    pub fn any_live(&self) -> bool {
        self.any
    }

    /// Whether any route is live into that target.
    #[inline]
    #[must_use]
    pub fn is_empty(&self, target: usize) -> bool {
        self.live[target].is_empty()
    }

    /// Whether any live route reads that source, which is what `write` gates on.
    #[inline]
    #[must_use]
    pub fn needs(&self, source: usize) -> bool {
        self.needed[source]
    }

    /// Opens a sample.
    #[inline]
    pub fn begin_sample(&mut self) {
        self.frame.begin_sample();
    }

    /// Publishes a source's **raw** value for this sample, scaled into the frame unit.
    ///
    /// The scaling is here rather than at every call site so no caller can forget it, and it is
    /// exact because [`FRAME_UNIT`] is a power of two.
    #[inline]
    pub fn write(&mut self, source: usize, raw: f32) {
        if self.needed[source] {
            self.frame.write(source, raw * FRAME_UNIT);
        }
    }

    /// Publishes a value that is **already in frame units** — the multiplier, which composes from
    /// sources that are themselves scaled.
    #[inline]
    pub fn write_in_frame_units(&mut self, source: usize, value: f32) {
        if self.needed[source] {
            self.frame.write(source, value);
        }
    }

    /// Reads a source as the frame holds it, in frame units.
    #[inline]
    #[must_use]
    pub fn read(&self, source: usize) -> f32 {
        self.frame.read(source)
    }

    /// This target's summed modulation, in its own domain.
    ///
    /// **Not for [`PRODUCT_TARGET`]**, whose `None` in [`UNIFORM_SCALE`] means *no scale at all*
    /// rather than *a per-route column*. Falling through would sum the multiplier's factors against
    /// octave scales; the assertion is what stops the two meanings crossing.
    #[inline]
    #[must_use]
    pub fn sum(&self, target: usize, routing: &Routing) -> f32 {
        debug_assert_ne!(
            target, PRODUCT_TARGET,
            "the multiplier's law is product, not sum"
        );
        match UNIFORM_SCALE[target] {
            Some(scale) => self.sum_uniform(target, routing, scale),
            None => mxm_modulation::sum_scaled(
                &self.frame,
                &self.live[target],
                &routing.amounts[target],
                self.column(target),
                if target == target::AMPLITUDE {
                    AMPLITUDE_SUM_BOUND
                } else {
                    SUM_BOUND
                },
            ),
        }
    }

    /// Which per-route scale column a non-uniform target uses.
    #[inline]
    #[must_use]
    const fn column(&self, target: usize) -> &'static [f32; SOURCES] {
        match target {
            target::CUTOFF => &CUTOFF_SCALE,
            target::AMPLITUDE => &AMPLITUDE_SCALE,
            target::BANK_LEVEL => &BANK_LEVEL_SCALE,
            _ => &PITCH_SCALE,
        }
    }

    /// A uniform-column target's sum: every route at scale one, the target scale applied last.
    ///
    /// `FRAME_SCALE * scale` is a compile-time product of a power of two with the target scale, so
    /// it is itself exact, and one multiply against it undoes the frame unit and applies the target
    /// scale together — which is the single multiply the legacy voice performed on its already
    /// summed bus. `mxm-mono-pr1` falsified the per-route alternative in its last bits.
    #[inline]
    #[must_use]
    pub fn sum_uniform(&self, target: usize, routing: &Routing, scale: f32) -> f32 {
        // The machine's pairs on its own scale, the added ones on theirs; while no added pair is
        // live this is the legacy bus's instruction sequence to the bit.
        let mut added_scales = [0.0; SOURCES];
        for (slot, scale) in added_scales.iter_mut().zip(ADDED_SCALE[target]) {
            *slot = scale.unwrap_or(0.0);
        }
        mxm_modulation::sum_split(
            &self.frame,
            &self.uniform[target],
            &routing.amounts[target],
            FRAME_SCALE * scale,
            &self.added[target],
            &added_scales,
            SUM_BOUND,
        )
    }

    /// The **multiplier module's** value, in raw domain — D4's law, in a scaled frame.
    ///
    /// Each factor is un-scaled back out of [`FRAME_UNIT`] before the law is applied, so the `1` in
    /// `1 + amount × (source − 1)` means *one raw unit* and every property the law is supposed to
    /// have survives: nothing present is neutral, a zero amount is neutral rather than
    /// annihilating, and two routes at full amount give the plain product of their sources.
    ///
    /// **The result is in raw domain**, because each factor was un-scaled before the law saw it, so
    /// the caller publishes it with [`Graph::write`] — which applies the frame unit like any other
    /// source. Publishing it with [`Graph::write_in_frame_units`] would put it into the frame eight
    /// times too large. The frame's own bound is then what keeps a cycle through it finite.
    #[inline]
    #[must_use]
    pub fn product(&self, routing: &Routing) -> f32 {
        mxm_modulation::product_with_tops(
            &self.frame,
            &self.live[PRODUCT_TARGET],
            &routing.amounts[PRODUCT_TARGET],
            &PRODUCT_TOPS,
            FRAME_SCALE,
        )
    }

    /// Clears the frame, leaving no tail between renders.
    ///
    /// Both halves: `previous` is state, so a reset clearing only `current` would let one render
    /// leak a sample into the next.
    pub fn reset(&mut self) {
        self.frame.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The unit has to hold the widest raw value any source publishes**, because
    /// `SourceFrame::write` clamps rather than complains: an under-sized unit would flatten the top
    /// of the keyboard silently. §3.3's arithmetic, as a test.
    #[test]
    fn the_frame_unit_holds_every_declared_raw_bound() {
        let widest: [(usize, f32); 4] = [
            (source::KEY, KEY_OCTAVES_ABOVE.max(KEY_OCTAVES_BELOW)),
            (source::NOISE, 1.25),
            (source::VCO_1, 2.0),
            (source::VCO_2, 2.0),
        ];
        for (source, raw) in widest {
            let published = raw * FRAME_UNIT;
            assert!(
                published <= 1.0,
                "{} publishes {published} and would be clamped",
                SOURCE_NAMES[source]
            );
        }
        // And a quarter would not hold them, which is the whole reason the unit is an eighth. Said
        // against the same table rather than as a folded constant, so it keeps meaning something if
        // a source's raw bound ever changes.
        assert!(
            widest.iter().any(|&(source, raw)| {
                let _ = source;
                raw * 0.25 > 1.0
            }),
            "a quarter unit would hold every source, so the eighth needs a different reason"
        );
    }

    /// **The bound must not clip what the panel can already reach.** For a per-route column the
    /// clamp lands after the scales, so on the pitch targets it is measured in semitones — a
    /// number chosen for a frame-unit target would quietly flatten the deepest sweeps.
    /// **What a full route delivers, in the target's own unit, against literal numbers.**
    ///
    /// The expected side is written out rather than built from the same constants the columns are,
    /// because a test that recomputes the table from the table's own ingredients agrees with any
    /// wiring, right or wrong. These are the machine's own reaches: the manual's twelve semitones of
    /// LFO pitch, twenty-four of sample and hold and of auto bend, the bender's fifteen in Direct
    /// and ten through the multiplier, and the cutoff's 120 k / 47 k envelope weight.
    #[test]
    fn a_full_route_reaches_what_the_machine_reached() {
        let cases: [(usize, usize, f32); 17] = [
            (target::VCO_1_PITCH, source::LFO, 12.0),
            (target::VCO_1_PITCH, source::SAMPLE_HOLD, 24.0),
            (target::VCO_2_PITCH, source::AUTO_BEND, 24.0),
            (target::VCO_1_PITCH, source::BEND, 15.0),
            (target::VCO_1_PITCH, source::MULTIPLIER, 10.0),
            // A source the machine never wired takes the collection's standard octave; it took
            // D9's 24 semitones until the modulation standard.
            (target::VCO_1_PITCH, source::VELOCITY, 12.0),
            (target::CUTOFF, source::ENVELOPE_1, 4.0 * 120.0 / 47.0),
            (target::CUTOFF, source::LFO, 4.0),
            (target::CUTOFF, source::SAMPLE_HOLD, 4.0),
            (target::CUTOFF, source::PEDAL, 1.0),
            (target::CUTOFF, source::ENVELOPE_2, 4.0),
            // The audio sources carry half the modulation reach, against their own peaks: both VCOs
            // swing to two and the noise to 1.25.
            (target::CUTOFF, source::VCO_2, 4.0),
            (target::CUTOFF, source::NOISE, 2.5),
            (target::CUTOFF, source::BEND, 2.0),
            (target::CUTOFF, source::MULTIPLIER, 2.0),
            // The amplifier has no attenuator: a full envelope opens it exactly.
            (target::VCA_LEVEL, source::ENVELOPE_1, 1.0),
            (target::VCO_1_PULSE_WIDTH, source::LFO_TRIANGLE, 1.0),
        ];
        for (target, source, expected) in cases {
            let delivered = reach(target, source, 1.0);
            assert!(
                (delivered - expected).abs() < 1e-4,
                "{} from {} reaches {delivered}, not {expected}",
                TARGET_NAMES[target],
                SOURCE_NAMES[source]
            );
        }
    }

    /// **A Key route reads per octave, never at the keyboard's peak.**
    ///
    /// Five and a half octaves times a declared reach is a number no player can act on, so the
    /// interface reads these per octave — and that only works because the column is *per octave of
    /// keyboard* to begin with: one octave of cutoff per octave of key is what tracking is.
    #[test]
    fn the_key_column_is_per_octave_of_keyboard() {
        assert!((FULL_SCALE[target::CUTOFF][source::KEY] - 1.0).abs() < 1e-6);
        // The standard's full tracking: twelve semitones per octave, so −100 % holds one pitch.
        assert!((FULL_SCALE[target::VCO_1_PITCH][source::KEY] - 12.0).abs() < 1e-6);
        assert!(
            reach(target::CUTOFF, source::KEY, 1.0) > 5.0,
            "at peak this reads five and a half octaves, which is why the interface does not"
        );
    }

    /// [`SOURCE_PEAK`] is the one place a source's swing is written down.
    #[test]
    fn the_peaks_are_the_raw_bounds_the_frame_unit_was_sized_for() {
        assert!((SOURCE_PEAK[source::KEY] - KEY_OCTAVES_ABOVE).abs() < 1e-6);
        assert!((SOURCE_PEAK[source::NOISE] - 1.25).abs() < 1e-6);
        assert!((SOURCE_PEAK[source::VCO_1] - 2.0).abs() < 1e-6);
        assert!((SOURCE_PEAK[source::VCO_2] - 2.0).abs() < 1e-6);
        for (source, &peak) in SOURCE_PEAK.iter().enumerate() {
            assert!(
                peak * FRAME_UNIT <= 1.0,
                "{} peaks at {peak} and would publish clamped",
                SOURCE_NAMES[source]
            );
        }
    }

    /// `init`'s live list is hand-rolled because `compact` cannot be const. The two must agree.
    #[test]
    fn the_const_init_compaction_is_what_compact_builds() {
        let declared = Routing::init();
        let mut rebuilt = Routing::init();
        rebuilt.compact();
        assert_eq!(declared.live(), rebuilt.live());
        assert_eq!(
            declared.live().len(),
            INIT_PRESENT.len() + INIT_AT_FULL.len() + INIT_BEND.len(),
            "every wired pair is live and nothing else is"
        );
        let mut empty = Routing::new();
        empty.compact();
        assert!(empty.live().is_empty());
    }

    #[test]
    fn the_bound_clears_every_legacy_reachable_pitch_sum() {
        let machine = VCO_LFO_SEMITONES
            + VCO_SH_SEMITONES
            + AUTO_BEND_MAX_SEMITONES
            + BENDER_VCO_CV_MAX_SEMITONES;
        assert!(
            machine < SUM_BOUND,
            "the machine's own pitch wiring reaches {machine} semitones"
        );
        // Every source at full amount, which is what a player can build.
        let widest: f32 = PITCH_SCALE.iter().map(|scale| scale * FRAME_UNIT).sum();
        assert!(
            widest <= SUM_BOUND,
            "all eighteen pitch routes at full reach {widest} semitones"
        );
    }

    #[test]
    fn the_unit_and_its_inverse_are_exact() {
        assert_eq!(FRAME_UNIT * FRAME_SCALE, 1.0);
        for raw in [0.1f32, 0.7, 1.0, 2.5, 5.58] {
            assert_eq!(raw * FRAME_UNIT * FRAME_SCALE, raw);
        }
    }

    /// **The conversion's arithmetic claim, at this layer.** A single route at full amount is the
    /// legacy expression bit for bit, because the unit and its inverse are powers of two.
    #[test]
    fn one_route_at_full_amount_is_the_legacy_product() {
        let mut routing = Routing::new();
        routing.present[target::VCO_1_PITCH][source::LFO] = true;
        routing.amounts[target::VCO_1_PITCH][source::LFO] = 1.0;
        let mut graph = Graph::new();
        graph.set_topology(&routing);
        graph.begin_sample();
        for raw in [-1.0f32, -0.15, 0.0, 0.37, 1.0] {
            graph.begin_sample();
            graph.write(source::LFO, raw);
            let legacy = 1.0 * raw * VCO_LFO_SEMITONES;
            assert_eq!(
                graph.sum(target::VCO_1_PITCH, &routing),
                legacy,
                "the routed pitch term lost the legacy bits at {raw}"
            );
        }
    }

    /// Amplitude's own claim, which the host golden rests on: `(1 × e1/8) × 8 = e1`.
    #[test]
    fn the_amplitude_envelope_at_full_is_exactly_the_envelope() {
        let routing = Routing::init();
        let mut graph = Graph::new();
        graph.set_topology(&routing);
        for e1 in [0.0f32, 0.125, 0.3, 0.7, 1.0] {
            graph.begin_sample();
            graph.write(source::ENVELOPE_1, e1);
            assert_eq!(graph.sum(target::VCA_LEVEL, &routing), e1);
        }
    }

    /// Init wires exactly §4's table, and every wired amount but the documented deviations is zero:
    /// the three at full, and the bender's two semitones into both pitches (the owner, 2026-09-26).
    #[test]
    fn init_wires_the_panel_and_three_documented_deviations() {
        let routing = Routing::init();
        let mut present = 0;
        for (target, row) in routing.present.iter().enumerate() {
            for (source, &on) in row.iter().enumerate() {
                if on {
                    present += 1;
                    assert_eq!(
                        routing.amounts[target][source],
                        init_amount(target, source),
                        "{} from {} has an undocumented init amount",
                        TARGET_NAMES[target],
                        SOURCE_NAMES[source]
                    );
                }
            }
        }
        assert_eq!(
            present,
            INIT_PRESENT.len() + INIT_AT_FULL.len() + INIT_BEND.len()
        );
        assert_eq!(INIT_AT_FULL.len(), 3);
    }

    /// **`product`'s two properties**, which are why the multiplier's factors are wired at full.
    #[test]
    fn the_multiplier_is_neutral_empty_and_neutral_at_zero_amount() {
        let mut graph = Graph::new();
        let empty = Routing::new();
        graph.set_topology(&empty);
        graph.begin_sample();
        assert_eq!(
            graph.product(&empty),
            1.0,
            "an unwired multiplier is neutral"
        );

        let mut zero = Routing::new();
        zero.present[target::MULTIPLIER][source::LFO_CENTRED] = true;
        graph.set_topology(&zero);
        graph.begin_sample();
        graph.write(source::LFO_CENTRED, 0.8);
        assert_eq!(
            graph.product(&zero),
            1.0,
            "a zero amount must be neutral, not annihilating"
        );

        let mut full = zero;
        full.amounts[target::MULTIPLIER][source::LFO_CENTRED] = 1.0;
        graph.set_topology(&full);
        graph.begin_sample();
        graph.write(source::LFO_CENTRED, 0.8);
        assert!((graph.product(&full) - 0.8).abs() < 1e-6);
    }

    /// An absent route contributes nothing whatever its amount holds.
    #[test]
    fn an_absent_route_contributes_nothing() {
        let mut routing = Routing::new();
        routing.amounts[target::CUTOFF][source::LFO] = 1.0;
        let mut graph = Graph::new();
        graph.set_topology(&routing);
        graph.begin_sample();
        graph.write(source::LFO, 1.0);
        assert_eq!(graph.sum(target::CUTOFF, &routing), 0.0);

        routing.present[target::CUTOFF][source::LFO] = true;
        graph.set_topology(&routing);
        graph.begin_sample();
        graph.write(source::LFO, 1.0);
        assert_eq!(
            graph.sum(target::CUTOFF, &routing),
            FILTER_MOD_OCTAVES,
            "a present route at full amount is the legacy octave reach"
        );
    }

    /// A source nothing reads is never published, so publishing is not a cost the patch does not ask
    /// for — and a newly read source starts from a deterministic zero rather than a stale phrase.
    #[test]
    fn an_unread_source_is_not_published_and_a_newly_read_one_starts_clear() {
        let mut routing = Routing::new();
        let mut graph = Graph::new();
        graph.set_topology(&routing);
        graph.begin_sample();
        graph.write(source::LFO, 1.0);
        assert_eq!(
            graph.read(source::LFO),
            0.0,
            "nothing reads it, so nothing published it"
        );

        routing.present[target::CUTOFF][source::LFO] = true;
        graph.set_topology(&routing);
        graph.begin_sample();
        assert_eq!(
            graph.read(source::LFO),
            0.0,
            "a newly read source starts from silence, not from an old phrase"
        );
    }

    #[test]
    fn the_declared_lists_are_the_declared_lengths() {
        assert_eq!(SOURCE_NAMES.len(), SOURCES);
        assert_eq!(TARGET_NAMES.len(), TARGETS);
        assert_eq!(source::VCO_2, SOURCES - 1);
        assert_eq!(target::BANK_LEVEL, TARGETS - 1);
        assert_eq!(PRODUCT_TARGET, target::MULTIPLIER);
    }
}
