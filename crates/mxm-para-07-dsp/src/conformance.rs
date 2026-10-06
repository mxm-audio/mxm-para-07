//! mxm-para-07's routing as the collection's modulation standard checks it
//! (`mxm_modulation::conformance`; `plans/plan-modulation-standard.md`).
//!
//! Behind the `conformance` feature, which only `[dev-dependencies]` enable — this crate's own
//! tests, and the plugin's, whose route readings are held to [`Declared::deliver`] — so no shipped
//! graph carries it. [`Declared`] answers every question through [`crate::routing`]'s own tables
//! and a real [`Graph`], its frame unit included, never a copy of them.

use mxm_modulation::conformance::{Declaration, Kind};
use mxm_modulation::standard::{self, Law, Offer, Performance};

use crate::routing::{
    self, Graph, KEY_UNIT_SEMITONES, PRODUCT_TARGET, Routing, SOURCE_NAMES, SOURCES, TARGET_NAMES,
    TARGETS, target,
};

/// What each target is, for the standard. A pulse width is the standard's width, delivered as a
/// fraction of the cycle (`PULSE_WIDTH_SWING` of it per unit of `m`); VCA level is the machine's
/// amplifier, the multiplier is its own module, and the bank's Level is the standard's control.
const KINDS: [Kind; TARGETS] = {
    let mut kinds = [Kind::Pitch; TARGETS];
    kinds[target::VCO_1_PULSE_WIDTH] = Kind::Width;
    kinds[target::VCO_2_PULSE_WIDTH] = Kind::Width;
    kinds[target::CUTOFF] = Kind::Cutoff;
    kinds[target::VCA_LEVEL] = Kind::Machine(Law::MachineAmplifier);
    kinds[target::MULTIPLIER] = Kind::Machine(Law::Product);
    kinds[target::AMPLITUDE] = Kind::Amplitude;
    kinds[target::BANK_LEVEL] = Kind::Control;
    kinds
};

/// mxm-para-07's routing declaration.
#[derive(Debug, Clone, Copy, Default)]
pub struct Declared;

/// Exactly one route, at `amount`.
fn one_route(target: usize, source: usize, amount: f32) -> Routing {
    let mut routing = Routing::new();
    routing.present[target][source] = true;
    routing.amounts[target][source] = amount;
    routing.compact();
    routing
}

impl Declaration for Declared {
    fn sources(&self) -> usize {
        SOURCES
    }

    fn targets(&self) -> usize {
        TARGETS
    }

    fn performance(&self, source: usize) -> Option<Performance> {
        routing::PERFORMANCE[source]
    }

    fn kind(&self, target: usize) -> Kind {
        KINDS[target]
    }

    fn machine(&self, target: usize, source: usize) -> bool {
        routing::machine(target, source)
    }

    fn offered(&self, target: usize, source: usize) -> Offer {
        routing::offer(target, source)
    }

    fn key_unit(&self) -> f32 {
        KEY_UNIT_SEMITONES
    }

    /// One route alone through a real [`Graph`], published through the frame unit; Amplitude
    /// through the factor the voice applies, and the multiplier as its factor's change.
    fn deliver(&self, target: usize, source: usize, amount: f32, raw: f32) -> f32 {
        let routing = one_route(target, source, amount);
        let mut graph = Graph::new();
        graph.set_topology(&routing);
        graph.begin_sample();
        graph.write(source, raw);
        if target == PRODUCT_TARGET {
            graph.product(&routing) - 1.0
        } else if target == target::AMPLITUDE {
            standard::amplitude_factor(graph.sum(target, &routing)) - 1.0
        } else if target == target::VCO_1_PULSE_WIDTH || target == target::VCO_2_PULSE_WIDTH {
            graph.sum(target, &routing) * routing::PULSE_WIDTH_SWING
        } else {
            graph.sum(target, &routing)
        }
    }

    fn name(&self, target: usize, source: usize) -> String {
        format!("{} from {}", TARGET_NAMES[target], SOURCE_NAMES[source])
    }
}

#[cfg(test)]
mod tests {
    use mxm_modulation::conformance::{self, Case, Input};

    use super::*;
    use crate::keyboard::NoteId;
    use crate::routing::source;
    use crate::voice::{Activity, Patch, Voice};

    fn report(result: Result<(), Vec<String>>) {
        if let Err(failures) = result {
            panic!("{} failure(s):\n{}", failures.len(), failures.join("\n"));
        }
    }

    fn id(key: u8) -> NoteId {
        NoteId {
            voice_id: None,
            channel: 0,
            key,
        }
    }

    /// **Every pair means what the standard says**: offered as `standard::offer` says — VCA level
    /// ← Key refused, VCA level ← Velocity on its closing half — nothing at a source's rest, a
    /// meaningful move at full, and the standard reach for every pair the machine did not have.
    ///
    /// Falsified before trusted: with the added pitch reach left at D9's 24 semitones, it names
    /// every added pitch pair from a performance source.
    #[test]
    fn every_pair_means_what_the_standard_says() {
        report(conformance::check_declaration(&Declared));
    }

    /// **A voice publishes what the standard says**, in raw units: Key in octaves from middle C,
    /// Velocity as `v − 1`, the wheel, pressure, pedal and lever as they arrive.
    ///
    /// Falsified before trusted: publishing the raw velocity fails at every input.
    #[test]
    fn a_voice_publishes_what_the_standard_says() {
        report(conformance::check_publishers(&Declared, |from, input| {
            let mut voice = Voice::default();
            voice.set_topology(&one_route(target::CUTOFF, from, 0.0));
            let (key, velocity) = match input {
                Input::Note(n) => (n, 1.0),
                Input::Normalised(value) if from == source::VELOCITY => (60, value),
                _ => (60, 1.0),
            };
            match input {
                Input::Normalised(value) if from == source::WHEEL => {
                    voice.set_mod_wheel(0, value);
                }
                Input::Normalised(value) if from == source::PRESSURE => {
                    voice.set_channel_pressure(0, value);
                }
                Input::Normalised(value) if from == source::PEDAL => {
                    voice.set_expression(0, value);
                }
                Input::Lever(value) => voice.set_pitch_bend(0, value),
                _ => {}
            }
            voice.note_on(id(key), velocity);
            voice.process(&Patch::default());
            voice.published_for_test(from)
        }));
    }

    /// **Velocity is the press that last triggered an envelope**: under GATE+TRIG every press
    /// retriggers and brings its own; under GATE a legato press sounds its key without a retrigger
    /// and keeps the phrase's.
    ///
    /// Falsified before trusted: publishing the sounding press's velocity reads the legato press's
    /// under GATE.
    #[test]
    fn velocity_is_the_press_that_last_triggered_an_envelope() {
        use crate::voice::TriggerMode;
        for (mode, expected) in [
            (TriggerMode::GateTrigger, -0.125),
            (TriggerMode::Gate, -0.75),
        ] {
            let mut voice = Voice::default();
            voice.set_topology(&one_route(target::CUTOFF, source::VELOCITY, 0.0));
            let mut patch = Patch::default();
            patch.env1.trigger = mode;
            patch.env2.trigger = mode;
            voice.note_on(id(60), 0.25);
            voice.process(&patch);
            assert_eq!(voice.published_for_test(source::VELOCITY), -0.75);
            voice.note_on(id(72), 0.875);
            voice.process(&patch);
            assert_eq!(
                voice.published_for_test(source::VELOCITY),
                expected,
                "{mode:?}"
            );
        }
    }

    /// **Every source's Amplitude route is the standard swing at its own peak**: half an amount
    /// with the source at its peak moves the factor by exactly half — below the clamp, so the
    /// reading (`SOURCE_PEAK` × the column) and the delivery agree for the generators too, the noise
    /// and the oscillators' audio included.
    ///
    /// Falsified before trusted: with a unit column, the oscillators reach the clamp at half depth.
    #[test]
    fn every_amplitude_route_reaches_the_standard_swing_at_its_sources_peak() {
        for s in (0..SOURCES).filter(|&s| s != source::KEY) {
            let peak = routing::SOURCE_PEAK[s];
            for amount in [0.5_f32, -0.5] {
                let moved = Declared.deliver(target::AMPLITUDE, s, amount, peak);
                assert!(
                    (moved - amount).abs() < 1e-6,
                    "{}: at {amount:+} and its peak {peak} it moves the factor by {moved}",
                    SOURCE_NAMES[s]
                );
            }
        }
    }

    /// **Every source's bank-level route is the standard control's reach at its own peak**: half
    /// an amount with the source at its peak moves the level by exactly half, the generators
    /// included, as Amplitude's does.
    ///
    /// Falsified before trusted: with a unit column, the oscillators' audio moves it by a whole.
    #[test]
    fn every_bank_level_route_reaches_the_standard_reach_at_its_sources_peak() {
        for s in (0..SOURCES).filter(|&s| s != source::KEY) {
            let peak = routing::SOURCE_PEAK[s];
            for amount in [0.5_f32, -0.5] {
                let moved = Declared.deliver(target::BANK_LEVEL, s, amount, peak);
                assert!(
                    (moved - amount).abs() < 1e-6,
                    "{}: at {amount:+} and its peak {peak} it moves the level by {moved}",
                    SOURCE_NAMES[s]
                );
            }
        }
    }

    /// **The register bank obeys its Level's routes** (the owner, 2026-09-27): with a register
    /// raised and the Level knob at zero the bank is silent, and the wheel at full through +100 %
    /// plays exactly what the knob at full plays.
    ///
    /// Falsified before trusted: with the voice ignoring the target, the routed render is silent.
    #[test]
    fn the_register_bank_obeys_its_level_routes() {
        let render = |knob: f32, routed: bool| {
            let mut routing = Routing::init();
            if routed {
                routing.present[target::BANK_LEVEL][source::WHEEL] = true;
                routing.amounts[target::BANK_LEVEL][source::WHEEL] = 1.0;
                routing.compact();
            }
            let mut voice = Voice::default();
            voice.set_topology(&routing);
            voice.set_mod_wheel(0, 1.0);
            voice.note_on(id(60), 1.0);
            let patch = Patch {
                osc1_level: 0.0,
                register_levels: [0.0, 0.0, 1.0, 0.0, 0.0],
                bank_level: knob,
                ..Patch::default()
            };
            (0..2_400)
                .map(|_| voice.process(&patch))
                .collect::<Vec<_>>()
        };
        assert!(
            render(0.0, false).iter().all(|&s| s == 0.0),
            "the premise: the knob at zero is silent"
        );
        let opened = render(0.0, true);
        assert!(opened.iter().any(|s| s.abs() > 1e-4), "the route opens it");
        assert_eq!(
            opened,
            render(1.0, false),
            "exactly as the knob at full does"
        );
    }

    /// **An envelope the S&H clock fires with no key ever pressed leaves Velocity at rest** — with
    /// the S&H as the gate, the owner no press has set is full, not the `−1` a zero would publish.
    ///
    /// Falsified before trusted: with the default owner's velocity at zero it publishes `−1`.
    #[test]
    fn a_trigger_with_no_press_leaves_velocity_at_rest() {
        use crate::voice::GateSource;
        let mut voice = Voice::default();
        voice.set_topology(&one_route(target::CUTOFF, source::VELOCITY, 0.0));
        let patch = Patch {
            gate_source: GateSource::SampleHold,
            ..Patch::default()
        };
        let mut fired = false;
        for _ in 0..96_000 {
            voice.process(&patch);
            fired |= voice.activity(&patch) == Activity::Live;
            assert_eq!(voice.published_for_test(source::VELOCITY), 0.0);
        }
        assert!(fired, "the premise: the S&H clock fires the envelopes");
    }

    /// **The Amplitude target is the standard factor after the VCA**: at −100 % from a wheel at full
    /// it silences the voice exactly, and at +100 % doubles it.
    ///
    /// Falsified before trusted: with the voice's factor removed, the doubled render equals the
    /// plain one.
    #[test]
    fn amplitude_is_the_standard_factor_after_the_vca() {
        let render = |amount: Option<f32>| {
            let mut routing = Routing::init();
            if let Some(amount) = amount {
                routing.present[target::AMPLITUDE][source::WHEEL] = true;
                routing.amounts[target::AMPLITUDE][source::WHEEL] = amount;
                routing.compact();
            }
            let mut voice = Voice::default();
            voice.set_topology(&routing);
            voice.set_mod_wheel(0, 1.0);
            voice.note_on(id(60), 1.0);
            let patch = Patch::default();
            (0..2_400)
                .map(|_| voice.process(&patch))
                .collect::<Vec<_>>()
        };
        let plain = render(None);
        assert!(
            plain.iter().any(|s| s.abs() > 1e-4),
            "the premise: it sounds"
        );
        assert!(render(Some(-1.0)).iter().all(|&s| s == 0.0), "silence");
        for (doubled, plain) in render(Some(1.0)).iter().zip(&plain) {
            assert_eq!(*doubled, plain * 2.0);
        }
    }

    /// **After a release, no performance route holds a note open** — every offered pair, on its
    /// offered half, at the softest and hardest notes and the keyboard's ends, gestures held at full
    /// through the note and let go at the release. VCA level ← the lever, the wheel, pressure and the
    /// pedal are the machine's own amplifier held open by a parked control, which is what that CV
    /// input does; they are declared drones.
    #[test]
    fn after_a_release_no_performance_route_holds_a_note_open() {
        let drones = [
            (target::VCA_LEVEL, source::BEND),
            (target::VCA_LEVEL, source::WHEEL),
            (target::VCA_LEVEL, source::PRESSURE),
            (target::VCA_LEVEL, source::PEDAL),
        ];
        report(conformance::check_release_silence(
            &Declared,
            &drones,
            |case: Case| {
                let mut routing = Routing::init();
                routing.present[case.target][case.source] = true;
                routing.amounts[case.target][case.source] = case.amount;
                routing.compact();
                let mut voice = Voice::default();
                voice.set_topology(&routing);
                let mut patch = Patch::default();
                patch.env1.release = 0.02;
                patch.env2.release = 0.02;
                voice.set_mod_wheel(0, 1.0);
                voice.set_channel_pressure(0, 1.0);
                voice.set_expression(0, 1.0);
                voice.set_pitch_bend(0, 1.0);
                voice.note_on(id(case.key), case.velocity);
                for _ in 0..4_800 {
                    voice.process(&patch);
                }
                voice.note_off(None, 0, case.key);
                voice.set_mod_wheel(0, 0.0);
                voice.set_channel_pressure(0, 0.0);
                voice.set_expression(0, 0.0);
                voice.set_pitch_bend(0, 0.0);
                (0..96_000).any(|_| {
                    voice.process(&patch) == 0.0 && voice.activity(&patch) == Activity::Inert
                })
            },
        ));
    }
}
