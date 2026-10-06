# mxm-para-07 — UI design brief

Required by `MXM_DESIGN_SYSTEM.md` §14 and written before editor work. It answers the ten questions
in order and records the DSP evidence the editor must not conceal. **Brief revision 7** retains
the settled release-portamento, duplicate-event and host-I/O contracts, corrected lever-equivalent
bend routing, deliberately unresolved gate-line topology, and dynamic card reflow from
`plans/plan-mxm-para-07.md` revision 9. The owner's two-page, quarter-4K revision was specified here
before its implementation; its fixed Synth/Mod pages have since given way to the collection's
space-derived pages (§6, §10). **The external audio input was removed by the owner on 2026-09-26**,
with its sensitivity switch, its envelope follower and the External positions of the ring
modulator's input and the mixer's fifth channel (§3, §5, §9).

**Instrument:** a two-pitch paraphonic subtractive synthesizer with two divider-derived oscillators,
one shared filter and amplifier, two envelopes, a sample-and-hold, ring modulation, and unusually
deep fixed routing. Architecture inspired by the Roland SH-7; the interface is not. The owner fixed
both the identity and category on 2026-09-04: **`mxm-para-07`**, establishing `para` for this
highest/lowest two-pitch architecture with one shared VCF and VCA.

**Evidence standing:** `research:instruments/sh-7.md` is the build reference. No working unit or
reference recording has been measured, so fidelity remains **UNVERIFIED**. Firm manual and
schematic behaviours are requirements; that page's candidate warts are not silently promoted to
facts.

---

## 1. Primary sound-design task

**Routing two keyed pitches through one shared articulation path.** The instrument's character is
not merely two oscillators or one filter: the highest key controls VCO-1, the lowest controls VCO-2,
then both pitches, the five-register square stack, ring modulation and noise meet
before one HPF, VCF, and VCA. The primary task is choosing that source mixture and deciding which of
the machine's modulation sources moves each shared destination.

The interface therefore leads with voice assignment and portamento, then the modulation system, the
pitch sources and the shared audio path in signal-chain order. It does not imply two voices, two
filters, or two release tails.

## 2. The three to five parameters users reach for most

1. **Cutoff** — the shared filter shapes every source and both keyed pitches.
2. **Resonance** — from emphasis into the diode-limited self-oscillation regime.
3. **VCO-2 tune** — the interval, beating, ring-modulator sidebands, and sync regime all depend on it.
4. **Filter audio modulation** — switching and scaling VCO-2 or noise into cutoff is a defining
   route, not an expert novelty.
5. **Sample time** — the independent S&H clock drives stepped modulation and the self-running gate
   mode.

Cutoff and Resonance use Primary controls. The LFO Rate and the Sample time are Standard, so each
always shows its reading, hertz or seconds free and the note when synced (the owner, 2026-09-27).
All other knobs, including VCO-2 tune, filter audio modulation and main source levels, use Compact
controls with visible names and values.
Shared button/selector geometry retains at least 32 × 32 pointer targets. No mode or route is hidden
merely because the source panel was large.

## 3. Signal flow that must be visible

```text
held presses ─► highest / lowest assignment ─► two directional portamentos
                  │ highest ─► VCO-1 core/dividers ─┬─► five locked square registers (1A) ─┐
                  │                                 └─► selected wave (1B) ────────────────┤
                  └ lowest / highest by mode ─► VCO-2 core/dividers ─► selected wave ──────┤
                                                ▲ unusual core-only sync from VCO-1         │
VCO-1B × VCO-2 ─► ring modulator ───────────────────────────────────────────────────────────┤
                                                                                   noise ────┤
                                                                                             ▼
                                 source mixer ─► one-pole HPF ─► four-stage VCF ─► one VCA ─► output

ENV-1 ─► VCF, VCA selection, both PWM sections
ENV-2 ─► VCA selection only
LFO ─► pitch, VCF, VCA; delayed sine only; undelayed triangle to PWM
S&H ─► pitch and VCF; its own clock can gate both envelopes
MIDI pitch bend ─► shared lever equivalent ─► each destination's CV / Off / LFO route
```

Four relationships must read without a manual:

- **Two pitches, one articulation path.** A second note adds or moves an oscillator pitch; it does
  not add a filter, VCA, envelope, or independent release.
- **VCO-1 always takes the highest held key and VCO-2 the lowest in Two-pitch mode.** One held key
  drives both. Releasing either of two keys immediately retargets both keyboard lanes to the
  survivor; a post-portamento lane still slews when the selected direction applies, so the sounding
  CV is immediate only at zero portamento or in a bypassed direction.
- **ENV-2 reaches only the VCA.** The second envelope is not a freely routable modulation source.
- **The apparent routing options are paired alternatives.** LFO or S&H, VCO-2 or noise, keyboard
  or pedal control: selecting one excludes its partner.

### External-control translation

The rear-panel controls are translated explicitly rather than inferred from note absence. In
particular, a NoteOff means a connected host gate went low; it never means that a gate jack became
unplugged.

| Hardware function | Host translation and visible behaviour |
|---|---|
| CV input | **Removed with the External key mode** (the owner, 2026-09-27). The machine's EXT CV/GATE position took both VCOs from the rear CV jack; the host's notes already are the keyboard, so a third mode that followed the last note only confused the choice. |
| Gate input and its normalled S&H clock | A persistent, preset-visible **Gate source** selector distinguishes **Host gate** (the jack is connected) from **S&H clock** (the jack is unplugged), **in either key mode** — on the machine only EXT CV/GATE reached it; the plug-in keeps the S&H clock's automatic performance and drops the mode (the owner, 2026-09-27). Host gate is high while any accepted host press is held and low after the last release; connected-low stays low indefinitely. S&H clock uses the sampler's clock and does not depend on note absence. The selector and current gate state are both visible on `Synth`, using text/shape rather than a fake jack or lamp. |
| Trigger input | A visible host-automatable **Trigger input** level reaches the hardware retrigger line. Only a low-to-high parameter event emits a trigger; it neither writes pitch nor asserts gate. Defaults and presets are low, and state restoration establishes the current level without manufacturing an edge. |
| Pedal VCF input | Standard MIDI **CC 11 Expression** is the live pedal voltage. It is kept per channel and the retained performance owner chooses the active channel, including through release; it is not written into a preset. It acts only when the Filter's Keyboard/Pedal selector chooses Pedal, retaining the hardware's fixed keyboard-tracking contribution in that position. |
| External signal input | **Removed** (the owner, 2026-09-26). No audio input is advertised. The ring modulator keeps VCO-2 as its second input and the mixer's fifth channel keeps the ring, the defaults the removed switches had. |
| CV and gate outputs | The hardware CV output is post-portamento **KCV-HIGH**: it follows high-key assignment and its directional glide; the gate output exposes the keyboard gate (`research:instruments/sh-7.md` §§3.13, 4.3). Neither output returns to the rendered voice, and a CLAP note-event output cannot represent that continuously moving CV, so this copy deliberately omits that control-output plumbing: it advertises no note-output port and emits no note events. The main output is mono or stereo; stereo duplicates the post-volume mono sample bit-identically into left and right, with no widening, panning or decorrelation. Hardware output/headphone pad choices remain host gain staging. |

### Live selector transitions and distinct control lines

**Key mode** and **Gate source** are automatable stepped selectors, not load-time configuration.
Their steady states are fixed above, but hardware evidence does not yet settle every transition while
a press is held, an envelope is releasing, HOLD is audible, or the S&H clock is autonomous. The
implementation therefore keeps keyboard depression/gate, external Host gate, the normalled S&H
clock, each envelope's selected gate, the retrigger pulse, LFO reset, and auto-bend start as
potentially distinct circuit lines. It must not invent one effective-gate edge stream and fan it out
to all consumers.

Before editor work, circuit-guided implementation traces the selector and mux topology, then
documents which unselected keyboard/external targets, owners, pitch CVs and gate levels continue or
retain; what each pitch and envelope-gate mux exposes on a live change; and which, if any, edges reach
each envelope, LFO reset, and auto bend independently. Firm keyboard behaviour stays firm: keyboard
depression supplies the documented keyboard gate, and in either envelope's LFO trigger mode it
forces the LFO to its documented restart phase whether KYBD TRIG is on or off. Whether external gate,
the normalled S&H clock, or a live selector-induced level change has the same reset or auto-bend
fan-out is left to that circuit-guided result. Focused transition scores cover held,
fully released/tailing, and autonomous-source states in both selector directions without assuming
that one observed edge must drive every destination. The editor exposes the saved choices and the
separate gate/reset/bend telemetry needed to explain the result; it does not hide or smooth sonic
consequences.

### Retained target, sounding CV and owner

The keyboard path keeps four facts distinct. Each high/low detector writes a **pre-portamento
keyboard target** into its hold stage; a downstream directional lag produces the **post-portamento
keyboard CV**; per-press tuning is then a direct oscillator-pitch offset; and the retained owner's
channel pitch bend becomes the one shared **lever-equivalent input**. That lever input reaches VCO,
VCF, and VCA only through each destination's own CV / Off / LFO selector and sensitivity: CV applies
the signed lever to that destination, Off does nothing, and LFO uses either lever direction as
rectified LFO depth. The filter's keyboard tracking and the retrigger detector follow the
post-portamento high CV, not per-note tuning or the lever input.

A collapse changes the targets and lane owners immediately, but each lag then obeys Normal, Up or
Down. On final ordinary release, targets and owners hold; the lag outputs do **not** freeze where
the release found them and do not snap to defaults. They continue toward the retained targets
through the release or HOLD, and settle to those targets if the voice becomes idle first. This is
the hardware's sample/hold-before-portamento order.

The same retained high owner supplies the shared lever-equivalent bend value and pedal state. In
Two-pitch mode it is the high assignment; in One-pitch mode it is the shared high assignment.
Before the first accepted pitch event after
activation or reset, a defined default performance channel owns the deterministic pre-note state.

### Duplicate presses and host event matching

Every accepted NoteOn is a distinct press. High/low assignment compares the MIDI key, not tuning or
bend. Among equal keys the oldest press owns a keyboard lane; with only that key value it owns both.
A duplicate press does not change gate, target or retrigger.
An equal-key handoff adopts the successor's per-note tuning and, for the high lane, its channel bend
and pedal channel, but does not retrigger merely because identity changed.

A voice ID is authoritative when both event and press carry one; otherwise channel and key match.
ID-less NoteOff, Choke and PolyTuning choose the oldest matching live press. A keyboard-mode choke
falls back like release while another press survives: a changed high target reaches the real
post-portamento trigger detector, while an equal-key handoff does not. Choking the final press cuts
the keyed articulation without a release, but not HOLD or an independently selected S&H gate.
These are deterministic software event semantics where the hardware provides no duplicate
host presses.

## 4. Controls in Play view

**No `Play` view.** This instrument has no macro system, and inventing four macros to satisfy a
shell pattern would create functionality the copy does not need. The reached-for controls in §2
stay prominent on their cards; two-pitch assignment and portamento lead the editor, while bender
routing is grouped with the other modulation destination depths. The host or MXM Player supplies note
input, pitch bend, and sequencing.

## 5. Advanced controls and disclosure

**No disclosure** (the owner, 2026-09-27). A *Setup* disclosure held one control, overall tuning,
behind a button — *a title and a setup button, and when I press it I just get a tune knob*. It is
**Master tune** on Oscillator 1's card, beside VCO-2's own Tune on Oscillator 2; it still tunes the whole
instrument, VCO-2 following with its offset. Routes, trigger modes, key mode, sync, envelope
selection, **Gate source** and **Trigger input** change what the instrument does and stay visible
on their cards. Gate source is a segmented connection choice, not a
status inferred from whether notes happen to be held. Trigger input is a named host control with its
current level and edge semantics exposed, not a decorative rear-panel jack.

The hardware's mode-dependent pulse-width slider becomes each VCO's **Pulse width** knob — where
the pulse sits — and a Pulse width route stack, both on that oscillator's card (the owner,
2026-09-27: not a card of their own). This improves the interface without widening the hardware's
one-sided pulse range or coupling the two sections.

## 6. Views

**Space-derived pages**, following design-system §3.2 and `plugins/AGENTS.md` (since the split,
2026-10-06, mxm-kit's [`docs/plugin-conventions.md`](https://github.com/mxm-audio/mxm-kit/blob/main/docs/plugin-conventions.md#editor-contract)). Each card has one
primary category, and the window's width and height decide the pages rather than an authored page
assignment:

- **Performance:** Keyboard (key mode, gate source, trigger input), then directional portamento
  with auto bend. How the keys play the voice comes first.
- **Modulators:** LFO, sample-and-hold, the parallel ENV-1/ENV-2 pair, amplifier modulation and
  the bender routes. The pitch, pulse-width and cutoff routes are on the Oscillator and Filter
  cards they move (the owner, 2026-09-24 and 2026-09-27); the bender routes are the lever's
  destination depths.
- **Generators:** Oscillator 1 (Master tune, Pulse width) and Oscillator 2 (Tune, Pulse width), then the
  phase-locked register bank.
- **Tone:** source Mixer and HPF/Filter.

The shared amplifier's output level is the master Volume, and it is not a card: it is the app bar's
output control, an inline slider beside the level meter (design system §3.1), on every page and
reached by the keyboard cursor there.

Full category names, no fixed tab count and no bar for one page. The ENV-1/ENV-2 and oscillator
pairs are same-category preferred groups. Every permanent parameter is present exactly once on the
musician cards or, for Volume, in the app bar, and host automation remains the complete host-owned
list; the collection's developer `Parameters` surface (CC 119 value 127) has no tab. There is no
`Effects` category because the original had no effects, and no `Sequencers` card because the host
owns sequencing.
S&H-clocked automatic performance is the Keyboard card's Gate source; it is a voice-routing
behaviour, not a step sequencer.

## 7. Identity accent

**Leaf green**, owned by mxm-kit's `crates/ui` as a semantic `mxm-ui` identity token rather than
colour literals in this plugin. The proposed token resolves to dark `#63CF63` and light `#2F692F`; the
shared theme tests must pin both values' contrast and must verify that applying the identity leaves
modulation and status colours unchanged. The editor consumes only that token.

Measured with the WCAG formula used by `mxm_ui::theme::contrast`:

| | vs `surface-1` | vs `surface-2` |
|---|---:|---:|
| Dark `#63CF63` | **8.83 : 1** | **8.07 : 1** |
| Light `#2F692F` | **6.60 : 1** | **5.53 : 1** |

Both clear 4.5:1 for text and 3:1 for control boundaries. At roughly 120° hue, the accent sits
between the collection's acid lime and performance green rather than colliding with either; it is
also separated from teal, LFO blue, key/voice violet, orchid, rose, danger, copper, and warning.
It occupies structure and selection only, never modulation or status meaning. A single restrained
green accent does not reproduce the source hardware's black panel or multi-colour slider-cap
arrangement.

## 8. Live visualizations

1. **~~Two-pitch assignment and external-state strip~~ — removed** (the owner, 2026-09-27: *this
   is a mess. Why is it there?*). A live ledger of owners, targets and CVs explained the machine's
   internals rather than the sound; the oscillators' sounding frequencies remain as readings.
2. **Filter response: set and sounding.** The response curve at the set cutoff, and — while a
   note sounds — dashed at the cutoff after envelope, LFO/S&H, key/pedal, audio-rate and bender
   terms. No readings printed (the owner, 2026-09-27: *these numbers make little sense*; an idle
   voice printed *Sounding cutoff: 1.0 Hz*); what the curves mean is its hover text.
3. **Sample-and-hold scope.** An oscilloscope of its output, rolling in from the right over eight
   sample times, with an LFO source drawn faintly behind, so the interaction between the
   independent sample clock and the LFO is visible rather than guessed. It moves while a note
   sounds and holds the last thing played at rest. It replaced three numeric meters and a clock
   readout, and then a computed picture of the steps (the owner, 2026-09-27: *can't you just show
   the sample and hold waveform instead?*, then *I meant an oscilloscope animation*).
4. **Overload and output metering.** The mixer warning reports pre-filter overload without limiting
   it; the app bar meter and latched clip state, beside the master Volume, report post-output
   level. Neither is drawn as a fake lamp.

Telemetry is atomics or a bounded lock-free snapshot, written by the audio thread without locks or
allocation. Peaks are max-combined and reset on read; clip remains latched until acknowledged; work
stops or throttles when the editor is closed.

## 9. What is removed from the source hardware layout, and why

**Kept:** the evidenced control functions, section membership, signal relationships, and the
external CV/gate/trigger/pedal distinctions in §3's translation table. **Removed:** the face,
keyboard furniture, and output plumbing that a plugin host already supplies.

| Removed or translated | Why |
|---|---|
| Physical panel geometry, slider banks, typography, colours, case, and wordmarks | Design system §2 forbids a hardware-replica interface. Cards follow tasks and signal flow. |
| The 44-key keyboard and bender lever | The host supplies note and bend input. The two-pitch allocator and all three bender routes remain in the voice. |
| Keyboard transpose | The player/DAW already transposes; it is keyboard furniture rather than the voice. |
| CV and gate output jacks | The CV jack exports post-portamento KCV-HIGH, including high-key assignment and glide, and the gate jack exports keyboard gate. They are omitted because this is control-output plumbing with no return into the rendered voice, and CLAP note events cannot reproduce a continuously moving CV—not because the hardware merely duplicates host input. The plugin advertises no note-output port and emits no note events. Gate **input**, its normalled connection, trigger input and pedal input are preserved by §3 instead of being conflated with note absence. |
| Output and headphone pad choices | One plugin output level replaces post-VCA resistor pads; the independent headphone amplifier is not part of the rendered voice. |
| Power and five-minute warm-up control | Warm-up is an operating constraint, not a measured mandatory sonic defect. |
| Any effect, arpeggiator, sequencer, MIDI panel, or patch memory from later software | None was on the source. Presets are the collection's required storage surface; sequence and arp remain host-side. |

The external **audio** input is removed (the owner, 2026-09-26): the jack, its sensitivity switch
and its envelope follower, and the External positions of the ring modulator's input and the mixer's
fifth channel. Neither default changes — the ring stays VCO-1 times VCO-2 and the fifth channel stays
the ring — so no factory sound changed. Stereo and mono, with no audio input, are the only layouts.

## 10. Quarter-4K fit, minimum size and 200% zoom

**Resizable, opening on one page** since 2026-09-27 — it opened on two, *Performance +
Modulators* and *Modulators + Generators + Tone* (the owner, 2026-09-24), until the cards removed
that day let all thirteen fit (`plans/plan-editor-standard.md` R0); the opening and minimum sizes
are derived from the cards' computed floors, and the editor's constants and tests hold them.
Category and card order are §6's. The opening size is the quarter-4K budget hugged (owner,
2026-09-09), derived by `the_opening_size_is_the_budget_hugged` rather than chosen.
`every_dynamic_page_fits_and_every_card_is_reachable` checks every derived page, in both themes, at the opening size, 1280 × 800 and the minimum, at 1×/2× with that simulated
physical budget fixed. Fitting pages do not scroll; only an indivisible overflow scrolls on both
axes. Cards keep their signal order inside each category, every card in a row shares its top and
bottom, and the ENV-1/ENV-2 and oscillator pairs stay on one row whenever it can hold them.

Dense knobs use the shared **Compact** tier except the two Primary filter controls and the two
Standard syncable knobs; names remain
visible, formatted values remain available on hover/focus and direct entry, and every control keeps
the design system's pointer floor. Each card's width floor is the larger of its no-overflow width,
computed from the card's layout tree, and its declared usable compact-control width. The minimum is one widest card (either
envelope) plus workspace gutters, and the card ceiling leaves spare canvas outside a lone card
instead of stretching its border across the row.

Zoom is the collection's app-bar control, chosen at **75–200%** independently of window width;
resizing never derives or changes it. Keep the physical window fixed for §15's DPI/zoom gate. The
earlier two-page editor's physical measurement (an 1880 × 1020 client in an 1896 × 1059 outer
window at Windows 96 DPI) does not carry over: the native window, real-DAW parenting and owner
inspection of the derived pages remain open.

---

## Init patch declared before editor work

- One plain source sounds: VCO-1B on a conventional waveform; VCO-1's register bank, VCO-2, ring/
  external, and noise are down.
- The filter is effectively open without wasting the control's top, the HPF is at its pass-through
  end, resonance and every modulation amount are zero, and ENV-1 opens the VCA.
- VCO-2 starts slightly detuned but silent, satisfying the multi-oscillator init rule without making
  the initial sound beat.
- Envelope times, oscillator ranges and waves, LFO/S&H rates and sources, key mode, trigger modes,
  and bend configuration start at useful settings; portamento, auto bend, LFO delay, HOLD, and
  bender depths start at zero because they are amounts.
- Init contains no effect and selects **Host gate**, so it has no autonomous S&H gate. Trigger input
  and every external performance level begin low. Init is generated from the CLAP defaults and has
  no preset file.

## Evidenced warts the DSP and interface must preserve

Grouped from `research:instruments/sh-7.md` §9; the grouping does not soften any row there.

| Area | Required behaviour |
|---|---|
| Divider oscillators | Octaves within each VCO are phase-locked; VCO-1's five registers cannot beat against VCO-1B; Range selects a divider tap; saw amplitude comes from a separately driven waveshaper; triangle is a folded saw. |
| Sync | VCO-1 resets VCO-2's high-frequency core without clearing its divider or disconnecting VCO-2 CV. Engagement phase, VCO-2 slope, unusual divided patterns, and out-of-range failure remain meaningful. |
| Keyboard, hold and glide | Highest goes to VCO-1, lowest to VCO-2; one key drives both; key-up immediately collapses both retained targets onto the survivor while each downstream directional lag slews or bypasses according to its topology; filter tracking follows the post-portamento upper CV; documented high-key press/release retrigger is derived there. Final key-up holds both targets and owners while the two lag outputs continue toward those targets through release/HOLD rather than freezing or snapping to defaults. |
| Routing limits | ENV-2 reaches only VCA; ENV-1 outweighs the cutoff slider; paired filter sources remain alternatives rather than sums. |
| LFO, envelope trigger and PWM | Destination-dependent DC offsets remain; delay fades sine only; PWM always receives undelayed triangle; pulse width narrows from 50% and never widens; the switched-inversion saw keeps its seam. In either envelope's LFO trigger mode, its envelope gate is the LFO square AND the gate selected for that envelope. Keyboard depression forces the documented LFO restart whether KYBD TRIG is on or off; external-gate, S&H-clock and live-selector reset fan-out remain circuit-guided rather than assumed. |
| Sample-and-hold | Its source choice is independent of the LFO waveform selector; it has its own clock, hold droop, and lag; with the Gate source on the S&H clock, that clock gates the envelopes, in either key mode. |
| Mixer and sources | The five-register mixer is level-normalised; the ring has the highest mixer weight; overload warns but does not limit; the balanced ring modulator, passive pink approximation, and explicit white-noise path remain distinct. |
| Envelopes, filters and amplifier | With all envelope controls at zero, each envelope produces the documented extremely short pulse; when that pulse drives VCF or VCA, the click is preserved and no hidden de-clicker removes it. HPF is one passive pole and loses level as its corner rises; VCF is a four-OTA low-pass with diode-limited feedback and no invented compensation; HOLD shares the VCA control path with envelope, LFO, and bender. |
| Performance modulation | Auto bend fires from the documented keyboard gate assertion, not release or retrigger; whether external/S&H/mux edges can start it is circuit-guided. MIDI pitch bend is the shared lever equivalent: each VCO/VCF/VCA destination independently chooses direct CV, Off, or rectified LFO-depth control, so Off is inert and either bend direction raises depth only in LFO mode. |
| Absences | No second LFO, clock sync, external S&H clock, VCO-to-VCO FM, sequencer, effects, wider-than-50% pulse, or independent articulation per pitch. |

Not mandatory without new evidence: live Range-switch transients, lower-key retrigger, delayed or
suppressed retrigger under long portamento, the LFO saw's relative rate, loaded VCA law, residual
waveshaper amplitude error, or a particular amount of bass loss at resonance. The implementation
records any calibration chosen around those gaps and keeps the fidelity gate UNVERIFIED.

### Fidelity proof obligations

The public per-plugin DSP harness measures the software model; hardware/reference measurements stay
in research. Besides the row-by-row behavioural tests above, event scores distinguish retained
targets from moving post-portamento CVs on collapse and final release, and distinguish duplicate
IDs, channels, tuning, ordinary release and choke handoffs. Gate-topology scores observe each
envelope-gate, LFO-reset and auto-bend-start line separately across keyboard, external, S&H and live
selector cases; only the documented keyboard depression/restart and keyboard-gate auto-bend paths
are fixed before the circuit-guided result. Bender scores drive positive and negative MIDI pitch bend
through each VCO/VCF/VCA mode independently, proving signed direct action in CV, no action in Off,
and same-direction rectified LFO-depth control in LFO, while per-note tuning remains a direct
oscillator offset. Alias measurements compare the model against suitable
additive or oversampled references for: divider taps and the waveshaper; the five-register stack;
static and modulated narrow PWM; core-only sync across engagement phases, lock and failure regimes;
and ring modulation with ordinary and synced inputs. The harness records its own
comparison floor. Every alias-sensitive score pairs rejection with a wanted-spectrum oracle: wanted
harmonic error or retention where an additive spectrum is known, and passband/reference error where
an oversampled rendering is the reference. Under matched fundamental/output level the model must
materially outperform a deliberately trivial/base-rate generator, or meet an independently audited
rejection bound, **without** buying that result by dulling the wanted high-frequency content. Exact
margins and measurement windows are fixed during implementation from the baseline/reference
comparison with headroom against both failure modes, following mxm-kit's
[`docs/oscillators/06-testing.md`](https://github.com/mxm-audio/mxm-kit/blob/main/docs/oscillators/06-testing.md)
§6.4 and [`docs/oscillators/AGENTS.md`](https://github.com/mxm-audio/mxm-kit/blob/main/docs/oscillators/AGENTS.md); oversampling choices and calibration values are measured and
documented beside the model rather than invented in this brief. The candidate's measurements alone
cannot set either half of the alias gate. Structural
host tests, not an audio score, prove that no note-output port or emitted note event exists; separate
render tests prove stereo is bit-identical dual mono. Until trustworthy hardware/reference
measurements exist, those tests prove a bounded, self-consistent model rather than hardware fidelity.

## Recognisability trial and sign-off

Before composition, a wireframe trial asks a player familiar with paraphonic or vintage subtractive
synths: (1) which oscillator takes each of two held notes; (2) how many filters and VCAs there are;
(3) where ENV-2 can go; and (4) what makes the automatic S&H performance run. On the finished
editor, the player must find Cutoff, VCO-2 interval, filter audio modulation, Sample time, key mode,
and output level within ten seconds each, entering at most one wrong page. Five of six is the gate.

The editor is built with in-process coverage for both themes, all zoom choices, keyboard-readable
names, non-overlapping filter/S&H displays and atomic telemetry. The design
system §15 visual review remains honestly manual at 75%, 100%, 150%, and 200%, as do the
recognisability trial, perceived hierarchy/originality check, and real-DAW parenting and resize gate.
