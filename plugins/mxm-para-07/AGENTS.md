# AGENTS.md — plugins/mxm-para-07

Parent: [`../AGENTS.md`](../AGENTS.md)

# Purpose

The nice-plug shell for `mxm-para-07`: permanent identity, parameters, MIDI/events, state, fifty
factory presets, realtime processing, atomic telemetry, the dynamically paged editor and the
instrument-owned control map over
[`crates/mxm-para-07-dsp`](../../crates/mxm-para-07-dsp/AGENTS.md). Architecture inspired by the
Roland SH-7; that maker/model is reference text, never product, parameter, mode or preset identity.

The editor factory stage is built. The design-system §15 visual review, recognisability trial and
real-DAW parenting/resize test remain manual, and fidelity remains UNVERIFIED.

# Ownership

Owns `BASELINE-C0.md`, `Cargo.toml`, `LICENSE`, `README.md`, `control-map.json`, `presets/` and
`src/`. DSP remains in the framework-free sibling crate; shared preset behaviour remains in
`mxm-preset`.

**`BASELINE-C0.md` is the routing conversion's reference**, captured before any of it
(`plans/plan-mxm-para-07-modulation.md` §10): Init's and the fifty factory sounds' digests, one
focused oracle per row of that plan's §1 at partial and full depth, and the throughput the cost gate
compares — on **both source paths**, because `render_sources` renders 1× for linear waveshapes with
the ring silent and 8× for a triangle, a pulse or an active ring, and an 8× render would hide a
regression in a routing sum. `lib.rs`'s `#[ignore]`d `baseline` module produces all of it and runs
unchanged on the converted tree.

The seam is not a second `process()`: `render_one` **is** the per-sample path, and `process` calls it
rather than repeating it. It used to hold its own copy of the same four steps, which merely agreed
with the seam's — a per-sample amount advance added to one and not the other would have split the
measured path from the host's silently. What `process` still owns is block-rate work only: the
telemetry publish and the peak and overload accumulation.

**The reference is blind to velocity, the wheel and pressure, and necessarily so** — those are the
three MIDI paths the conversion *adds*, so they have no before to be identical to. Everything the
machine already has is driven: both pitch lanes, the pedal and the lever — and the external port
until it was removed (2026-09-26); every digest held without it.
`the_focused_paths` asserts that no oracle renders Init's own digest and that no case renders alike
at partial and full depth, because twice while building it a case looked like data while measuring
only a silent oscillator or a clamped filter.

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
- **Two tempo syncs** (2026-09-25, `plans/plan-tempo-sync-controls.md`), each the collection's
  quarter note beside its knob: `lfosync` on `params::LFO_SYNC` (1/32 to four bars, the top the
  fastest; this LFO's 0.2 Hz floor holds two bars but not four at 120 bpm) and `shsync` on
  `params::SH_SYNC` (1/64 to a whole note, the top the longest). `synced_lfo_rate` and
  `synced_sh_time` resolve them once a callback from the modulated positions; `Telemetry::tempo`
  lets the knobs read their divisions. `sections::synced_row` keeps a knob, its quarter note and
  what follows in one flat row — nested, the inner row keeps the width it is offered and pushes
  the next knob across the card.
- **Routing pairs**: one presence and one amount per *(target, source)* pair — nine targets by
  eighteen sources, **161 pairs, 322 ids**, spelled `mod_<target>_<source>` and
  `mod_<target>_<source>on`. The machine's gain CV keeps its `mod_amp_*` ids and is shown as **VCA
  level**; **Amplitude** (`mod_amplitude_*`) is the collection's standard factor after it
  (`plans/plan-modulation-standard.md`); **Bank level** (`mod_bank_*`, the owner, 2026-09-27) is the
  register bank's Level knob as a target — the standard's control, not on the machine, every pair
  at the standard reach normalised by its source's peak (`BANK_LEVEL_SCALE`), absent at Init. **One pair is refused** — VCA level ← Key, which could only
  latch the voice open — so `TargetRoutes` writes its `Params` impl by hand, the derive's ids in the
  derive's order with that pair's two left out; the multiplier may still read itself, one sample
  late like any backward route. VCA level ← Velocity travels only its positive half. `ROUTE_IDS`
  writes the ids out and a test holds it to what is actually registered.
- **Route readings are the shared `mxm_modulation_params::reading`** (`reading::amount_param_at`):
  st, oct or % of what the pair delivers at its source's peak, Key per octave — a pulse width in
  percent of the cycle (the machine's PWM at full `+40 %`, an added pair the standard `+45 %`).
  `every_route_parameter_says_what_the_dsp_does` (`mxm_plugin_test::routing_checks`) holds each
  pair's registration, travel, unit and reading to `mxm_para_07_dsp::conformance::Declared`.
- **Twenty-eight ids are retired** (`plans/plan-mxm-para-07-modulation.md` §5) and **none may ever
  be re-used**: `vco1pwm` `vco1pwmsrc` `vco1lfo` `vco1sh` `vco1auto` and the matching `vco2*` five,
  `filterenv` `envpolarity` `filtermodsrc` `filtermod` `keysource` `keytrack` `filteraudiosrc`
  `filteraudio` `follower` `followerpolarity` `vcaenv` `vcalfo` `bendvcomode` `bendvco`
  `bendfiltermode` `bendfilter` `bendvcamode` `bendvca`. Each became routes; the reachability
  property in `crates/mxm-para-07-dsp/tests/legacy_reachability.rs` proves no sound went with them,
  except `follower` and `followerpolarity`, whose path went with the external input.
- **Seventeen more retired with the external input** (the owner, 2026-09-26) and **none may ever be
  re-used**: `ringinput` `fifthinput` `extsensitivity`, and the Follower source's pair on all seven
  targets, `mod_<target>_follow` and `mod_<target>_followon`. The ring modulator keeps its default
  second input, VCO-2, and the mixer's fifth channel its default, the ring, as fixed paths. All
  forty-five went to `params::RETIRED_IDS`, which `no_retired_id_is_back` holds. A session or preset
  naming them still loads: the wrapper skips unknown state ids
  (`a_session_naming_the_retired_external_ids_restores_everything_else` in the player's behaviour
  test) and the preset layer reports and skips unknown ids
  (`a_preset_naming_the_external_inputs_retired_ids_loads_everything_else`) — a user preset saved
  before shows them as its load problem. **A patch that had selected External loses that choice**
  and plays the default path; every factory sound held both defaults, so none changed.
- **Two more retired with the modulation standard** and **never to be re-used**: `mod_amp_key`
  `mod_amp_keyon`, the refused pair. All forty-seven are `params::RETIRED_IDS`.
- **`keymode` has two values**, *Two-pitch* and *One-pitch*: the third, External (variant id
  `external`), retired on 2026-09-27 (the owner) and its id is never to be re-used. A session that
  chose it restores as One-pitch (`params::retire_external_key_mode`, in `filter_state`; nice-plug
  stores a key mode by variant id and would otherwise only log the unknown one); a preset stores the
  normalised value, which rounds to One-pitch on its own. Its S&H gating is the Gate source, in
  either mode. *Clocked gate* and *Clocked drone* are One-pitch with the S&H clock.

Never rename or reuse an id. Tests compare the derive's complete set with `all_parameters`.

## Host and event behaviour

- Audio layouts are stereo then mono, with no audio input of any kind and no note output. Stereo
  duplicates one post-volume sample. Bundle-driven host tests select both exact configurations,
  compare mono with bit-identical dual mono, require that no third is advertised, and attach an
  output-event sink that must remain empty.
- Every NoteOn reaches the DSP's fixed ledger, which retains its velocity; a zero velocity is
  still read as a NoteOff. **The Velocity source is `v − 1` of the press that last triggered an
  envelope** (the standard's rule): under GATE+TRIG each press brings its own, under GATE a legato
  press keeps the phrase's. NoteOff,
  Choke and PolyTuning preserve the DSP's ID-authoritative/oldest-match semantics.
- Pitch bend is per channel through the retained high owner's lever routes. **Init wires
  Pitch ← Bend at ±2 st on both VCOs** (the owner's ruling, an Init deviation), so every factory
  sound, generated from Init, carries it too. CC 11 is the
  per-channel pedal voltage; in Pedal mode the amount scales that voltage while fixed 1 V/oct
  high-key tracking remains. CC 1 is the per-channel mod wheel and channel pressure is retained per
  channel, both kept whether or not a note sounds so a note started under an already-raised wheel
  inherits it. The machine had neither, so both exist only as routable sources and carry nothing
  until a route reads them. CC 120 is immediate latched panic; CC 123 is ordinary release.
- `Trigger input` emits only a low-to-high edge. Activation, reset and state restoration adopt its
  current level without manufacturing a trigger. Sample-accurate automation is enabled because the
  parameter event's offset determines that edge; a low-high-low pulse inside one host block is kept.
- Events split processing at their sample offset and event-free spans are capped at 64 samples.
- Activation accepts only finite sample rates at or above the DSP's 12 Hz safety floor. Lower or
  non-finite host configurations are refused before any processing state is changed.
- **Parameter text is idempotent through the host's normalised conversion** (format, parse, format
  gives the same text), which `param-conversions` checks only at its own grid. A time chooses `ms`
  or `s` from its *rounded* milliseconds: chosen from the raw value, 0.9995 s printed `1000 ms` and
  read back `1.00 s` (Portamento, LFO delay, Sample time, Sample lag), and the envelope times, whose
  inverse of one second lands just below it, read `1.00 s` back as `1000 ms`.
  `params::tests::every_parameter_text_is_idempotent_through_the_hosts_conversion` walks every
  parameter with the unit on, over clap-validator 0.4.1's own grid, the `i / 19` grid and both
  sides of every unit, precision and sign switch.

## Init, state, activity and telemetry

Init equals the CLAP defaults: one 8' saw at 0.7, VCO-2 silent and +0.07 semitone, effectively open
VCF, pass-through HPF, Host gate/Two-pitch, useful envelope/LFO/S&H configurations, and every depth,
resonance, glide, delay, HOLD, trigger level and bend amount at zero — except the ±2 st Pitch ←
Bend routes. **Each Pitch stack opens with LFO and Bend only** (the owner, 2026-09-27): the Sample
and hold and Auto bend depths, the machine's own sliders, are no longer present at Init, and the
four factory sounds that use them (*Auto rise*, *Auto fall*, *Stepped pitch*, *Random drift*) set
their presence themselves. The width knob is each pulse-width target's
base and routes add to it; the depth knob and the source switch retired into a route amount and a
route presence.

`mxm-preset::Instrument` uses `params::all_parameters` in declaration order and the persisted
identity slot. The fifty files are generated from `FACTORY_DESIGN`, complete, categorized, distinct
and audible. No factory preset adds an effect; ring modulation, filter audio modulation and S&H are
internal researched signal paths, not effects.

`Voice::Activity` owns status: Live → `KeepAlive`, Tailing → finite `Tail`, Inert → `Normal`.
Only an envelope **routed to the amplifier** owns Tailing, and the tail is the longest such
release; an unrouted release does not keep silent DSP awake, and a route at zero depth carries
nothing either way. On first inert sample recursive state settles; later inert
samples skip the entire DSP, except both HOLD threshold transitions and Key/Gate selector changes
still reach the voice so their wake-edge history remains current. A held press cannot reopen a
panic-silenced gate. Telemetry uses atomics only: a max/reset output peak, latched clip/overload,
the sounding cutoff and oscillator frequencies, whether a note sounds, and the sample and hold's
scope — its source and output, one point a millisecond (`SCOPE_RATE_HZ`, a stride of samples set
at activation) into two `SCOPE_LEN` rings, written only while an editor is open
(`Telemetry::editor_open`, read once a callback) and only on samples the voice ran
(`feed_scope`; `the_scope_fills_while_the_voice_runs_and_holds_at_rest`).

## Editor

Implements `docs/briefs/mxm-para-07.md`, as the owner amended it on 2026-09-27. Thirteen cards in
`editor::SECTIONS`, the one ownership list, each with one primary category: **Performance** —
Keyboard, Portamento and auto bend; **Modulators** — LFO, Sample & Hold, Envelope 1/2, Amplifier
modulation, Bender routes; **Generators** — Oscillator 1/2, Register bank; **Tone** — Mixer, Filter.
**Every route is on the card it moves** (the owner, 2026-09-24 and 2026-09-27): each oscillator
carries its Tune and Pulse width knobs, then its Pitch and Pulse width stacks; the Cutoff stack is
under the Filter's knobs, and the bank's Level stack under the Register bank's knobs. Oscillator
1 carries **Master tune** (`tune`), the instrument's tuning, which VCO-2 follows with its own offset
(its own *Tune*, on Oscillator 2). Amplifier modulation holds the VCA level stack, then the Amplitude stack.
**Auto bend shares Portamento's card** (R2's call, the owner's to overrule): each is a timed pitch
motion, a time and a direction; hugged, Auto bend was one knob and a switch. **Gone on 2026-09-27**,
on the owner's word: the *Two-pitch assignment* display (a live dump of owners, targets and CVs —
*a mess*), the *Voice setup* card (a Setup disclosure holding only Tune) and the *Pulse width* card.
`SECTIONS` is category-first, so a card's index is both its stable paging `Key` and its place in
the derived order, and a test fails if that stops being true. Bender routes is mixed-purpose; it is
a Modulator because it holds the lever's destination depths. Every permanent parameter is on
exactly one card, except the master `volume`, and all remain available through host automation.

**The master `volume` is the app bar's** (owner, 2026-09-18: an instrument's master output is in the
app bar; there is no Output card). It is an inline slider beside the level meter (design system
§3.1), drawn inside `mxm_ui::navigation::bar_card` under key 64, outside the paging keys.
`editor::BAR_PARAMETERS` is its entry in the ownership test beside `Section::parameters`, and
`the_master_volume_is_drawn_once_in_the_app_bar` holds that it registers only under the bar card,
whichever card is requested.

`mxm_ui::paging::editor` derives the pages from width and height (`plugins/AGENTS.md`): there is no
authored page assignment and no bar for one page. Envelope 1/2 and Oscillator 1/2 are the
same-category preferred groups. The renderer is the only scroll owner — do not wrap it in another
`ScrollArea`, which hid the real viewport from reflow and made native resize lag and jump in the
earlier two-page editor.

**No help text on a card** (the owner, 2026-09-27; design system §7.6): the only caption is a live
reading, the oscillators' *Sounding … Hz* while a note sounds (empty at rest), which
`no_card_prints_help_text` holds. What a caption
used to explain is its control's tooltip (`sections::description`), written for the player — what
the control does to the sound, never the circuit's vocabulary.

**Every card is a `mxm_ui::tree`** (`crates/ui/AGENTS.md`, *A card body as data*).
`sections::card` describes a card's body once and that one description is measured for the card's
floor and height and drawn leaf by leaf through the same bindings (`sections::paint`); the paged
view is `paging::editor::show`. **Floors are computed**, and each card is exactly as wide as its
floor: its ceiling is its floor (`plans/plan-editor-standard.md` A1), with no usability minimum
(A2). **Painted names drop the prefix their card already carries** (`sections::panel_label`,
design system §7.1): *Rate*, *Attack*, *Range*, *32'*, *Pulse width*; a stack's title from
`sections::TARGET_PANEL_NAMES` — *Pitch* and *Pulse width* on an oscillator. Sources are
capitalised labels (*Sample and hold*, *Envelope 1*), which is the routes' host names too; a
switch's cells are its parameter's own option text. Knob rows are the collection's
`mxm_ui::tree::knob_row` (at `mxm_ui::control::knob_column`), four to a row, and **a knob's
column holds its name in the name box's two lines and, for Cutoff and Resonance, its widest reading
on its one line** (`sections::knob`). What a card prints from telemetry — the oscillators' sounding
frequencies — and whether the Mixer carries its overload button are read once per frame
(`sections::Readings`) and are inputs to the tree; **an oscillator's sounding caption reserves its
widest reading** (`sections::sounding`), so a note starting or a frequency changing never moves the
card. There is no disclosure: the developer channel's CC 118 is consumed and changes nothing, as in
mxm-mono-08. **The two displays are pictures, not readouts** (the owner, 2026-09-27: *these numbers
make little sense*), each filling the card at its fixed height (`FILTER_HEIGHT`, `SH_HEIGHT`) and no
narrower than `DISPLAY_MIN_WIDTH`: the filter's curve at the set cutoff, and — only while a note
sounds (`Telemetry::sounding`, published each block from the voice's activity) — dashed at the cutoff
the voice is playing; and **the sample and hold as an oscilloscope** (the owner, 2026-09-27: *an
oscilloscope animation*, after a computed picture of its steps): its output rolling in from the
right, the LFO it samples faintly behind when the source is the saw or triangle, over eight sample
times (`visuals::window`, synced times resolved, 0.25 s to the ring's 8.2 s), each pixel column
drawn by its first, lowest, highest and last point (`visuals::columns`) so a 25 Hz LFO keeps its
swing and a step stays upright. It repaints every frame while a note sounds and **holds the last
thing played at rest**, where the voice's LFO is parked too — no idle motion (design system §9).
Neither display prints a number; each says what it shows in its hover text.

The resizable editor opens at the quarter-4K budget hugged (`REFERENCE`, 1721 × 1064, held by
`the_opening_size_is_the_budget_hugged`), **on one page** since 2026-09-27: with the assignment
display, Voice setup and Pulse width cards gone and two default rows off each Pitch stack, all
thirteen cards fit where they had needed two pages (the owner's ruling of 2026-09-24 for that larger
panel). When the LFO Rate and Sample time began showing their readings (below) the LFO and S&H
cards widened by about ten points and the top row of seven no longer fitted; **the envelopes'
trigger reads *Gate + trig***, as mxm-mono-02's does (the owner, the same day), which narrows both
envelope cards and puts it back on one page. A whole category never
costs a page (`crates/ui/AGENTS.md`). Its minimum holds one widest card plus
workspace gutters (`MINIMUM`, held by `the_window_minimum_holds_one_widest_card_and_its_gutters`), and
the app bar at its last compact step is wider and sets it (`the_app_bar_holds_in_the_minimum_window`).
`every_dynamic_page_fits_and_every_card_is_reachable`
checks every derived page at the opening size, 1280 × 800 and the minimum, in both themes at 1× and
2× with the simulated physical window fixed; `parallel_branches_share_a_row_and_every_row_ends_level`
reads every card's rectangle on a tall canvas; `every_card_passes_the_tree_checks_in_every_state`
runs the shared per-card checks (`mxm_plugin_test::tree_checks`) over every card at Init, with
every route revealed at full negative depth, with the mixer overload latched, and with the telemetry
text at its longest. That is logical geometry: the physical window, DPI
and real-DAW behaviour have not been re-measured since the pages became derived. Dense knobs use
the shared Compact tier except the Primary Cutoff and Resonance controls and **the two syncable
knobs, LFO Rate and Sample time, which are Standard** so they always show their reading — hertz or
seconds free, the note when synced (the owner, 2026-09-27: every LFO shows its time or rhythm, as
mxm-mono-00's do; `sections::knob_size`, held by
`dense_continuous_controls_use_compact_without_reducing_the_pointer_floor`); labels stay visible and
the others' values remain available through hover/focus and direct entry. The app bar owns presets, the latched
level/clip meter, the master Volume and the collection's `mxm_ui::shell::zoom_control` and
`editor_theme_control`; the opening theme is `mxm_ui::theme::preference`. Leaf green is
`mxm_ui::theme::LEAF_GREEN`, owned and contrast-tested by `crates/ui`; this crate contains no colour
literal.

The keyboard cursor runs. `panel` holds a `navigation::State` and calls
`navigation::paged_with_bar` with the Volume bar card before the cards — or `navigation::stop` on
the Parameters surface — with the same busy condition it hands the renderer's `hold`.
`editor/binding.rs` wraps every knob, slider and selector in `navigation::at` with the parameter's
step law, and `sections.rs` does the same for toggles.
`the_keyboard_cursor_reaches_and_operates_every_parameter` requires exact coverage.

`editor/binding.rs` is the sole parameter-write funnel: controls show the unmodulated base while
host modulation or the current drag is a secondary overlay, and each edit is fully bracketed. The
displays, the sounding readings and overload/output indicators read only `telemetry.rs` atomics; each
display is named for the accessibility tree (`the_displays_are_named_and_print_no_readings`), and a
regression locates each real painted background and requires the following control below it for
Filter and S&H across themes and widths. The editor requests a frame every 50 ms while open, and
every frame while the scope moves;
resizing never chooses zoom, and no editor state is read by DSP.

With `MXM_DEV_CC`, CC 119 selects a category's first card (0 Performance, 1 Modulators, 3
Generators, 4 Tone) or the tabless Parameters surface (127); CC 118 is consumed and changes
nothing (there is no disclosure), CC 117 opens the preset browser, and CC 116 applies a theme
without saving it. In-process tests request each card rather than assuming its page, and cover the
live readings, the browser, the Parameters surface, exactly-once parameter ownership and bracketed gestures. The player's `t7_editor` adds
bundle-driven advertisement plus deliberately interactive open/close/reopen lifecycle coverage.

## Control map and absent effects

The map is checked against both the current standard and the player's frozen pre-instrument
ten-page baseline fixture. It claims only roles whose semantics and curve fit. S&H timing,
Gate source, Trigger input, continuous HPF and the routing pairs at large remain intentionally
unmapped. Envelope 1 fills the Filter-envelope page because it is the only envelope the machine
wires to the filter. **Three roles are unfilled since the routing conversion**, as `mxm-mono-02` and
`mxm-poly-06` already record: both `pwm_source` roles, the source switch having become two routes,
and `filter_env.polarity`, the polarity now being the sign of the (Cutoff ← Envelope 1) amount.
**Three name route amounts Init wires** — `osc1.pwm_depth` and `osc2.pwm_depth` the pulse-width
triangle pairs, `filter.key_track` the Cutoff ← Key pair, `lfo1.to_amp` the VCA level ← LFO
centred pair — so no role is dead on a fresh instance. The Amp page stays empty: which envelope reaches the
amplifier is a route presence now, and either or both can be routed there, so no static Amp-role
mapping is correct in every state. There are no effect roles:
the source hardware shipped with no effects (`research:instruments/sh-7.md` §10.1), and none were
invented.

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
cargo test -p mxm-player --test t5_control_map
cargo clippy -p mxm-para-07 --all-targets -- -D warnings
cargo xtask bundle mxm-para-07
clap-validator validate "target/bundled/mxm-para-07.clap"
cargo xtask bundle mxm-para-07 --release
clap-validator validate "target/bundled/mxm-para-07.clap"
cargo test -p mxm-para-07-host-tests      # behaviour and golden_audio, through MXM Player
cargo test -p mxm-player --test t7_editor
cargo test -p mxm-player --test t7_editor -- --ignored --nocapture # real window, deliberate
```

Debug validation is mandatory because `assert_process_allocs` is debug-only. Bundle before player
tests or they skip. `host-tests/tests/fixtures/control-map-old-player-ten-page.json` is the
byte-for-byte ten-page player standard at software commit `68e3f82`, frozen there so a mutable
current standard cannot masquerade as backward-compatibility proof; `behaviour` holds this
instrument's map to both it and the current standard. The design-system §15 visual review and real-DAW parenting/resize remain manual;
Linux, macOS and hardware comparison remain unverified.

# Child DOX Index

None.
