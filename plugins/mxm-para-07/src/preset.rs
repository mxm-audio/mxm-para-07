//! This instrument's shared-preset adapter and fifty factory sounds.

use crate::params::{MxmPara07Params, all_parameters};
use std::sync::RwLock;

pub use mxm_preset::{Category, INIT_NAME, Preset, PresetIdentity, Value, factory};

/// **The tempo syncs this plugin gained on 2026-09-25** (`plans/plan-tempo-sync-controls.md`). A
/// preset file written before them was written unsynced, so each loads off rather than keeping the
/// instance's sync, and without reporting a missing control.
pub(crate) const TEMPO_SYNC_IDS: &[&str] = &["lfosync", "shsync"];

impl mxm_preset::Instrument for MxmPara07Params {
    fn clap_id(&self) -> &'static str {
        crate::CLAP_ID
    }
    fn parameters(&self) -> Vec<(&'static str, &dyn mxm_preset::ErasedParam)> {
        all_parameters(self)
            .into_iter()
            .chain(self.routes.parameters())
            .collect()
    }
    fn identity(&self) -> &RwLock<PresetIdentity> {
        &self.preset
    }
    fn factory_files(&self) -> &'static [(&'static str, &'static str)] {
        FACTORY_FILES
    }

    fn default_missing_legacy_parameter(&self, id: &str) -> bool {
        TEMPO_SYNC_IDS.contains(&id)
    }
}

pub const FACTORY_FILES: &[(&str, &str)] = &[
    ("Open saw", include_str!("../presets/open-saw.json")),
    (
        "Split octaves",
        include_str!("../presets/split-octaves.json"),
    ),
    ("Wide pair", include_str!("../presets/wide-pair.json")),
    (
        "Narrow beating",
        include_str!("../presets/narrow-beating.json"),
    ),
    ("Square pair", include_str!("../presets/square-pair.json")),
    ("Triangle air", include_str!("../presets/triangle-air.json")),
    ("Pulse motion", include_str!("../presets/pulse-motion.json")),
    (
        "Envelope pulse",
        include_str!("../presets/envelope-pulse.json"),
    ),
    ("Core lock", include_str!("../presets/core-lock.json")),
    ("Broken lock", include_str!("../presets/broken-lock.json")),
    (
        "Register organ",
        include_str!("../presets/register-organ.json"),
    ),
    (
        "Deep registers",
        include_str!("../presets/deep-registers.json"),
    ),
    (
        "Bright registers",
        include_str!("../presets/bright-registers.json"),
    ),
    ("Hollow stack", include_str!("../presets/hollow-stack.json")),
    ("Full stack", include_str!("../presets/full-stack.json")),
    ("Ring bell", include_str!("../presets/ring-bell.json")),
    (
        "Metallic ring",
        include_str!("../presets/metallic-ring.json"),
    ),
    ("Low ring", include_str!("../presets/low-ring.json")),
    ("White wind", include_str!("../presets/white-wind.json")),
    ("Pink breath", include_str!("../presets/pink-breath.json")),
    ("Round bass", include_str!("../presets/round-bass.json")),
    (
        "Register bass",
        include_str!("../presets/register-bass.json"),
    ),
    ("Split bass", include_str!("../presets/split-bass.json")),
    (
        "Tracking bass",
        include_str!("../presets/tracking-bass.json"),
    ),
    ("Rubber pulse", include_str!("../presets/rubber-pulse.json")),
    ("Clear lead", include_str!("../presets/clear-lead.json")),
    ("Glide lead", include_str!("../presets/glide-lead.json")),
    ("Upward glide", include_str!("../presets/upward-glide.json")),
    (
        "Downward glide",
        include_str!("../presets/downward-glide.json"),
    ),
    ("Auto rise", include_str!("../presets/auto-rise.json")),
    ("Auto fall", include_str!("../presets/auto-fall.json")),
    (
        "Delayed vibrato",
        include_str!("../presets/delayed-vibrato.json"),
    ),
    (
        "Stepped pitch",
        include_str!("../presets/stepped-pitch.json"),
    ),
    (
        "Stepped filter",
        include_str!("../presets/stepped-filter.json"),
    ),
    ("Clocked gate", include_str!("../presets/clocked-gate.json")),
    ("Random drift", include_str!("../presets/random-drift.json")),
    (
        "Filter chatter",
        include_str!("../presets/filter-chatter.json"),
    ),
    (
        "Audio contour",
        include_str!("../presets/audio-contour.json"),
    ),
    ("Soft brass", include_str!("../presets/soft-brass.json")),
    ("Hard brass", include_str!("../presets/hard-brass.json")),
    ("Slow strings", include_str!("../presets/slow-strings.json")),
    ("Square pad", include_str!("../presets/square-pad.json")),
    ("Moving pad", include_str!("../presets/moving-pad.json")),
    (
        "Dual envelope",
        include_str!("../presets/dual-envelope.json"),
    ),
    (
        "Second envelope",
        include_str!("../presets/second-envelope.json"),
    ),
    ("Short click", include_str!("../presets/short-click.json")),
    (
        "Filter whistle",
        include_str!("../presets/filter-whistle.json"),
    ),
    ("Held drone", include_str!("../presets/held-drone.json")),
    (
        "Clocked drone",
        include_str!("../presets/clocked-drone.json"),
    ),
    ("Noise drone", include_str!("../presets/noise-drone.json")),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// **A session or preset that chose the retired External key mode loads as One-pitch**, and a
    /// preset saved with three modes keeps its One-pitch and Two-pitch.
    ///
    /// Falsified before trusted: without `retire_external_key_mode` the session's key mode stays
    /// `external`, an id the parameter no longer knows.
    #[test]
    fn a_state_that_chose_external_loads_as_one_pitch() {
        use crate::params::KeyModeKind;
        use nice_plug::plugin::ParamValue;
        use nice_plug::prelude::{Param as _, Plugin as _};

        let mut state = nice_plug::prelude::PluginState {
            version: String::new(),
            params: Default::default(),
            fields: Default::default(),
        };
        state.params.insert(
            "keymode".to_owned(),
            ParamValue::String("external".to_owned()),
        );
        crate::MxmPara07::filter_state(&mut state);
        assert!(
            matches!(state.params.get("keymode"), Some(ParamValue::String(id)) if id == "one"),
            "{:?}",
            state.params.get("keymode")
        );

        let params = crate::params::MxmPara07Params::default();
        for (stored, expected) in [
            (1.0, KeyModeKind::OnePitch),
            (0.5, KeyModeKind::OnePitch),
            (0.0, KeyModeKind::TwoPitch),
        ] {
            assert_eq!(params.key_mode.preview_plain(stored), expected, "{stored}");
        }
    }

    /// **A project saved before the tempo syncs restores them Off** (`mxm_preset::add_switches_off`),
    /// whatever this instance had.
    #[test]
    fn an_older_state_restores_the_tempo_syncs_off() {
        use nice_plug::prelude::Plugin as _;
        let mut state = nice_plug::prelude::PluginState {
            version: String::new(),
            params: Default::default(),
            fields: Default::default(),
        };
        crate::MxmPara07::filter_state(&mut state);
        for id in TEMPO_SYNC_IDS {
            assert!(
                matches!(
                    state.params.get(*id),
                    Some(nice_plug::plugin::ParamValue::Bool(false))
                ),
                "{{id}} was not restored off"
            );
        }
    }

    /// **A preset saved before the tempo syncs loads them off, and cleanly** ([`TEMPO_SYNC_IDS`]).
    #[test]
    fn a_preset_from_before_the_tempo_syncs_loads_them_off() {
        let params = crate::params::MxmPara07Params::default();
        let mut old = mxm_preset::Preset::init(&params);
        for id in TEMPO_SYNC_IDS {
            old.params.remove(*id);
        }
        let (writes, problems) = old.resolve(&params);
        assert!(problems.is_empty(), "{{problems:?}}");
        for id in TEMPO_SYNC_IDS {
            assert!(
                writes.iter().any(|(w, _, v)| w == id && *v == 0.0),
                "{{id}} was not written off"
            );
        }
    }

    /// **A preset naming the external input's retired ids loads everything else** (the owner,
    /// 2026-09-26): a user preset saved while the input was here names its seventeen ids, which are
    /// reported and skipped, and every live parameter is still written.
    #[test]
    fn a_preset_naming_the_external_inputs_retired_ids_loads_everything_else() {
        let params = crate::params::MxmPara07Params::default();
        let mut old = mxm_preset::Preset::init(&params);
        let retired = &crate::params::RETIRED_IDS[28..45];
        assert_eq!(retired.len(), 17);
        for id in retired {
            old.params.insert(
                (*id).to_owned(),
                mxm_preset::Value {
                    v: 1.0,
                    text: String::new(),
                },
            );
        }
        let (writes, problems) = old.resolve(&params);
        assert_eq!(problems.len(), retired.len(), "{problems:?}");
        for id in retired {
            assert!(
                problems.iter().any(|p| p.contains(&format!("`{id}`"))),
                "`{id}` was not the one skipped: {problems:?}"
            );
        }
        assert_eq!(
            writes.len(),
            Instrument::parameters(&params).len(),
            "every live parameter still applies"
        );
    }

    use mxm_preset::{Instrument, user_root};

    type Design = (&'static str, Category, &'static [(&'static str, f32)]);
    const FACTORY_DESIGN: &[Design] = &[
        ("Open saw", Category::Template, &[("cutoff", 0.88)]),
        (
            "Split octaves",
            Category::Lead,
            &[("vco2", 0.75), ("vco2range", 0.75), ("cutoff", 0.72)],
        ),
        (
            "Wide pair",
            Category::Pad,
            &[
                ("vco2", 0.8),
                ("vco2tune", 0.58),
                ("env1release", 0.55),
                ("cutoff", 0.65),
            ],
        ),
        (
            "Narrow beating",
            Category::Keys,
            &[("vco2", 0.7), ("vco2tune", 0.51), ("cutoff", 0.75)],
        ),
        (
            "Square pair",
            Category::Lead,
            &[
                ("vco1wave", 0.67),
                ("vco2", 0.75),
                ("cutoff", 0.68),
                ("resonance", 0.18),
            ],
        ),
        (
            "Triangle air",
            Category::Lead,
            &[
                ("vco1wave", 0.0),
                ("cutoff", 0.78),
                ("env1attack", 0.3),
                ("env1release", 0.42),
            ],
        ),
        (
            "Pulse motion",
            Category::Pad,
            &[
                ("vco1wave", 1.0),
                ("lforate", 0.28),
                ("mod_vco1pitch_lfo", 0.53),
                ("mod_vco1pw_lfotri", 0.875),
            ],
        ),
        (
            "Envelope pulse",
            Category::Pluck,
            &[
                ("vco1wave", 1.0),
                ("env1decay", 0.35),
                ("env1sustain", 0.1),
                ("mod_vco1pw_env1on", 1.0),
                ("mod_vco1pw_env1", 0.925),
                ("mod_cutoff_env1", 0.7),
            ],
        ),
        (
            "Core lock",
            Category::Lead,
            &[
                ("vco2", 0.9),
                ("sync", 1.0),
                ("vco2tune", 0.7),
                ("mod_cutoff_env1", 0.65),
            ],
        ),
        (
            "Broken lock",
            Category::Fx,
            &[
                ("vco2", 0.8),
                ("sync", 1.0),
                ("vco2range", 0.0),
                ("vco2tune", 0.92),
                ("resonance", 0.4),
            ],
        ),
        (
            "Register organ",
            Category::Keys,
            &[
                ("vco1", 0.0),
                ("bank", 0.8),
                ("reg16", 0.7),
                ("reg8", 1.0),
                ("reg4", 0.55),
                ("env1attack", 0.08),
            ],
        ),
        (
            "Deep registers",
            Category::Bass,
            &[
                ("vco1", 0.0),
                ("bank", 0.9),
                ("reg32", 1.0),
                ("reg16", 0.65),
                ("cutoff", 0.45),
            ],
        ),
        (
            "Bright registers",
            Category::Keys,
            &[
                ("vco1", 0.2),
                ("bank", 0.9),
                ("reg8", 0.5),
                ("reg4", 0.8),
                ("reg2", 1.0),
                ("cutoff", 0.8),
            ],
        ),
        (
            "Hollow stack",
            Category::Pad,
            &[
                ("vco1", 0.0),
                ("bank", 0.85),
                ("reg32", 0.5),
                ("reg8", 1.0),
                ("reg2", 0.45),
                ("env1release", 0.5),
            ],
        ),
        (
            "Full stack",
            Category::Keys,
            &[
                ("vco1", 0.0),
                ("bank", 0.75),
                ("reg32", 0.4),
                ("reg16", 0.55),
                ("reg8", 1.0),
                ("reg4", 0.55),
                ("reg2", 0.4),
            ],
        ),
        (
            "Ring bell",
            Category::Percussion,
            &[
                ("vco1", 0.15),
                ("fifth", 0.45),
                ("vco2tune", 0.78),
                ("env1decay", 0.42),
                ("env1sustain", 0.0),
                ("resonance", 0.25),
            ],
        ),
        (
            "Metallic ring",
            Category::Fx,
            &[
                ("vco1", 0.1),
                ("fifth", 0.7),
                ("vco2range", 1.0),
                ("vco2tune", 0.83),
                ("cutoff", 0.8),
            ],
        ),
        (
            "Low ring",
            Category::Bass,
            &[
                ("vco1range", 0.25),
                ("fifth", 0.35),
                ("vco2range", 0.25),
                ("vco2tune", 0.65),
                ("cutoff", 0.4),
            ],
        ),
        (
            "White wind",
            Category::Fx,
            &[
                ("vco1", 0.0),
                ("noise", 0.7),
                ("cutoff", 0.48),
                ("env1attack", 0.5),
                ("env1release", 0.58),
            ],
        ),
        (
            "Pink breath",
            Category::Fx,
            &[
                ("vco1", 0.0),
                ("noise", 0.75),
                ("noisecolour", 1.0),
                ("cutoff", 0.38),
                ("env1attack", 0.55),
                ("env1release", 0.62),
            ],
        ),
        (
            "Round bass",
            Category::Bass,
            &[
                ("vco1wave", 0.0),
                ("vco1range", 0.25),
                ("cutoff", 0.38),
                ("env1decay", 0.38),
                ("env1sustain", 0.25),
                ("mod_cutoff_env1", 0.725),
            ],
        ),
        (
            "Register bass",
            Category::Bass,
            &[
                ("vco1", 0.25),
                ("bank", 0.7),
                ("reg32", 1.0),
                ("reg16", 0.55),
                ("cutoff", 0.34),
                ("resonance", 0.25),
            ],
        ),
        (
            "Split bass",
            Category::Bass,
            &[
                ("vco1range", 0.25),
                ("vco2", 0.75),
                ("vco2range", 0.0),
                ("cutoff", 0.42),
                ("env1decay", 0.35),
                ("env1sustain", 0.3),
            ],
        ),
        (
            "Tracking bass",
            Category::Bass,
            &[
                ("vco1range", 0.25),
                ("cutoff", 0.3),
                ("resonance", 0.38),
                ("mod_cutoff_key", 0.85),
                ("mod_cutoff_env1", 0.71),
            ],
        ),
        (
            "Rubber pulse",
            Category::Bass,
            &[
                ("vco1wave", 1.0),
                ("vco1width", 0.65),
                ("vco1range", 0.25),
                ("cutoff", 0.3),
                ("env1decay", 0.34),
                ("env1sustain", 0.08),
                ("mod_cutoff_env1", 0.775),
            ],
        ),
        (
            "Clear lead",
            Category::Lead,
            &[
                ("vco2", 0.45),
                ("vco2tune", 0.52),
                ("cutoff", 0.82),
                ("env1release", 0.32),
            ],
        ),
        (
            "Glide lead",
            Category::Lead,
            &[
                ("vco2", 0.55),
                ("portamento", 0.28),
                ("cutoff", 0.66),
                ("lfodelay", 0.42),
                ("mod_vco1pitch_lfo", 0.525),
            ],
        ),
        (
            "Upward glide",
            Category::Lead,
            &[
                ("portamento", 0.4),
                ("portamentomode", 0.5),
                ("vco2", 0.45),
                ("cutoff", 0.62),
            ],
        ),
        (
            "Downward glide",
            Category::Lead,
            &[
                ("portamento", 0.4),
                ("portamentomode", 1.0),
                ("vco2", 0.45),
                ("cutoff", 0.62),
            ],
        ),
        (
            "Auto rise",
            Category::Lead,
            &[
                ("autobenddirection", 1.0),
                ("autobendtime", 0.38),
                ("cutoff", 0.58),
                ("mod_vco1pitch_auto", 0.8),
                ("mod_vco1pitch_autoon", 1.0),
            ],
        ),
        (
            "Auto fall",
            Category::Lead,
            &[
                ("autobendtime", 0.3),
                ("cutoff", 0.58),
                ("resonance", 0.2),
                ("mod_vco1pitch_auto", 0.825),
                ("mod_vco1pitch_autoon", 1.0),
            ],
        ),
        (
            "Delayed vibrato",
            Category::Lead,
            &[
                ("lfodelay", 0.62),
                ("lforate", 0.4),
                ("cutoff", 0.72),
                ("mod_vco1pitch_lfo", 0.55),
                ("mod_vco2pitch_lfo", 0.55),
            ],
        ),
        (
            "Stepped pitch",
            Category::Sequence,
            &[
                ("shtime", 0.3),
                ("shsource", 1.0),
                ("cutoff", 0.6),
                ("mod_vco1pitch_sh", 0.675),
                ("mod_vco1pitch_shon", 1.0),
                ("mod_vco2pitch_sh", 0.675),
                ("mod_vco2pitch_shon", 1.0),
            ],
        ),
        (
            "Stepped filter",
            Category::Sequence,
            &[
                ("shtime", 0.25),
                ("resonance", 0.38),
                ("cutoff", 0.4),
                ("mod_cutoff_lfoon", 0.0),
                ("mod_cutoff_shon", 1.0),
                ("mod_cutoff_sh", 0.775),
            ],
        ),
        (
            "Clocked gate",
            Category::Sequence,
            &[
                // One-pitch with the S&H clock: the retired External mode's automatic performance.
                ("keymode", 1.0),
                ("gatesource", 1.0),
                ("shtime", 0.2),
                ("env1decay", 0.25),
                ("env1sustain", 0.2),
                ("cutoff", 0.55),
            ],
        ),
        (
            "Random drift",
            Category::Sequence,
            &[
                ("shtime", 0.65),
                ("shlag", 0.55),
                ("cutoff", 0.55),
                ("mod_vco1pitch_sh", 0.54),
                ("mod_vco1pitch_shon", 1.0),
                ("mod_cutoff_lfoon", 0.0),
                ("mod_cutoff_shon", 1.0),
                ("mod_cutoff_sh", 0.59000003),
            ],
        ),
        (
            "Filter chatter",
            Category::Fx,
            &[
                ("cutoff", 0.28),
                ("resonance", 0.65),
                ("vco2", 0.4),
                ("mod_cutoff_vco2", 0.85),
            ],
        ),
        (
            "Audio contour",
            Category::Fx,
            &[
                ("noise", 0.25),
                ("cutoff", 0.35),
                ("resonance", 0.55),
                ("mod_cutoff_noiseon", 1.0),
                ("mod_cutoff_noise", 0.825),
                ("mod_cutoff_vco2on", 0.0),
            ],
        ),
        (
            "Soft brass",
            Category::Brass,
            &[
                ("vco2", 0.65),
                ("cutoff", 0.42),
                ("env1attack", 0.25),
                ("env1decay", 0.45),
                ("env1sustain", 0.65),
                ("env1release", 0.35),
                ("mod_cutoff_env1", 0.775),
            ],
        ),
        (
            "Hard brass",
            Category::Brass,
            &[
                ("vco2", 0.85),
                ("vco2wave", 0.33),
                ("cutoff", 0.35),
                ("env1attack", 0.08),
                ("env1decay", 0.35),
                ("env1sustain", 0.5),
                ("mod_cutoff_env1", 0.86),
            ],
        ),
        (
            "Slow strings",
            Category::Strings,
            &[
                ("vco2", 0.72),
                ("vco2tune", 0.53),
                ("cutoff", 0.5),
                ("env1attack", 0.58),
                ("env1release", 0.65),
                ("mod_vco1pitch_lfo", 0.52),
            ],
        ),
        (
            "Square pad",
            Category::Pad,
            &[
                ("vco1wave", 0.67),
                ("vco2", 0.7),
                ("vco2wave", 0.67),
                ("cutoff", 0.48),
                ("env1attack", 0.5),
                ("env1release", 0.62),
            ],
        ),
        (
            "Moving pad",
            Category::Pad,
            &[
                ("vco2", 0.75),
                ("lforate", 0.22),
                ("env1attack", 0.55),
                ("env1release", 0.64),
                ("mod_vco1pitch_lfo", 0.52),
                ("mod_vco2pitch_lfo", 0.52),
                ("mod_cutoff_lfo", 0.59000003),
            ],
        ),
        (
            "Dual envelope",
            Category::Pad,
            &[
                ("env2attack", 0.6),
                ("env2release", 0.62),
                ("env1attack", 0.3),
                ("cutoff", 0.3),
                ("vco2", 0.65),
                ("mod_cutoff_env1", 0.775),
                ("mod_amp_env1on", 0.0),
                ("mod_amp_env2on", 1.0),
                ("mod_amp_env2", 1.0),
            ],
        ),
        (
            "Second envelope",
            Category::Keys,
            &[
                ("env2decay", 0.42),
                ("env2sustain", 0.2),
                ("env2release", 0.35),
                ("cutoff", 0.7),
                ("bank", 0.3),
                ("reg4", 1.0),
                ("mod_amp_env1on", 0.0),
                ("mod_amp_env2on", 1.0),
                ("mod_amp_env2", 1.0),
            ],
        ),
        (
            "Short click",
            Category::Percussion,
            &[
                ("env1attack", 0.0),
                ("env1decay", 0.0),
                ("env1sustain", 0.0),
                ("env1release", 0.0),
                ("cutoff", 0.45),
                ("mod_cutoff_env1", 0.825),
            ],
        ),
        (
            "Filter whistle",
            Category::Lead,
            &[
                ("vco1", 0.15),
                ("cutoff", 0.5),
                ("resonance", 0.9),
                ("env1release", 0.4),
                ("mod_cutoff_key", 0.825),
            ],
        ),
        (
            "Held drone",
            Category::Drone,
            &[
                ("hold", 0.65),
                ("vco2", 0.6),
                ("vco2tune", 0.54),
                ("cutoff", 0.4),
                ("mod_cutoff_lfo", 0.56),
            ],
        ),
        (
            "Clocked drone",
            Category::Drone,
            &[
                // One-pitch with the S&H clock, as Clocked gate.
                ("keymode", 1.0),
                ("gatesource", 1.0),
                ("shtime", 0.55),
                ("vco2", 0.5),
                ("cutoff", 0.42),
                ("env1release", 0.5),
            ],
        ),
        (
            "Noise drone",
            Category::Drone,
            &[
                ("hold", 0.55),
                ("vco1", 0.2),
                ("noise", 0.5),
                ("noisecolour", 1.0),
                ("cutoff", 0.3),
                ("mod_cutoff_lfoon", 0.0),
                ("mod_cutoff_shon", 1.0),
                ("mod_cutoff_sh", 0.6),
            ],
        ),
    ];

    fn generated(
        params: &MxmPara07Params,
        name: &str,
        category: Category,
        overrides: &[(&str, f32)],
    ) -> Preset {
        let mut preset = Preset::init(params);
        preset.name = name.to_owned();
        preset.category = category;
        // Routing ids included: a converted design names route pairs, not the retired knobs.
        let bindings: Vec<_> = all_parameters(params)
            .into_iter()
            .chain(params.routes.parameters())
            .collect();
        for (id, value) in overrides {
            let (_, param) = bindings
                .iter()
                .find(|(candidate, _)| candidate == id)
                .unwrap_or_else(|| panic!("{name:?} names unknown parameter {id:?}"));
            preset.params.insert(
                (*id).to_owned(),
                Value {
                    v: *value,
                    text: param.format(*value),
                },
            );
        }
        preset
    }

    #[test]
    #[ignore = "writes factory preset files"]
    fn write_the_factory_presets() {
        let params = MxmPara07Params::default();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("presets");
        std::fs::create_dir_all(&root).unwrap();
        for (name, category, overrides) in FACTORY_DESIGN {
            let file = root.join(format!("{}.json", name.to_lowercase().replace(' ', "-")));
            std::fs::write(
                file,
                generated(&params, name, *category, overrides).to_json(),
            )
            .unwrap();
        }
    }
    #[test]
    fn factory_contracts_hold() {
        assert_eq!(FACTORY_FILES.len(), 50);
        assert_eq!(FACTORY_DESIGN.len(), 50);
        let params = MxmPara07Params::default();
        for (name, category, overrides) in FACTORY_DESIGN {
            assert_ne!(*category, Category::Uncategorised);
            for (id, v) in *overrides {
                assert!((0.0..=1.0).contains(v), "{name}: {id}={v}");
                assert!(
                    all_parameters(&params).iter().any(|(known, _)| known == id)
                        || params
                            .routes
                            .parameters()
                            .iter()
                            .any(|(known, _)| known == id),
                    "{name}: {id}"
                );
            }
        }
    }

    /// The shipped file against the design with each value compared within rounding, everything
    /// else exactly. The files hold Windows' bits, and each platform's maths library rounds in its
    /// own way: macOS computes a gain's normalised value one step away (the owner, 2026-10-06: pin
    /// on Windows only).
    fn assert_same_within_rounding(shipped: &Preset, designed: &Preset, name: &str) {
        for (id, value) in &shipped.params {
            let by_design = designed
                .params
                .get(id)
                .unwrap_or_else(|| panic!("regenerate {name}: {id} is not in the design"));
            assert!(
                (value.v - by_design.v).abs() <= 1.0e-6,
                "regenerate {name}: {id} is {} in the file and {} by design",
                value.v,
                by_design.v
            );
        }
        let (mut shipped, mut designed) = (shipped.clone(), designed.clone());
        for value in shipped
            .params
            .values_mut()
            .chain(designed.params.values_mut())
        {
            value.v = 0.0;
        }
        assert_eq!(shipped, designed, "regenerate {name}");
    }

    #[test]
    fn shipped_factory_files_match_the_design_and_are_complete() {
        let params = MxmPara07Params::default();
        for (name, category, overrides) in FACTORY_DESIGN {
            let text = FACTORY_FILES.iter().find(|(n, _)| n == name).unwrap().1;
            let shipped = Preset::parse(text, crate::CLAP_ID).unwrap();
            let designed = generated(&params, name, *category, overrides);
            if cfg!(target_os = "windows") {
                assert_eq!(shipped, designed, "regenerate {name}");
            } else {
                assert_same_within_rounding(&shipped, &designed, name);
            }
            assert!(
                shipped.resolve(&params).1.is_empty(),
                "{name} is incomplete"
            );
        }
    }
    #[test]
    fn every_factory_sound_is_distinct_categorised_and_audible() {
        for (i, (name, text)) in FACTORY_FILES.iter().enumerate() {
            let a = Preset::parse(text, crate::CLAP_ID).unwrap();
            assert_ne!(a.category, Category::Uncategorised);
            let v = |id: &str| a.params.get(id).map_or(0.0, |x| x.v);
            assert!(
                v("vco1") > 0.02
                    || (v("bank") > 0.02
                        && ["reg32", "reg16", "reg8", "reg4", "reg2"]
                            .iter()
                            .any(|id| v(id) > 0.02))
                    || v("noise") > 0.02
                    || v("fifth") > 0.02,
                "{name} has no source"
            );
            assert!(
                v("volume") > 0.02 && (v("cutoff") > 0.05 || v("filterenv") > 0.2),
                "{name} is inaudible"
            );
            for (other, text) in &FACTORY_FILES[i + 1..] {
                let b = Preset::parse(text, crate::CLAP_ID).unwrap();
                assert_ne!(a.params, b.params, "{name} duplicates {other}");
            }
        }
    }
    #[test]
    fn init_is_generated_from_defaults_and_the_list_begins_with_it() {
        let params = MxmPara07Params::default();
        let list = factory(&params);
        assert_eq!(list[0].name, INIT_NAME);
        assert_eq!(list.len(), 51);
        let init = Preset::init(&params);
        for (id, param) in params.parameters() {
            assert_eq!(init.params[id].v, param.default_normalised(), "{id}");
        }
    }
    #[test]
    fn user_storage_is_namespaced_by_the_permanent_id() {
        if let Some(root) = user_root(crate::CLAP_ID) {
            assert!(root.ends_with("presets"));
            assert!(root.to_string_lossy().contains(crate::CLAP_ID));
        }
    }
}
