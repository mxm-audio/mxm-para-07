//! mxm-para-07 — two keyed pitches through one shared articulation path.
//!
//! The product is an original CLAP identity over the framework-free `mxm-para-07-dsp` machine.
//! It has no effect: the source instrument shipped with none, and it has no note output and no
//! sequencer. Velocity, the mod wheel and channel pressure are routable modulation sources the
//! machine itself had none of. There is no audio input: the external input and everything on it
//! were removed by the owner on 2026-09-26.

/// The only product-name literal in this crate.
macro_rules! plugin_name {
    () => {
        "mxm-para-07"
    };
}
pub const NAME: &str = plugin_name!();
pub const CLAP_ID: &str = concat!("dk.mxm.", plugin_name!());

mod editor;
mod params;
pub mod preset;
mod routes;
mod telemetry;

use mxm_para_07_dsp::MIN_SAMPLE_RATE;
use mxm_para_07_dsp::keyboard::NoteId;
use mxm_para_07_dsp::routing::Routing;
use mxm_para_07_dsp::voice::{Activity, EnvelopePatch, OscPatch, Patch, Voice};
use nice_plug::midi::{Channel, Key, VoiceID};
use nice_plug::prelude::*;
use params::MxmPara07Params;
use std::sync::Arc;

/// A note's identity in the shape the voice logic was written for. nice-plug 0.4 types it
/// (`VoiceID`, `Channel`, `Key`, each with a wildcard); 0.3 handed over a host's wildcard (-1) as
/// 255 and a missing voice id as `None`. Converting here keeps every note decision, and every
/// recorded render, exactly what it was before the upgrade.
fn legacy_note(voice_id: VoiceID, channel: Channel, key: Key) -> (Option<i32>, u8, u8) {
    (
        voice_id.id(),
        channel.number().unwrap_or(u8::MAX),
        key.number().unwrap_or(u8::MAX),
    )
}

const MAX_BLOCK_SIZE: usize = 64;
const DEV_VIEW_CC: u8 = 119;
const DEV_DISCLOSURE_CC: u8 = 118;
const DEV_BROWSER_CC: u8 = 117;
const DEV_THEME_CC: u8 = 116;
const DEV_CC_ENV: &str = "MXM_DEV_CC";

pub struct MxmPara07 {
    params: Arc<MxmPara07Params>,
    voice: Voice,
    sample_rate: f32,
    telemetry: Arc<telemetry::Telemetry>,
    dev_cc: bool,
    trigger_high: bool,
    was_inert: bool,
    last_key_mode: mxm_para_07_dsp::voice::KeyMode,
    last_gate_source: mxm_para_07_dsp::voice::GateSource,
    last_hold_open: bool,
    last_patch: Patch,
    /// The topology this interval is running, resolved from the panel's routing parameters.
    ///
    /// Presence is resolved once per interval because it decides the compacted live lists; the
    /// amounts are written per sample into `voice.routing_mut()`.
    routing: Routing,
    /// The LFO rate and the sample time as their syncs resolved them for this callback, or `None`
    /// for their free values (`plans/plan-tempo-sync-controls.md`).
    synced_lfo_hz: Option<f32>,
    synced_sh_s: Option<f32>,
    /// Samples between two of the scope's points, from the sample rate, and how many remain until
    /// the next (`telemetry::SCOPE_RATE_HZ`).
    scope_stride: u32,
    scope_countdown: u32,
}

impl Default for MxmPara07 {
    fn default() -> Self {
        let patch = Patch::default();
        Self {
            params: Arc::new(MxmPara07Params::default()),
            voice: Voice::default(),
            sample_rate: 48_000.0,
            telemetry: telemetry::Telemetry::shared(),
            dev_cc: std::env::var_os(DEV_CC_ENV).is_some(),
            trigger_high: false,
            was_inert: true,
            last_key_mode: patch.key_mode,
            last_gate_source: patch.gate_source,
            last_hold_open: false,
            last_patch: patch,
            routing: Routing::new(),
            synced_lfo_hz: None,
            synced_sh_s: None,
            scope_stride: 48,
            scope_countdown: 48,
        }
    }
}

impl MxmPara07 {
    fn supports_sample_rate(sample_rate: f32) -> bool {
        sample_rate.is_finite() && sample_rate >= MIN_SAMPLE_RATE
    }

    /// Advance every smoother exactly once and build the DSP's plain-value patch.
    #[inline]
    fn next_patch(&self) -> Patch {
        let p = &self.params;
        // **The width knob is the target's base** (§5), and always: what a route adds is the
        // route's own amount. The selector that used to zero this is retiring.
        let osc1_width = p.vco1_width.smoothed.next();
        let osc2_width = p.vco2_width.smoothed.next();
        Patch {
            key_mode: p.key_mode.value().into(),
            gate_source: p.gate_source.value().into(),
            portamento_s: p.portamento.value(),
            portamento_mode: p.portamento_mode.value().into(),
            total_tune_semitones: p.tune.smoothed.next(),
            vco2_tune_semitones: p.vco2_tune.smoothed.next(),
            osc1: OscPatch {
                range: p.vco1_range.value().into(),
                waveform: p.vco1_wave.value().into(),
                pulse_width: osc1_width,
            },
            osc2: OscPatch {
                range: p.vco2_range.value().into(),
                waveform: p.vco2_wave.value().into(),
                pulse_width: osc2_width,
            },
            sync: p.sync.value(),
            register_levels: [
                p.reg32.smoothed.next(),
                p.reg16.smoothed.next(),
                p.reg8.smoothed.next(),
                p.reg4.smoothed.next(),
                p.reg2.smoothed.next(),
            ],
            bank_level: p.bank.smoothed.next(),
            osc1_level: p.vco1.smoothed.next(),
            osc2_level: p.vco2.smoothed.next(),
            noise_level: p.noise.smoothed.next(),
            fifth_level: p.fifth.smoothed.next(),
            noise_colour: p.noise_colour.value().into(),
            hpf_hz: p.hpf.smoothed.next(),
            cutoff_hz: p.cutoff.smoothed.next(),
            resonance: p.resonance.smoothed.next(),
            env1: EnvelopePatch {
                attack: p.env1_attack.value(),
                decay: p.env1_decay.value(),
                sustain: p.env1_sustain.smoothed.next(),
                release: p.env1_release.value(),
                trigger: p.env1_trigger.value().into(),
            },
            env2: EnvelopePatch {
                attack: p.env2_attack.value(),
                decay: p.env2_decay.value(),
                sustain: p.env2_sustain.smoothed.next(),
                release: p.env2_release.value(),
                trigger: p.env2_trigger.value().into(),
            },
            hold: p.hold.smoothed.next(),
            lfo_wave: p.lfo_shape.value().into(),
            lfo_rate_hz: self.synced_lfo_hz.unwrap_or_else(|| p.lfo_rate.value()),
            lfo_delay_s: p.lfo_delay.value(),
            keyboard_trigger_lfo: p.lfo_key_trigger.value(),
            sh_source: p.sh_source.value().into(),
            sh_sample_time_s: self.synced_sh_s.unwrap_or_else(|| p.sh_time.value()),
            sh_lag_s: p.sh_lag.value(),
            auto_bend_time_s: p.auto_bend_time.value(),
            auto_bend_up: p.auto_bend_direction.value() == params::DirectionKind::Up,
            volume: p.volume.smoothed.next(),
        }
    }

    fn handle_event(&mut self, event: NoteEvent<()>) {
        match event {
            NoteEvent::NoteOn {
                voice_id,
                channel,
                key,
                velocity,
                ..
            } => {
                let (voice_id, channel, note) = legacy_note(voice_id, channel, key);
                if velocity <= 0.0 {
                    self.voice.note_off(voice_id, channel, note);
                } else {
                    self.voice.note_on(
                        NoteId {
                            voice_id,
                            channel,
                            key: note,
                        },
                        velocity,
                    );
                }
            }
            NoteEvent::NoteOff {
                voice_id,
                channel,
                key,
                ..
            } => {
                let (voice_id, channel, note) = legacy_note(voice_id, channel, key);
                self.voice.note_off(voice_id, channel, note);
            }
            NoteEvent::Choke {
                voice_id,
                channel,
                key,
                ..
            } => {
                let (voice_id, channel, note) = legacy_note(voice_id, channel, key);
                self.voice.choke(voice_id, channel, note);
            }
            NoteEvent::PolyTuning {
                voice_id,
                channel,
                key,
                tuning,
                ..
            } => {
                let (voice_id, channel, note) = legacy_note(voice_id, channel, key);
                self.voice.set_poly_tuning(voice_id, channel, note, tuning);
            }
            NoteEvent::MidiPitchBend { channel, value, .. } => {
                self.voice.set_pitch_bend(channel, 2.0 * (value - 0.5))
            }
            NoteEvent::MidiCC {
                channel, cc, value, ..
            } => match cc {
                DEV_VIEW_CC if self.dev_cc => self
                    .telemetry
                    .request_view((value.clamp(0.0, 1.0) * 127.0).round() as u8),
                DEV_DISCLOSURE_CC if self.dev_cc => self.telemetry.request_disclosure(value >= 0.5),
                DEV_BROWSER_CC if self.dev_cc => self.telemetry.request_browser(value >= 0.5),
                // A theme by index, 0 light / 1 dark / 2 system: applied to the editor and never
                // saved, so a capture run cannot rewrite the choice made in the app bar.
                DEV_THEME_CC if self.dev_cc => self
                    .telemetry
                    .request_theme((value.clamp(0.0, 1.0) * 127.0).round() as u8),
                // **New MIDI path.** CC 1 was accepted and ignored because the machine had no mod
                // wheel; it is kept now because a route can read it, at zero depth until one does.
                control_change::MODULATION_MSB => self.voice.set_mod_wheel(channel, value),
                11 => self.voice.set_expression(channel, value),
                control_change::ALL_SOUND_OFF => {
                    self.voice.all_sound_off();
                    self.was_inert = false;
                }
                control_change::ALL_NOTES_OFF => self.voice.all_notes_off(),
                _ => {}
            },
            // **New MIDI path.** Channel pressure is retained per channel whether or not a note
            // sounds, so a note started while a key is already leaned on inherits it.
            NoteEvent::MidiChannelPressure {
                channel, pressure, ..
            } => self.voice.set_channel_pressure(channel, pressure),
            // No brightness or other per-note destination exists.
            _ => {}
        }
    }

    fn current_process_status(&self) -> ProcessStatus {
        match self.voice.activity(&self.last_patch) {
            Activity::Live => ProcessStatus::KeepAlive,
            Activity::Tailing => {
                ProcessStatus::Tail(self.voice.tail_samples(&self.last_patch).max(1))
            }
            Activity::Inert => ProcessStatus::Normal,
        }
    }

    /// One sample through exactly the work [`Plugin::process`] does per sample.
    ///
    /// Resolve the topology this interval will run. **Once per interval, never per sample.**
    ///
    /// Presence decides the compacted live lists, so it is resolved here and the amounts are
    /// written per sample. Every path that can render owes this call: `mxm-creative-sampler`'s
    /// conversion shipped a voice that was never armed, so the first note after any allocation
    /// rendered with nothing routed.
    fn resolve_topology(&mut self) {
        // **The panel's own routing parameters, not the legacy mapping.** `topology_from` also
        // snaps every newly present route's smoother to its stored depth, so a pair switched on
        // arrives where the player left it rather than ramping from a stale number.
        let resolved = self.params.routes.topology_from(&self.routing);
        // **A topology change is a wake gesture** (§6). The inert shortcut skips the whole voice,
        // so a route that becomes able to sound while parked would otherwise not be noticed until
        // something else woke it, and a routed LFO would wake into a frozen phase.
        if resolved.present != self.routing.present {
            self.was_inert = false;
        }
        self.routing = resolved;
        self.voice.set_topology(&self.routing);
    }

    /// One sample of the instrument: **the per-sample path, shared**.
    ///
    /// `process` calls this rather than repeating it (`plans/plan-mxm-para-07-modulation.md` §10).
    /// It used to hold its own copy of the same four steps, which merely *agreed* with this one —
    /// and a per-sample amount advance added to one and not the other would have silently split the
    /// measured path from the host's. What `process` still owns is only block-rate work: the
    /// telemetry publish and the peak and overload accumulation.
    pub fn render_one(&mut self) -> f32 {
        let patch = self.next_patch();
        // **Each live route's own smoother, advanced exactly once.** An absent route's is left
        // where the player put it, which is what `TargetRoutes::arm` then resumes from.
        self.params.routes.advance(self.voice.routing_mut());
        let high = self.params.trigger_input.value();
        if high != self.trigger_high {
            self.voice.set_trigger_input(high);
            self.trigger_high = high;
            self.was_inert = false;
        }
        let sample = self.render_sample(&patch);
        self.last_patch = patch;
        sample
    }

    /// The sample and hold's scope, one sample's worth: every `scope_stride`th sample the voice ran,
    /// its source and output into the editor's ring. A parked voice writes nothing, so the scope
    /// holds still at rest rather than drawing a line of stale values.
    #[inline]
    fn feed_scope(&mut self, t: &mxm_para_07_dsp::voice::Telemetry) {
        if self.was_inert {
            return;
        }
        self.scope_countdown = self.scope_countdown.saturating_sub(1);
        if self.scope_countdown == 0 {
            self.scope_countdown = self.scope_stride;
            self.telemetry.push_scope(t.sh_source_value, t.sh_out);
        }
    }

    /// A block of them — the throughput probe C0 measures with.
    pub fn render_block_for_test(&mut self, out: &mut [f32]) {
        for slot in out.iter_mut() {
            *slot = self.render_one();
        }
    }

    #[inline]
    fn render_sample(&mut self, patch: &Patch) -> f32 {
        let key_changed =
            patch.key_mode != self.last_key_mode || patch.gate_source != self.last_gate_source;
        let hold_open = patch.hold > 0.0;
        let control_transition = key_changed || hold_open != self.last_hold_open;
        self.last_key_mode = patch.key_mode;
        self.last_gate_source = patch.gate_source;
        self.last_hold_open = hold_open;
        let inert = self.voice.activity(patch) == Activity::Inert;
        // One call on entry settles recursive state and exact silence. Thereafter an inert voice
        // advances no oscillator, filter, noise, LFO or S&H state until an event or deliberate
        // control transition requires its edge history to advance.
        let sample = if inert && self.was_inert && !control_transition {
            0.0
        } else {
            self.voice.process(patch)
        };
        self.was_inert = self.voice.activity(patch) == Activity::Inert;
        sample
    }
}

impl Plugin for MxmPara07 {
    const NAME: &'static str = crate::NAME;
    const VENDOR: &'static str = "mxm";
    const URL: &'static str = "https://mxm.dk";
    const EMAIL: &'static str = "plugins@mxm.dk";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: None,
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: None,
            main_output_channels: NonZeroU32::new(1),
            ..AudioIOLayout::const_default()
        },
    ];
    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
    // Trigger input derives an event from parameter edges, so its automation offsets are semantic.
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;
    type Editor = editor::MxmPara07Editor;
    type SysExMessage = ();
    type BackgroundTask = ();
    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }
    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Self::Editor> {
        editor::create(self.params.clone(), self.telemetry.clone())
    }
    fn activate(
        &mut self,
        _layout: &AudioIOLayout,
        config: &BufferConfig,
        _context: &mut impl ActivateContext<Self>,
    ) -> bool {
        // A new activation starts with no tempo and nothing resolved: the first callback reports
        // the tempo, so neither the audio nor an editor frame before it shows the last session's
        // divisions (`plans/plan-tempo-sync-controls.md`).
        self.telemetry.tempo.publish(None);
        self.synced_lfo_hz = None;
        self.synced_sh_s = None;
        if !Self::supports_sample_rate(config.sample_rate) {
            return false;
        }
        self.sample_rate = config.sample_rate;
        self.voice.set_sample_rate(self.sample_rate);
        self.voice.reset();
        self.trigger_high = self.params.trigger_input.value();
        self.voice.restore_trigger_input(self.trigger_high);
        self.telemetry.publish_sample_rate(self.sample_rate);
        self.scope_stride = (self.sample_rate / telemetry::SCOPE_RATE_HZ)
            .round()
            .max(1.0) as u32;
        self.scope_countdown = self.scope_stride;
        self.resolve_topology();
        self.was_inert = false;
        true
    }
    fn reset(&mut self) {
        self.voice.reset();
        self.trigger_high = self.params.trigger_input.value();
        self.voice.restore_trigger_input(self.trigger_high);
        self.resolve_topology();
        self.was_inert = false;
    }
    /// **A project saved before the tempo syncs** restores each Off rather than keeping this
    /// instance's, and a loaded preset's baseline gains it, so the preset stays clean
    /// (`mxm_preset::add_switches_off`).
    fn filter_state(state: &mut PluginState) {
        mxm_preset::add_switches_off(state, crate::preset::TEMPO_SYNC_IDS);
        params::retire_external_key_mode(state);
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let samples = buffer.samples();
        let mut next_event = context.next_event();
        let mut start = 0usize;
        // The LFO's and the sample clock's tempo syncs, once per callback, and the tempo in force
        // for the editor's readings.
        let tempo = context.transport().tempo;
        self.synced_lfo_hz = self.params.synced_lfo_rate(tempo);
        self.synced_sh_s = self.params.synced_sh_time(tempo);
        self.telemetry.tempo.publish(tempo);
        // Visualization work stops while no editor is open to show it.
        let scope = self.telemetry.editor_open();
        while start < samples {
            let mut end = (start + MAX_BLOCK_SIZE).min(samples);
            loop {
                match next_event {
                    Some(event) if (event.timing() as usize) <= start => {
                        self.handle_event(event);
                        next_event = context.next_event();
                    }
                    Some(event) if (event.timing() as usize) < end => {
                        end = event.timing() as usize;
                        break;
                    }
                    _ => break,
                }
            }
            // The interval's topology, resolved once here: presence changes only on a parameter
            // event, and the event loop above has already split this span at every one of them.
            self.resolve_topology();
            let output = buffer.as_slice();
            let mut peak = 0.0f32;
            let mut overloaded = false;
            for i in start..end {
                let sample = self.render_one();
                let t = self.voice.telemetry();
                if scope {
                    self.feed_scope(&t);
                }
                overloaded |= t.overload;
                peak = peak.max(sample.abs());
                for channel in output.iter_mut() {
                    channel[i] = sample;
                }
            }
            let mut t = self.voice.telemetry();
            t.overload |= overloaded;
            self.telemetry.publish_block(peak, t);
            start = end;
        }
        let status = self.current_process_status();
        // Whether a note sounds, for the displays that show where the voice is now: one relaxed
        // store a block.
        self.telemetry
            .publish_sounding(!matches!(status, ProcessStatus::Normal));
        status
    }
}

impl ClapPlugin for MxmPara07 {
    const CLAP_ID: &'static str = CLAP_ID;
    const CLAP_DESCRIPTION: Option<&'static str> = Some(
        "A paraphonic synthesizer with divider oscillators: two notes at once through one filter",
    );
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Synthesizer,
        ClapFeature::Stereo,
        ClapFeature::Mono,
    ];
}

nice_export_clap!(MxmPara07);

#[cfg(test)]
mod tests {
    use super::*;
    use nice_plug::prelude::Params;

    struct TestActivateContext;

    impl ActivateContext<MxmPara07> for TestActivateContext {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }

        fn execute(&self, _task: ()) {}

        fn set_latency_samples(&self, _samples: u32) {}

        fn set_current_voice_capacity(&self, _capacity: u32) {}
    }

    fn activate_at(plugin: &mut MxmPara07, sample_rate: f32) -> bool {
        plugin.activate(
            &MxmPara07::AUDIO_IO_LAYOUTS[0],
            &BufferConfig {
                sample_rate,
                min_buffer_size: Some(1),
                max_buffer_size: 64,
                process_mode: ProcessMode::Realtime,
            },
            &mut TestActivateContext,
        )
    }

    #[test]
    fn permanent_identity_and_bundle_name_agree() {
        assert_eq!(CLAP_ID, format!("dk.mxm.{NAME}"));
        mxm_plugin_test::bundle::is_named(env!("CARGO_MANIFEST_DIR"), env!("CARGO_PKG_NAME"), NAME);
    }
    #[test]
    fn parameter_ids_are_unique_and_the_editor_order_is_complete() {
        let p = MxmPara07Params::default();
        // A routing parameter is carried by the preset layer and drawn by its target's stack, not
        // listed here: this list is the editor's declaration order for the instrument's own panel.
        let declared: Vec<_> = p
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .filter(|id| !id.starts_with("mod_"))
            .collect();
        let listed = params::all_parameters(&p);
        // **Fifty-one: the eighty less the twenty-eight §5 retires, the two tempo syncs of
        // 2026-09-25, and less the three the external input took with it on 2026-09-26.** None
        // comes back -- a live id's range cannot widen and no retired id may be re-used in place
        // (governing §5.1).
        let expected: Vec<_> = "keymode gatesource triggerinput portamento portamentomode tune \
            vco1range vco1wave vco1width \
            vco2range vco2wave vco2width vco2tune sync \
            reg32 reg16 reg8 reg4 reg2 bank vco1 vco2 noise fifth noisecolour \
            hpf cutoff resonance \
            env1attack env1decay env1sustain env1release env1trigger \
            env2attack env2decay env2sustain env2release env2trigger \
            hold lfoshape lforate lfosync lfodelay lfokeytrigger \
            shsource shtime shsync shlag autobendtime autobenddirection \
            volume"
            .split_whitespace()
            .collect();
        assert_eq!(
            listed.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            expected
        );
        let mut unique: Vec<_> = listed.iter().map(|(id, _)| *id).collect();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), listed.len());
        assert_eq!(listed.len(), declared.len());
    }
    #[test]
    fn control_map_leaves_amp_roles_empty_because_either_envelope_can_be_routed() {
        let map = include_str!("../control-map.json");
        assert!(
            !map.contains("\"amp_env."),
            "a static Amp role would edit one envelope even when vcaenv selects the other"
        );
        assert!(
            map.contains("\"filter_env.attack\": \"env1attack\""),
            "Envelope 1's invariant filter route should remain mapped"
        );
        // A switch that became routes cannot claim its old role -- `mxm-mono-02` and `mxm-poly-06`
        // record the same three.
        for role in ["osc1.pwm_source", "osc2.pwm_source", "filter_env.polarity"] {
            // The *binding* form, not the bare name: the map explains in its own comment why these
            // three are unfilled, and a substring match would catch that explanation.
            assert!(
                !map.contains(&format!("\"{role}\":")),
                "{role} cannot be filled once the switch it named became routes"
            );
        }
        // **A role may name a route amount only where Init wires that route**, or its knob is dead
        // on a fresh instance.
        for (role, id) in [
            ("osc1.pwm_depth", "mod_vco1pw_lfotri"),
            ("osc2.pwm_depth", "mod_vco2pw_lfotri"),
            ("filter.key_track", "mod_cutoff_key"),
            ("lfo1.to_amp", "mod_amp_lfoc"),
        ] {
            assert!(
                map.contains(&format!("\"{role}\": \"{id}\"")),
                "{role} should name {id}"
            );
            let (t, s) = routes::ROUTE_IDS
                .iter()
                .enumerate()
                .find_map(|(t, row)| row.iter().position(|(a, _)| *a == id).map(|s| (t, s)))
                .unwrap_or_else(|| panic!("{id} is not a routing amount"));
            assert!(
                Routing::init().present[t][s],
                "{role} names {id}, which Init does not wire: a knob on it would do nothing"
            );
        }
    }

    #[test]
    fn init_obeys_amount_configuration_and_source_contracts() {
        let p = MxmPara07Params::default();
        for (name, value) in [
            ("portamento", p.portamento.value()),
            ("resonance", p.resonance.value()),
            ("hold", p.hold.value()),
            ("LFO delay", p.lfo_delay.value()),
            ("S&H lag", p.sh_lag.value()),
        ] {
            assert_eq!(value, 0.0, "{name} is an amount");
        }
        assert_eq!(p.vco1.value(), 0.7);
        assert_eq!(p.vco2.value(), 0.0);
        assert!(p.vco2_tune.value() > 0.0 && p.vco2_tune.value() < 0.1);
        assert!(p.cutoff.value() > 10_000.0 && p.cutoff.value() < 20_000.0);
        assert_eq!(p.key_mode.value(), params::KeyModeKind::TwoPitch);
        assert_eq!(p.gate_source.value(), params::GateSourceKind::Host);
    }
    #[test]
    fn activation_accepts_the_safe_rate_floor_and_rejects_rates_below_it() {
        let mut accepted = MxmPara07::default();
        assert!(activate_at(&mut accepted, MIN_SAMPLE_RATE));
        assert_eq!(accepted.sample_rate, MIN_SAMPLE_RATE);

        for unsupported in [
            MIN_SAMPLE_RATE - 0.001,
            1.0,
            0.0,
            -1.0,
            f32::NAN,
            f32::INFINITY,
        ] {
            let mut rejected = MxmPara07::default();
            assert!(!activate_at(&mut rejected, unsupported));
            assert_eq!(
                rejected.sample_rate, 48_000.0,
                "rejected activation must not alter processing state"
            );
        }
    }

    #[test]
    fn layouts_are_stereo_then_mono_with_no_audio_input_and_no_note_output() {
        assert_eq!(MxmPara07::AUDIO_IO_LAYOUTS.len(), 2);
        for (layout, channels) in MxmPara07::AUDIO_IO_LAYOUTS.iter().zip([2, 1]) {
            assert!(layout.main_input_channels.is_none());
            assert!(layout.aux_input_ports.is_empty());
            assert_eq!(layout.main_output_channels, NonZeroU32::new(channels));
        }
        assert_eq!(MxmPara07::MIDI_OUTPUT, MidiConfig::None);
    }
    #[test]
    fn midi_routes_extremes_tuning_bend_expression_and_terminations() {
        let mut p = MxmPara07::default();
        let on = |note, channel| NoteEvent::NoteOn {
            timing: 0,
            voice_id: VoiceID::Wildcard,
            channel: Channel::Number(channel),
            key: Key::Number(note),
            velocity: 1.0,
        };
        p.handle_event(on(48, 1));
        p.handle_event(on(72, 2));
        let a = p.voice.assignment();
        assert_eq!(a.high.unwrap().id.key, 72);
        assert_eq!(a.low.unwrap().id.key, 48);
        p.handle_event(NoteEvent::PolyTuning {
            timing: 0,
            voice_id: VoiceID::Wildcard,
            channel: Channel::Number(2),
            key: Key::Number(72),
            tuning: 0.5,
        });
        assert_eq!(p.voice.assignment().high.unwrap().tuning_semitones, 0.5);
        p.handle_event(NoteEvent::MidiPitchBend {
            timing: 0,
            channel: 2,
            value: 1.0,
        });
        p.handle_event(NoteEvent::MidiCC {
            timing: 0,
            channel: 2,
            cc: 11,
            value: 0.75,
        });
        p.handle_event(NoteEvent::MidiCC {
            timing: 0,
            channel: 0,
            cc: control_change::ALL_NOTES_OFF,
            value: 0.0,
        });
        p.handle_event(NoteEvent::MidiCC {
            timing: 0,
            channel: 0,
            cc: control_change::ALL_SOUND_OFF,
            value: 0.0,
        });
        assert_eq!(p.voice.activity(&Patch::default()), Activity::Inert);
    }
    #[test]
    fn the_developer_channel_is_off_unless_the_environment_asked_for_it() {
        let mut p = MxmPara07 {
            dev_cc: false,
            ..Default::default()
        };
        let cc = |n, value| NoteEvent::MidiCC {
            timing: 0,
            channel: 0,
            cc: n,
            value,
        };
        p.handle_event(cc(DEV_VIEW_CC, 2.0 / 127.0));
        p.handle_event(cc(DEV_THEME_CC, 1.0 / 127.0));
        assert_eq!(p.telemetry.take_view_request(), None);
        assert_eq!(p.telemetry.take_theme_request(), None);
        p.dev_cc = true;
        p.handle_event(cc(DEV_VIEW_CC, 2.0 / 127.0));
        p.handle_event(cc(DEV_DISCLOSURE_CC, 1.0));
        p.handle_event(cc(DEV_BROWSER_CC, 1.0));
        p.handle_event(cc(DEV_THEME_CC, 1.0 / 127.0));
        assert_eq!(p.telemetry.take_view_request(), Some(2));
        assert_eq!(p.telemetry.take_disclosure_request(), Some(true));
        assert_eq!(p.telemetry.take_browser_request(), Some(true));
        assert_eq!(
            p.telemetry.take_theme_request(),
            Some(1),
            "1 is dark, as mxm_ui::theme::from_index reads it"
        );
    }
    #[test]
    fn tail_status_uses_only_the_envelope_connected_to_the_amplifier() {
        let mut plugin = MxmPara07::default();
        let patch = Patch {
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
        plugin.voice.note_on(
            NoteId {
                voice_id: Some(1),
                channel: 0,
                key: 60,
            },
            0.0,
        );
        for _ in 0..2_000 {
            plugin.voice.process(&patch);
        }
        plugin.voice.note_off(Some(1), 0, 60);
        // The tail belongs to the envelope *routed* into the amplifier now, so an unarmed plugin
        // has no envelope there at all and would report no tail rather than a short one. Both
        // halves are needed: `resolve_topology` decides presence, and the per-sample amounts pass
        // gives the route a depth. A present route at zero depth carries nothing, by design.
        plugin.resolve_topology();
        plugin.params.routes.advance(plugin.voice.routing_mut());
        plugin.last_patch = patch;

        let ProcessStatus::Tail(samples) = plugin.current_process_status() else {
            panic!("the selected envelope should report its audible tail");
        };
        assert!(
            samples < 4_000,
            "the disconnected eight-second envelope extended the tail to {samples} samples"
        );
    }

    #[test]
    fn parked_panic_observes_hold_close_before_reopen_wakes_it() {
        let mut plugin = MxmPara07::default();
        let mut patch = Patch {
            hold: 1.0,
            ..Patch::default()
        };
        plugin.render_sample(&patch);
        assert_eq!(plugin.voice.activity(&patch), Activity::Live);

        plugin.handle_event(NoteEvent::MidiCC {
            timing: 0,
            channel: 0,
            cc: control_change::ALL_SOUND_OFF,
            value: 0.0,
        });
        assert_eq!(plugin.render_sample(&patch), 0.0);
        assert_eq!(plugin.voice.activity(&patch), Activity::Inert);

        patch.hold = 0.0;
        assert_eq!(plugin.render_sample(&patch), 0.0);
        assert_eq!(plugin.voice.activity(&patch), Activity::Inert);

        patch.hold = 1.0;
        plugin.render_sample(&patch);
        assert_eq!(
            plugin.voice.activity(&patch),
            Activity::Live,
            "closing HOLD while parked must reach Voice before reopening it"
        );
    }

    /// **The scope takes a point a millisecond while the voice runs, and none while it is parked**,
    /// so at rest it holds the last thing played rather than scrolling a flat line.
    #[test]
    fn the_scope_fills_while_the_voice_runs_and_holds_at_rest() {
        let mut p = MxmPara07::default();
        let (mut source, mut out) = (Vec::new(), Vec::new());
        let run = |p: &mut MxmPara07, samples: usize| {
            for _ in 0..samples {
                p.render_one();
                let t = p.voice.telemetry();
                p.feed_scope(&t);
            }
        };
        run(&mut p, 480);
        p.telemetry.scope_recent(usize::MAX, &mut source, &mut out);
        assert!(
            source.is_empty(),
            "a parked voice wrote {} points",
            source.len()
        );

        p.voice.note_on(
            NoteId {
                voice_id: None,
                channel: 0,
                key: 60,
            },
            0.0,
        );
        p.was_inert = false;
        run(&mut p, 480);
        p.telemetry.scope_recent(usize::MAX, &mut source, &mut out);
        assert_eq!(
            source.len(),
            10,
            "480 samples at 48 kHz are ten milliseconds"
        );
    }

    #[test]
    fn inert_processing_is_parked_and_wake_events_resume_it() {
        let mut p = MxmPara07::default();
        let patch = Patch::default();
        assert_eq!(p.render_sample(&patch), 0.0);
        assert!(p.was_inert);
        p.voice.note_on(
            NoteId {
                voice_id: None,
                channel: 0,
                key: 60,
            },
            0.0,
        );
        p.was_inert = false;
        assert!(p.render_sample(&patch).is_finite());
        assert_ne!(p.voice.activity(&patch), Activity::Inert);
        p.voice.all_sound_off();
        p.was_inert = false;
        assert_eq!(p.render_sample(&patch), 0.0);
        assert!(p.was_inert);
    }
}

/// **C0: the pre-conversion reference** (`plans/plan-mxm-para-07-modulation.md` §10).
///
/// Captured before a line of routing exists, through the plugin's own per-sample path. Everything
/// here is `#[ignore]`d: these are measurements and dumps, not assertions, and the figures count
/// only from a quiet machine.
///
/// **What this reference is blind to, by necessity.** Velocity, the wheel and pressure are the three
/// MIDI paths the conversion *adds* (§3.1): today `handle_event` ignores velocity, accepts and
/// discards CC 1, and has no pressure path at all, so no score can exercise them here. They have no
/// before to be identical to, and C1 introduces them. Everything the machine *does* have is driven —
/// both pitch lanes, the pedal and the lever — because a reference blind to a path it exists to
/// protect is worse than none: `mxm-mono-08`'s M0 rendered six sounds byte-identically to one
/// another until its score drove the external input. **This score drove the external port too until
/// the owner removed it on 2026-09-26**; at every factory sound's settings it reached only the
/// follower, whose route sat at zero, so every digest held without it.
#[cfg(test)]
mod baseline {
    use super::*;
    use std::time::Instant;

    const FS: f32 = 48_000.0;
    /// MXM Player's own block, so this score's block counts line up with the host golden's 72.
    const BLOCK: usize = 512;

    /// A plugin with every smoother activated and the voice armed, exactly as `activate` leaves it
    /// (mxm-kit's `docs/adding-an-instrument.md` gotcha 13).
    pub(crate) fn plugin() -> MxmPara07 {
        let mut plugin = MxmPara07::default();
        for (_, ptr, _) in plugin.params.param_map() {
            unsafe { ptr._internal_update_smoother(FS, true) };
        }
        plugin.sample_rate = FS;
        plugin.voice.set_sample_rate(FS);
        plugin.voice.reset();
        plugin.trigger_high = plugin.params.trigger_input.value();
        plugin.voice.restore_trigger_input(plugin.trigger_high);
        // **Arm the topology, exactly as `activate` does.** This helper mirrors `activate` field by
        // field rather than calling it, so every line `activate` gains has to be repeated here or
        // the harness renders an unarmed voice: every route absent, all modulation silently zero.
        // `mxm-mono-08`'s baseline shipped that fault at B1 and rendered silence.
        plugin.resolve_topology();
        plugin.was_inert = false;
        plugin
    }

    /// Writes one normalised value by permanent id, as a host automation write would.
    pub(crate) fn set(plugin: &MxmPara07, id: &str, normalised: f32) {
        for (candidate, ptr, _) in plugin.params.param_map() {
            if candidate == id {
                unsafe { ptr._internal_set_normalized_value(normalised) };
                return;
            }
        }
        panic!("no parameter {id}");
    }

    /// Snaps every smoother, as a settled load would.
    pub(crate) fn settle(plugin: &MxmPara07) {
        for (_, ptr, _) in plugin.params.param_map() {
            unsafe { ptr._internal_update_smoother(FS, true) };
        }
    }

    /// Applies a factory file's stored values by permanent id — **only `v` is read**, as the preset
    /// system reads it — then snaps every smoother, as a fresh load would settle.
    fn apply(plugin: &MxmPara07, json: &str) -> usize {
        let designed = mxm_preset::Preset::parse(json, CLAP_ID).expect("a factory file parses");
        let map = plugin.params.param_map();
        let mut applied = 0;
        for (id, ptr, _) in &map {
            if let Some(value) = designed.params.get(id) {
                unsafe { ptr._internal_set_normalized_value(value.v) };
                applied += 1;
            }
        }
        for (_, ptr, _) in &map {
            unsafe { ptr._internal_update_smoother(FS, true) };
        }
        applied
    }

    fn slug(name: &str) -> String {
        name.to_lowercase().replace(' ', "-")
    }

    /// A factory sound by its file's slug, so no display-name spelling is guessed at.
    fn factory(want: &str) -> (&'static str, &'static str) {
        crate::preset::FACTORY_FILES
            .iter()
            .find(|(name, _)| slug(name) == want)
            .map(|&(name, text)| (name, text))
            .unwrap_or_else(|| panic!("no factory sound with slug {want:?}"))
    }

    fn note_on(plugin: &mut MxmPara07, note: u8) {
        plugin.handle_event(NoteEvent::NoteOn {
            timing: 0,
            voice_id: VoiceID::Wildcard,
            channel: Channel::Number(0),
            key: Key::Number(note),
            velocity: 1.0,
        });
    }

    fn note_off(plugin: &mut MxmPara07, note: u8) {
        plugin.handle_event(NoteEvent::NoteOff {
            timing: 0,
            voice_id: VoiceID::Wildcard,
            channel: Channel::Number(0),
            key: Key::Number(note),
            velocity: 0.0,
        });
    }

    fn run(plugin: &mut MxmPara07, out: &mut Vec<f32>, blocks: usize) {
        for _ in 0..blocks {
            for _ in 0..BLOCK {
                out.push(plugin.render_one());
            }
        }
    }

    /// **The score every sound plays, fixed forever.** Seventy-two blocks, the host golden's own
    /// shape (`plugins/mxm-para-07/host-tests/tests/golden_audio.rs`), so the two measure the same
    /// gestures: a low press, a high press above it, a middle press that moves the shared gate but
    /// neither pitch, then the extremes released in the order that collapses ownership.
    ///
    /// **Plus the two performance paths this machine already has** — the pedal (CC 11) and the
    /// lever — which the host golden does not send. Without them, `keysource` in Pedal mode and
    /// every bender route render exactly as their Off positions do.
    fn render_score(plugin: &mut MxmPara07) -> Vec<f32> {
        // **Arm the topology from the patch this sound actually holds.** `plugin()` armed from the
        // default panel, and every caller sets its parameters afterwards; without this the
        // topology is resolved from a panel that no longer exists by the time it renders. It shows
        // up only where a *selector* changes presence — the bender — because everything else moves
        // amounts on routes Init already wires, which is why the bank looked fine while all four
        // bender oracles rendered exactly Init's peak.
        plugin.resolve_topology();
        let mut out = Vec::with_capacity(72 * BLOCK);
        run(plugin, &mut out, 2);
        note_on(plugin, 48);
        run(plugin, &mut out, 10);
        note_on(plugin, 67);
        run(plugin, &mut out, 8);
        plugin.handle_event(NoteEvent::MidiCC {
            timing: 0,
            channel: 0,
            cc: 11,
            value: 0.7,
        });
        run(plugin, &mut out, 6);
        note_on(plugin, 57);
        run(plugin, &mut out, 4);
        plugin.handle_event(NoteEvent::MidiPitchBend {
            timing: 0,
            channel: 0,
            value: 0.75,
        });
        run(plugin, &mut out, 4);
        plugin.handle_event(NoteEvent::MidiPitchBend {
            timing: 0,
            channel: 0,
            value: 0.5,
        });
        note_off(plugin, 67);
        run(plugin, &mut out, 10);
        note_off(plugin, 57);
        run(plugin, &mut out, 6);
        note_off(plugin, 48);
        run(plugin, &mut out, 22);
        out
    }

    /// FNV-1a over the raw bits, the digest the player's golden computes.
    fn digest(samples: &[f32]) -> String {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for sample in samples {
            for byte in sample.to_bits().to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        format!("{hash:016x}")
    }

    /// Init first, then the fifty in shipped order.
    fn bank() -> Vec<(&'static str, Vec<f32>, usize)> {
        let mut out = Vec::new();
        let mut init = plugin();
        out.push(("Init", render_score(&mut init), 0));
        for (name, json) in crate::preset::FACTORY_FILES {
            let mut p = plugin();
            let applied = apply(&p, json);
            out.push((name, render_score(&mut p), applied));
        }
        out
    }

    fn dump_dir() -> Option<std::path::PathBuf> {
        std::env::var_os("MXM_C0_DUMP").map(std::path::PathBuf::from)
    }

    fn dump(dir: &Option<std::path::PathBuf>, name: &str, samples: &[f32]) {
        if let Some(dir) = dir {
            let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
            std::fs::write(dir.join(format!("{name}.f32")), bytes).expect("dump");
        }
    }

    /// A digest and a peak for Init and every factory sound; with `MXM_C0_DUMP` set, the renders too.
    #[test]
    #[ignore = "a measurement, not an assertion; release only"]
    fn the_bank_digests() {
        let dir = dump_dir();
        println!();
        for (name, samples, applied) in bank() {
            let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            println!(
                "  | `{}` | `{}` | {peak:.4} | {applied} |",
                slug(name),
                digest(&samples)
            );
            dump(&dir, &slug(name), &samples);
        }
        println!();
    }

    /// Sample-by-sample difference from the renders in `MXM_C0_DUMP`: the worst absolute
    /// difference, and the worst relative to that sound's own peak.
    ///
    /// **A digest says only *different*; it cannot report a tolerance**, and §11 asks for one —
    /// *"factory bank sounds like itself: C0 per-preset digests, re-association tolerance
    /// reported"*. Cutoff is where bit-identity is expected to end: its envelope term folds
    /// `FILTER_ENV_OCTAVES × FILTER_ENV_WEIGHT` into one scale (§3.3), and its routed sum runs in
    /// source order where the legacy expression accumulated env, mod, tracking, audio, follower and
    /// bend in its own. So the dumps have to be taken **before** that step, against a tree still
    /// identical to C0, or the figure can never be produced.
    #[test]
    #[ignore = "a comparison against a local dump, not an assertion"]
    fn the_bank_against_the_c0_dump() {
        let Some(dir) = dump_dir() else {
            println!("set MXM_C0_DUMP to the directory `the_bank_digests` wrote");
            return;
        };
        println!();
        let mut worst_overall = 0.0f32;
        for (name, samples, _) in bank() {
            let path = dir.join(format!("{}.f32", slug(name)));
            let Ok(bytes) = std::fs::read(&path) else {
                println!("  | `{}` | no dump |", slug(name));
                continue;
            };
            let before: Vec<f32> = bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| f32::from_le_bytes(*c))
                .collect();
            assert_eq!(
                before.len(),
                samples.len(),
                "{} was dumped at a different length",
                slug(name)
            );
            let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            let worst = before
                .iter()
                .zip(&samples)
                .fold(0.0f32, |m, (a, b)| m.max((a - b).abs()));
            worst_overall = worst_overall.max(worst);
            let relative = if peak > 0.0 { worst / peak } else { 0.0 };
            println!("  | `{}` | {worst:.3e} | {relative:.3e} |", slug(name));
        }
        println!("\n  worst absolute difference across the bank: {worst_overall:.3e}\n");
    }

    /// One focused case: its name, the permanent id carrying its depth, the normalised values for
    /// partial and full, and the companions that choose that path.
    ///
    /// **The depth pair is explicit rather than computed.** A legacy depth ran 0…1, so normalised
    /// 0.5 was half of it; a route amount is signed, so normalised 0.5 is *zero* and a naive port
    /// would have rendered no modulation at all while still looking like a measurement. The
    /// inverted-polarity case is a negative amount, and `amp-hold` is not a route at all — HOLD is
    /// the amplifier target's own base — so no single formula covers them.
    type FocusedDepth = (
        &'static str,
        &'static str,
        (f32, f32),
        &'static [(&'static str, f32)],
    );

    /// **Every row of §1 that carries a depth**, as the route that carries it and the presences that
    /// choose it. Each is rendered at partial and full depth.
    const FOCUSED_DEPTHS: &[FocusedDepth] = &[
        ("vco1-pitch-lfo", "mod_vco1pitch_lfo", (0.75, 1.0), &[]),
        // **VCO-2 is silent at Init** (`vco2` defaults to 0.0), so without raising its mixer level
        // this case rendered Init's own digest at both depths and measured nothing at all.
        (
            "vco2-pitch-lfo",
            "mod_vco2pitch_lfo",
            (0.75, 1.0),
            &[("vco2", 0.7)],
        ),
        // Present by hand since Init stopped wiring them (the owner, 2026-09-27).
        (
            "vco1-pitch-sh",
            "mod_vco1pitch_sh",
            (0.75, 1.0),
            &[("mod_vco1pitch_shon", 1.0)],
        ),
        (
            "vco1-pitch-autobend",
            "mod_vco1pitch_auto",
            (0.75, 1.0),
            &[("autobendtime", 0.5), ("mod_vco1pitch_autoon", 1.0)],
        ),
        // The bender's two modes read different sources, which is why they are separate pairs.
        (
            "pitch-bend-direct",
            "mod_vco1pitch_bend",
            (0.75, 1.0),
            &[("mod_vco1pitch_bendon", 1.0)],
        ),
        (
            "pitch-bend-lfo",
            "mod_vco1pitch_mult",
            (0.75, 1.0),
            &[("mod_vco1pitch_multon", 1.0)],
        ),
        (
            "pw-lfo-triangle",
            "mod_vco1pw_lfotri",
            (0.75, 1.0),
            &[("vco1wave", 1.0)],
        ),
        (
            "pw-env1",
            "mod_vco1pw_env1",
            (0.75, 1.0),
            &[("mod_vco1pw_env1on", 1.0), ("vco1wave", 1.0)],
        ),
        // **Every cutoff case opens the filter part-way first.** Init's cutoff is 18 kHz against a
        // ceiling of `min(20 kHz, 0.45 × fs)` — about 0.152 octaves of headroom, less than any one
        // of these terms asks for: the bend reaches two octaves and the envelope four. Left at
        // Init an oracle measures the clamp rather than the term.
        (
            "cutoff-env1-positive",
            "mod_cutoff_env1",
            (0.75, 1.0),
            &[("cutoff", 0.5)],
        ),
        // **The retired polarity switch is the amount's sign now** (D6), so inverted is negative.
        (
            "cutoff-env1-inverted",
            "mod_cutoff_env1",
            (0.25, 0.0),
            &[("cutoff", 0.5)],
        ),
        (
            "cutoff-lfo",
            "mod_cutoff_lfo",
            (0.75, 1.0),
            &[("cutoff", 0.5)],
        ),
        (
            "cutoff-sh",
            "mod_cutoff_sh",
            (0.75, 1.0),
            &[
                ("cutoff", 0.5),
                ("mod_cutoff_shon", 1.0),
                ("mod_cutoff_lfoon", 0.0),
            ],
        ),
        (
            "cutoff-key",
            "mod_cutoff_key",
            (0.75, 1.0),
            &[("cutoff", 0.5)],
        ),
        // **Pedal mode was two routes, not one**: the manual's fixed 1 V/oct key tracking stays at
        // full beside the pedal at the amount.
        (
            "cutoff-pedal",
            "mod_cutoff_pedal",
            (0.75, 1.0),
            &[
                ("cutoff", 0.5),
                ("mod_cutoff_pedalon", 1.0),
                ("mod_cutoff_key", 1.0),
            ],
        ),
        (
            "cutoff-vco2-audio",
            "mod_cutoff_vco2",
            (0.75, 1.0),
            &[("cutoff", 0.5), ("vco2", 0.7)],
        ),
        (
            "cutoff-noise-audio",
            "mod_cutoff_noise",
            (0.75, 1.0),
            &[
                ("cutoff", 0.5),
                ("mod_cutoff_noiseon", 1.0),
                ("mod_cutoff_vco2on", 0.0),
                ("noise", 0.7),
            ],
        ),
        (
            "cutoff-bend-direct",
            "mod_cutoff_bend",
            (0.75, 1.0),
            &[("cutoff", 0.5), ("mod_cutoff_bendon", 1.0)],
        ),
        (
            "cutoff-bend-lfo",
            "mod_cutoff_mult",
            (0.75, 1.0),
            &[("cutoff", 0.5), ("mod_cutoff_multon", 1.0)],
        ),
        // HOLD is the amplifier target's own base, not a route, so its 0…1 travel is unchanged.
        ("amp-hold", "hold", (0.5, 1.0), &[]),
        ("amp-vca-lfo", "mod_amp_lfoc", (0.75, 1.0), &[]),
        (
            "amp-bend-direct",
            "mod_amp_bend",
            (0.75, 1.0),
            &[("mod_amp_bendon", 1.0)],
        ),
        (
            "amp-bend-lfo",
            "mod_amp_mult",
            (0.75, 1.0),
            &[("mod_amp_multon", 1.0)],
        ),
    ];

    /// §1's one row that is a connection rather than a depth: which envelope reaches the VCA. It is
    /// a pair of presences now, where it used to be a two-way selector.
    const FOCUSED_SWITCHES: &[(&str, &[(&str, f32)])] = &[(
        "amp-env2",
        &[
            ("mod_amp_env1on", 0.0),
            ("mod_amp_env2on", 1.0),
            ("env2decay", 0.6),
        ],
    )];

    /// **Each §1 row at partial and full depth**, digested and — with `MXM_C0_DUMP` — dumped.
    ///
    /// These are the focused per-path oracles §11 compares the converted single-term paths against.
    /// Taken from a `HEAD` worktree, they avoid keeping the old path alive beside the new
    /// (`plan-mxm-mono-00-modulation.md`'s A0 shape).
    #[test]
    #[ignore = "a measurement, not an assertion; release only"]
    fn the_focused_paths() {
        let dir = dump_dir();
        println!();
        let init = digest(&render_score(&mut plugin()));
        let mut depths: Vec<(String, String)> = Vec::new();
        let mut switches: Vec<(String, String)> = Vec::new();
        for (case, depth_id, (partial, full), companions) in FOCUSED_DEPTHS {
            for (label, depth) in [("partial", *partial), ("full", *full)] {
                let mut p = plugin();
                for (id, value) in *companions {
                    set(&p, id, *value);
                }
                set(&p, depth_id, depth);
                settle(&p);
                let samples = render_score(&mut p);
                let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
                let name = format!("{case}-{label}");
                let hash = digest(&samples);
                println!("  | `{name}` | `{hash}` | {peak:.4} |");
                dump(&dir, &name, &samples);
                depths.push((name, hash));
            }
        }
        for (case, values) in FOCUSED_SWITCHES {
            let mut p = plugin();
            for (id, value) in *values {
                set(&p, id, *value);
            }
            settle(&p);
            let samples = render_score(&mut p);
            let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            let hash = digest(&samples);
            println!("  | `{case}` | `{hash}` | {peak:.4} |");
            dump(&dir, case, &samples);
            switches.push(((*case).to_owned(), hash));
        }
        println!();

        // **The oracles falsify themselves.** Twice while building this reference a case looked
        // like data while measuring nothing: `vco2-pitch-lfo` rendered Init's own digest, because
        // VCO-2's mixer level is zero at Init and modulating a silent oscillator is inaudible; and
        // `cutoff-bend-direct` hashed identically at both depths, because Init's 18 kHz cutoff
        // leaves 0.152 octaves of headroom where that term asks for two, so both depths pinned the
        // filter at its ceiling. A case that does not move is not a reference, and eyeballing a
        // hundred digests is not a safeguard.
        for (name, hash) in depths.iter().chain(switches.iter()) {
            assert_ne!(
                *hash, init,
                "{name} renders exactly Init, so it measures nothing"
            );
        }
        for pair in depths.chunks(2) {
            assert_ne!(
                pair[0].1, pair[1].1,
                "{} and {} render alike, so the depth reaches nothing",
                pair[0].0, pair[1].0
            );
        }
    }

    fn throughput(label: &str, plugin: &mut MxmPara07) {
        // The caller applies a preset after `plugin()`, so the topology is resolved here for the
        // same reason `render_score` resolves it.
        plugin.resolve_topology();
        note_on(plugin, 48);
        note_on(plugin, 67);
        let mut out = [0.0f32; 256];
        // Warm the caches: the first blocks pay for page faults, which is not the question.
        for _ in 0..64 {
            plugin.render_block_for_test(&mut out);
        }
        let blocks = 4_000;
        let start = Instant::now();
        for _ in 0..blocks {
            plugin.render_block_for_test(&mut out);
        }
        let taken = start.elapsed().as_secs_f64();
        let samples = (blocks * out.len()) as f64;
        println!(
            "  mxm-para-07, {label}, two held keys: {:.3} ns/sample ({:.0} samples/s)",
            taken * 1e9 / samples,
            samples / taken
        );
    }

    /// Per-sample cost through the plugin's own path, three times each so the spread is visible.
    /// **A figure counts only from a quiet machine.**
    ///
    /// **Both source paths, because they differ eightfold.** `render_sources` renders 1× for linear
    /// waveshapes with the ring silent and 8× for a triangle, a pulse or an active ring
    /// (`voice.rs:870-880`). Init is the 1× case — saw and square, and `fifth` at zero leaves
    /// `ring_active` false — so the 8× case is Init with a pulse VCO-1. An 8× render dwarfs a
    /// routing sum, so a regression measured only there would be invisible.
    ///
    /// **Two routed patches, where §10 named one.** §10 asks for "a factory preset using filter
    /// audio modulation and LFO pitch"; **no shipped preset uses both** — `filteraudio` is non-zero
    /// only in `audio-contour` and `filter-chatter`, and LFO pitch only in `delayed-vibrato`,
    /// `glide-lead`, `moving-pad`, `pulse-motion` and `slow-strings`, and the two sets are disjoint.
    /// Holding one of each serves the intent, costs one more reading, and keeps both patches shipped
    /// rather than invented.
    #[test]
    #[ignore = "a measurement, not an assertion; release only"]
    fn throughput_of_both_source_paths_and_the_routed_patches() {
        println!();
        for pass in 1..=3 {
            throughput(
                &format!("pass {pass}, Init, 1x source render"),
                &mut plugin(),
            );

            let mut eight = plugin();
            // Pulse is a nonlinear waveshape, so `render_sources` takes the 8x path.
            set(&eight, "vco1wave", 1.0);
            settle(&eight);
            throughput(
                &format!("pass {pass}, Init with a pulse VCO-1, 8x source render"),
                &mut eight,
            );

            for want in ["audio-contour", "delayed-vibrato"] {
                let (name, json) = factory(want);
                let mut p = plugin();
                apply(&p, json);
                throughput(&format!("pass {pass}, {name}"), &mut p);
            }
        }
        println!();
    }
}

/// **A synced value reaches the patch** (`plans/plan-tempo-sync-controls.md`): what a sync resolved
/// for this callback is what the DSP is given, and with none the free value is.
#[cfg(test)]
mod tempo_sync_path {
    use super::*;

    #[test]
    fn the_synced_lfo_rate_and_sample_time_are_the_patchs() {
        let mut plugin = MxmPara07::default();
        let free = plugin.next_patch();
        plugin.synced_lfo_hz = Some(free.lfo_rate_hz + 1.0);
        plugin.synced_sh_s = Some(free.sh_sample_time_s + 0.1);
        let synced = plugin.next_patch();
        assert_eq!(synced.lfo_rate_hz, free.lfo_rate_hz + 1.0);
        assert_eq!(synced.sh_sample_time_s, free.sh_sample_time_s + 0.1);
    }
}

/// **Activation forgets the last session's tempo and resolved syncs**: the first callback reports the
/// tempo, so nothing — the audio, or an editor frame before it — starts from the previous session's
/// divisions.
#[cfg(test)]
mod activation_forgets_the_tempo {
    use super::*;

    #[test]
    fn activation_forgets_the_last_tempo_and_resolved_syncs() {
        use nice_plug::prelude::Plugin as _;
        let mut plugin = MxmPara07::default();
        plugin.telemetry.tempo.publish(Some(120.0));
        plugin.synced_lfo_hz = Some(1.0);
        plugin.synced_sh_s = Some(0.5);
        let layout = MxmPara07::AUDIO_IO_LAYOUTS[0];
        let config = BufferConfig {
            sample_rate: 48_000.0,
            min_buffer_size: None,
            max_buffer_size: 512,
            process_mode: ProcessMode::Realtime,
        };
        let _ = plugin.activate(&layout, &config, &mut NoInit);
        assert_eq!(plugin.telemetry.tempo.get(), None);
        assert_eq!(plugin.synced_lfo_hz, None);
        assert_eq!(plugin.synced_sh_s, None);
    }

    /// An activation context that asks nothing of a host.
    struct NoInit;

    impl ActivateContext<MxmPara07> for NoInit {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        fn execute(&self, _task: <MxmPara07 as Plugin>::BackgroundTask) {}
        fn set_latency_samples(&self, _samples: u32) {}
        fn set_current_voice_capacity(&self, _capacity: u32) {}
    }
}

/// What a player reads — on hover in the editor, and in a host's plugin browser — speaks to the
/// player about the sound, never about the machine or the code (`mxm_plugin_test::hover_text`).
#[cfg(test)]
mod speaks_to_the_player {
    #[test]
    fn hover_text() {
        mxm_plugin_test::hover_text::speaks_to_the_player(env!("CARGO_MANIFEST_DIR"));
    }

    #[test]
    fn host_description() {
        mxm_plugin_test::hover_text::host_description_speaks_to_the_player(env!(
            "CARGO_MANIFEST_DIR"
        ));
    }
}
