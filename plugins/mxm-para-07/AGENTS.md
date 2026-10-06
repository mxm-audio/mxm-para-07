# AGENTS.md — plugins/mxm-para-07

Parent: [`../AGENTS.md`](../AGENTS.md)

# Purpose

The nice-plug shell for `mxm-para-07`: permanent identity, parameters, MIDI/events, state, fifty
factory presets, realtime processing, atomic telemetry, the dynamically paged editor and the
instrument-owned control map over
[`crates/mxm-para-07-dsp`](../../crates/mxm-para-07-dsp/AGENTS.md). Architecture inspired by the
Roland SH-7; that maker/model is reference text, never product, parameter, mode or preset identity.

Fidelity remains UNVERIFIED; status, history, measurements and the owner's rulings behind each rule
below are in [NOTES.md](NOTES.md#status).

# Ownership

Owns `BASELINE-C0.md`, `Cargo.toml`, `README.md`, `control-map.json`, `presets/`, `src/` and
`host-tests/`; its licence is the repository's root `LICENSE` (there is no per-plugin `LICENSE`
since the split, 2026-10-06). DSP remains in the framework-free sibling crate; shared preset
behaviour remains in mxm-kit's `mxm-preset`.

- **`BASELINE-C0.md` is the routing conversion's reference** (digests, focused oracles and
  throughput on both source paths), produced by `lib.rs`'s `#[ignore]`d `baseline` module.
- **`render_one` is the per-sample path and `process` calls it**; never give `process` its own copy.
  `process` owns block-rate work only (telemetry publish, peak and overload accumulation).
- `the_focused_paths` keeps every oracle distinct from Init and from itself across depths
  ([NOTES.md § BASELINE-C0.md](NOTES.md#baseline-c0md)).

# Local Contracts

## Permanent interface

- `CLAP_ID`: `dk.mxm.mxm-para-07`, derived from the one `plugin_name!` literal.
- Parameter ids, grouped in declaration order:
  - Voice: `keymode` `gatesource` `triggerinput` `portamento` `portamentomode` `tune`.
  - Oscillators: `vco1range` `vco1wave` `vco1width`; the same `vco2*` set plus `vco2tune`; `sync`.
  - Sources: `reg32` `reg16` `reg8` `reg4` `reg2` `bank` `vco1` `vco2` `noise` `fifth`
    `noisecolour`. `fifth`, painted *Ring*, is the ring modulator's level: VCO-1 times VCO-2.
  - Filter: `hpf` `cutoff` `resonance`.
  - Envelopes/amplifier: `env1attack` `env1decay` `env1sustain` `env1release` `env1trigger`,
    the matching `env2*` ids, `hold`.
  - Modulation/performance: `lfoshape` `lforate` `lfosync` `lfodelay` `lfokeytrigger` `shsource`
    `shtime` `shsync` `shlag` `autobendtime` `autobenddirection` `volume`.
- **Two tempo syncs**: `lfosync` (`params::LFO_SYNC`) and `shsync` (`params::SH_SYNC`), resolved once
  a callback by `synced_lfo_rate` and `synced_sh_time`. `sections::synced_row` keeps a knob, its
  quarter note and what follows in one flat row; never nest it.
- **Routing pairs**: one presence and one amount per *(target, source)*, **161 pairs, 322 ids**,
  `mod_<target>_<source>` and `mod_<target>_<source>on`. `mod_amp_*` is shown as **VCA level**;
  `mod_amplitude_*` is the standard Amplitude; `mod_bank_*` is Bank level. **VCA level ← Key is
  refused**, so `TargetRoutes` writes its `Params` impl by hand (the derive's ids and order, minus
  that pair). `ROUTE_IDS` writes the ids out and a test holds it to what is registered.
- **Route readings are the shared `mxm_modulation_params::reading`**;
  `every_route_parameter_says_what_the_dsp_does` holds each pair to
  `mxm_para_07_dsp::conformance::Declared`.
- **Forty-seven retired ids, never to be re-used**, all in `params::RETIRED_IDS`
  (`no_retired_id_is_back`): twenty-eight from the routing conversion, seventeen with the external
  input, and `mod_amp_key` `mod_amp_keyon`. Sessions and presets naming them still load, skipping
  them. The full lists: [NOTES.md § Permanent interface](NOTES.md#permanent-interface).
- **`keymode` has two values**, *Two-pitch* and *One-pitch*; the variant id `external` is retired
  and never re-used. A session that chose it restores as One-pitch
  (`params::retire_external_key_mode`, in `filter_state`).

Never rename or reuse an id. Tests compare the derive's complete set with `all_parameters`.

## Host and event behaviour

Detail: [NOTES.md § Host and event behaviour](NOTES.md#host-and-event-behaviour).

- Audio layouts are stereo then mono, **no audio input of any kind and no note output**; stereo
  duplicates one post-volume sample. Host tests require exactly those two and an empty event sink.
- A zero velocity is a NoteOff. **The Velocity source is `v − 1` of the press that last triggered an
  envelope.** NoteOff, Choke and PolyTuning keep the DSP's ID-authoritative/oldest-match semantics.
- Pitch bend is per channel through the high owner's lever routes; **Init wires Pitch ← Bend at
  ±2 st on both VCOs**. CC 11 is the pedal voltage; CC 1 (wheel) and channel pressure are retained
  per channel whether or not a note sounds, as routable sources only. CC 120 is immediate latched
  panic; CC 123 is ordinary release.
- `Trigger input` emits only a low-to-high edge. Activation, reset and state restoration adopt its
  current level without manufacturing a trigger. Sample-accurate automation is enabled because the
  parameter event's offset determines that edge; a low-high-low pulse inside one host block is kept.
- Events split processing at their sample offset and event-free spans are capped at 64 samples.
- Activation accepts only finite sample rates at or above the DSP's 12 Hz safety floor. Lower or
  non-finite host configurations are refused before any processing state is changed.
- **Parameter text is idempotent through the host's normalised conversion**
  (`params::tests::every_parameter_text_is_idempotent_through_the_hosts_conversion`); a time picks
  `ms` or `s` from its rounded milliseconds.

## Init, state, activity and telemetry

Detail: [NOTES.md § Init, state, activity and telemetry](NOTES.md#init-state-activity-and-telemetry).

- Init equals the CLAP defaults: one 8' saw at 0.7, every depth, resonance, glide, delay, HOLD,
  trigger level and bend amount at zero except the ±2 st Pitch ← Bend routes. **Each Pitch stack
  opens with LFO and Bend only**; factory sounds that use S&H or Auto bend depth set the presence.
- `mxm-preset::Instrument` uses `params::all_parameters` in declaration order and the persisted
  identity slot. The fifty files are generated from `FACTORY_DESIGN`. No factory preset adds an
  effect.
- `Voice::Activity`: Live → `KeepAlive`, Tailing → finite `Tail`, Inert → `Normal`. Only an envelope
  routed to the amplifier owns Tailing. Later inert samples skip the DSP except HOLD threshold and
  Key/Gate selector changes. A held press cannot reopen a panic-silenced gate.
- Telemetry uses atomics only. The S&H scope is written only while an editor is open and only on
  samples the voice ran (`the_scope_fills_while_the_voice_runs_and_holds_at_rest`).

## Editor

Implements `docs/briefs/mxm-para-07.md` as amended 2026-09-27. Rulings, history and the full
description: [NOTES.md § Editor](NOTES.md#editor).

- **`editor::SECTIONS` is the one ownership list**: thirteen cards, category-first, so a card's
  index is its stable paging `Key` (a test holds it). **Every route is on the card it moves.** Every
  permanent parameter is on exactly one card, except `volume`.
- **The master `volume` is the app bar's** (key 64, `editor::BAR_PARAMETERS`;
  `the_master_volume_is_drawn_once_in_the_app_bar`). There is no Output card.
- Pages are derived (`mxm_ui::paging::editor`). **The renderer is the only scroll owner**: never wrap
  it in another `ScrollArea`.
- **No help text on a card** (`no_card_prints_help_text`): the only caption is the oscillators' live
  *Sounding … Hz*; explanations go in tooltips (`sections::description`), in the player's words.
- **Every card is a `mxm_ui::tree`** (`sections::card`, `sections::paint`). Floors are computed and
  a card's ceiling is its floor. Painted names drop their card's prefix (`sections::panel_label`,
  `sections::TARGET_PANEL_NAMES`). An oscillator's sounding caption reserves its widest reading.
- **The two displays are pictures, not readouts**: the filter curve and the S&H oscilloscope, which
  holds the last thing played at rest (no idle motion). Neither prints a number.
- `REFERENCE` and `MINIMUM` are derived and held by `the_opening_size_is_the_budget_hugged` and
  `the_window_minimum_holds_one_widest_card_and_its_gutters`; the panel opens on one page.
  `every_dynamic_page_fits_and_every_card_is_reachable`,
  `parallel_branches_share_a_row_and_every_row_ends_level` and
  `every_card_passes_the_tree_checks_in_every_state` are the layout checks (logical geometry only).
- LFO Rate and Sample time are Standard knobs so they always show their reading
  (`sections::knob_size`). This crate contains no colour literal (`mxm_ui::theme::LEAF_GREEN`).
- **The keyboard cursor runs** (`navigation::paged_with_bar`, `navigation::at` in
  `editor/binding.rs` and `sections.rs`); `the_keyboard_cursor_reaches_and_operates_every_parameter`
  requires exact coverage.
- `editor/binding.rs` is the sole parameter-write funnel; each edit is fully bracketed. Displays and
  indicators read only `telemetry.rs` atomics. Resizing never chooses zoom, and no editor state is
  read by DSP.
- With `MXM_DEV_CC`: CC 119 selects a category's first card or Parameters (127), CC 118 is consumed
  and changes nothing, CC 117 opens the preset browser, CC 116 applies a theme without saving it.

## Control map and absent effects

Detail: [NOTES.md § Control map](NOTES.md#control-map-and-absent-effects).

- The map is checked against both the current standard and the player's frozen ten-page baseline
  fixture, and claims only roles whose semantics and curve fit.
- Intentionally unmapped: S&H timing, Gate source, Trigger input, continuous HPF and the routing
  pairs at large; both `pwm_source` roles and `filter_env.polarity`; the Amp page.
- There are no effect roles: the source hardware shipped with no effects, and none are invented.

# Work Guidance

- Preserve the one parameter order in `params::all_parameters`, the category-first
  `editor::SECTIONS` order and the atomic telemetry boundary; editor state never enters DSP.
- Keep every process-path operation allocation-, lock-, I/O- and formatting-free.
- Hardware fidelity remains UNVERIFIED until trustworthy comparison exists.

# Verification

```bash
cargo test -p mxm-para-07
# Every page, light and dark, for review -> target/layout-tree/mxm-para-07/<MXM_PICTURES tag>/
MXM_PICTURES=after cargo test -p mxm-para-07 --lib tree_pictures -- --ignored
cargo test -p mxm-player --test t5_control_map  # in the mxm-player repository; this plugin's map is held by host-tests' behaviour
cargo clippy -p mxm-para-07 --all-targets -- -D warnings
cargo xtask bundle mxm-para-07
clap-validator validate "target/bundled/mxm-para-07.clap"
cargo xtask bundle mxm-para-07 --release
clap-validator validate "target/bundled/mxm-para-07.clap"
cargo test -p mxm-para-07-host-tests      # behaviour and golden_audio, through MXM Player
cargo test -p mxm-player --test t7_editor      # in the mxm-player repository
cargo test -p mxm-player --test t7_editor -- --ignored --nocapture # there too; real window, deliberate
```

- Debug validation is mandatory because `assert_process_allocs` is debug-only. Bundle before player
  tests or they skip.
- `host-tests/tests/fixtures/control-map-old-player-ten-page.json` is frozen; never update it to
  match the current standard ([NOTES.md § Verification notes](NOTES.md#verification-notes)).
- The design-system §15 visual review and real-DAW parenting/resize remain manual.

# Child DOX Index

None.
