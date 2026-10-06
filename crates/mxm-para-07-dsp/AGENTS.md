# AGENTS.md — mxm-para-07 DSP

## Purpose

Framework-free DSP for `mxm-para-07`: two keyed pitches through one shared mixer, HPF, resonant
low-pass, dual envelopes and VCA. It implements the Roland SH-7's function without using that
identity as the product name. Hardware evidence is `research:instruments/sh-7.md`; product decisions
are `docs/briefs/mxm-para-07.md` and `plans/plan-mxm-para-07.md` (in the private archive).

MSRV is Rust 1.87. This crate owns the complete audio machine, tests, measurement harness and render
demo. It has no plugin-framework or UI concerns and **one runtime dependency**,
[`mxm-modulation`](https://github.com/mxm-audio/mxm-kit/blob/main/crates/mxm-modulation/AGENTS.md) — the collection's shared routing, itself
dependency-free and at this same floor. `cargo tree` is the check.

The evidence, research citations, measurements and owner rulings behind every rule below are in
[NOTES.md](NOTES.md).

## Ownership

- `src/keyboard.rs`: fixed-capacity overlapping-note ledger, highest/lowest writers and
  retained targets.
- `src/oscillator.rs`: two high-frequency master cores, uncleared divider chains, five VCO-1A
  registers, independently driven waveshapers, local antialiasing and core-only sync.
- `src/lfo.rs`: one LFO, its destination-specific offsets, sine-only delay and keyboard reset.
- `src/sh.rs`: independent S&H clock, source mux, hold droop and output lag.
- `src/envelope.rs`: two instances of the local exponential ADSR; routing restrictions belong to
  `voice.rs`.
- `src/filter.rs`: one local TPT/ZDF four-CA3080 cascade with diode-limited global feedback.
- `src/voice.rs`: signal graph, event-line ordering, portamento, routing, HPF, noise, ring,
  bender and activity contract.
- `src/routing.rs`: the instrument's route declaration — sources, targets, scales, laws, offers and
  the Init wiring — over `mxm-modulation`'s graph.
- `src/conformance.rs`: this routing as the collection's modulation standard checks it, behind the
  `conformance` feature that only `[dev-dependencies]` enable (this crate's tests and the plugin's).
- `examples/measure.rs`: console measurements; `examples/render_para_07.rs`: deterministic stereo WAV
  with the mono hardware output duplicated, and no added effect.

## Local Contracts

### Machine topology and event order

Full text and citations: [NOTES.md § Machine topology](NOTES.md#machine-topology-and-event-order).

- Every press is tracked. VCO-1 takes the highest held key, VCO-2 the lowest; one key doubles;
  middle notes affect the shared gate only. Targets collapse at once when an extreme is released and
  hold their final values after the last release.
- Host events match presses by `voice_id`, channel and key; each press also gets an internal
  sequence identity that owns retained high/low state. Id-less release, choke and tuning update the
  oldest matching press; duplicate note-ons stay separate. Capacity 32; an accepted extreme owner is
  never evicted.
- Target selection precedes the two independent portamento lags (NORMAL both ways, UP, DOWN).
  Retrigger reads the post-portamento high-key CV, never the host event.
- **Gate, key depression, external trigger, sine-delay dump, keyboard/LFO reset and auto-bend start
  are separate lines.** Only an ordinary keyboard-gate assertion dumps the sine delay and starts auto
  bend; every accepted depression restarts LFO phase when KYBD TRIG or an LFO-triggered envelope asks.
  S&H gate changes and the trigger input never reset the LFO, dump its delay or start auto bend.
  All Notes Off and a final Choke cancel queued keyboard and retrigger state, not an independent
  trigger; their release reaches the envelopes only when Host is the gate. A trigger while gate is
  low releases on the next sample. LFO envelope mode is selected gate AND LFO square.
- **There is no EXTERNAL key mode** (the owner, 2026-09-27): **the S&H clock is the Gate source,
  chosen in either key mode**. Shared per-channel controls follow the high owner in both modes.
- Per-note tuning and per-channel bend/expression follow the current extreme owner; a handoff never
  borrows the released owner's expression, and a whole-owner handoff between equal keys never
  re-arms retrigger.

### Audio graph

Full text and citations: [NOTES.md § Audio graph](NOTES.md#audio-graph).

- Both VCOs keep a 32x master and **uncleared** divider state; Range selects a tap. Sync resets
  VCO-2's core only, on VCO-1 core edges, never VCO-2's divider. Deliberately unusual and audible.
- VCO-1A's five taps stay phase-locked and level-normalised. Each waveshaper has its own CV-driven
  ramp reset only by its divider-output edge; PolyBLEP on saw and pulse edges; triangles, pulses and
  active ring paths run at 8× with box decimation, never touching divider or sync state.
- PWM is one-sided, 50% toward 10%, and always takes the undelayed positive LFO triangle. The LFO
  keeps the hardware's destination offsets (PLUS ≠ 0-CENTER).
- S&H has its own clock and source; the hold leaks with a chosen 30 s time constant
  (`the_hold_leaks_slowly`) and the output lag is a separate path. No UI state enters this crate.
- Mixer resistor ratios are preserved; **no post-mixer normaliser or limiter**.
- **There is no external input** (the owner, 2026-09-26): the ring is VCO-1 times VCO-2 and the
  mixer's fifth channel carries it, as fixed paths.
- HPF: one passive pole. VCF: four OTA poles, diode-limited resonance, **no invented compensation**.
  Filter tracking follows post-portamento high-key CV (Pedal mode keeps fixed 1 V/oct tracking).
- ENV-1 alone feeds VCF and PWM; ENV-2 only the VCA. **Zero A/D/R stays a 1 ms click; never
  de-click it away.**
- The one VCA sums selected envelope, HOLD, LFO 0-CENTER and bender CV, additively; its routed input
  is **VCA level**. No invented resonance-volume compensation.
- Bender DIRECT is signed; LFO mode rectifies bend magnitude. VCO endpoints are mode-local
  (±15 st direct, ±10 st LFO).

### The modulation standard

Sources and added routes mean what they mean on every instrument (`mxm_modulation::standard`).
Detail: [NOTES.md § The modulation standard](NOTES.md#the-modulation-standard).

- **Publishers.** Key is `(high key − 60) / 12` (`KEY_UNIT_SEMITONES`). **Velocity is `v − 1` of the
  press that last triggered an envelope** (`envelope_velocity`); an owner no press has set reads full.
- **Reach.** Machine pairs keep the machine's spans; every added pair takes the standard reach
  (`ADDED_SCALE` through `mxm_modulation::sum_split`, bit-identical while none is live). A pulse
  width is the standard's width (`PULSE_WIDTH_SWING`); the Amplitude and Bank level columns are
  normalised by each source's peak.
- **VCA level ← Key is refused**; VCA level ← Velocity is offered only its positive half.
- **Amplitude** is a target after the VCA (`standard::amplitude_factor`); **Bank level**
  (`target::BANK_LEVEL`) adds to `bank_level` before the mixer. Neither can open the VCA, and
  nothing routes to either at Init.
- **The multiplier is `mxm_modulation::product_with_tops`**, each factor neutral at its source's top
  (`PRODUCT_TOPS`) — zero for the standard Velocity, so a velocity factor is the number it was.
- **Init wires Pitch ← Bend on both VCOs at ±2 st** (`INIT_BEND`) and leaves the Sample and hold and
  Auto bend pitch depths absent (`INIT_PRESENT`): two owner-ruled Init deviations.
- `conformance.rs` runs the standard's checks — the declaration, the publishers, the velocity rule,
  the Amplitude factor and release silence — each falsified once.

### Intentional warts and uncertainty seams

Each is deliberate and cited in [NOTES.md § Warts](NOTES.md#intentional-warts-and-uncertainty-seams);
none may be "fixed":

- Phase-locked register bank, independently driven waveshaper ramps, core-only sync.
- High/low paraphony, middle-key gate life, upper-key tracking, target collapse, two directional lags.
- Retrigger downstream of glide, with a chosen threshold (a candidate wart).
- One-edge pulse width; no pulse width on the register bank.
- LFO waveforms with incompatible DC offsets; only sine is delayed.
- A drooping S&H hold (never an ideal digital hold), slight by choice.
- Filter-modulation selector exclusions; the S&H clock gates envelopes through the Gate source.
- No gain compensation or limiter for the register bank or the hot ring channel.
- One passive HPF pole; a load-dependent passive pink-noise approximation.
- No Q-compensation path; the resonance droop is a chosen topology result.
- ENV-2 VCA-only, and the zero-time click.
- Rectified bender LFO mode; auto bend only on detached gate assertion.
- A perfect switched-inversion LFO saw and a passive pink network: explicit measurement seams.

### Chosen and derived constants

Every constant absent from hardware evidence is chosen or derived, with its status, reason and
measurements, in the table at [NOTES.md § Chosen and derived constants](NOTES.md#chosen-and-derived-constants)
(the evidence rows the source comments point to). Keep it current when a constant moves.

### Realtime and numeric rules

Full text: [NOTES.md § Realtime and numeric rules](NOTES.md#realtime-and-numeric-rules-in-full).

- `Voice::process` performs no allocation, locking, I/O or formatting.
- Audio/state are `f32`; phase, lag and recursive coefficient calculations use `f64` where error
  accumulates.
- The plugin accepts only finite rates at or above 12 Hz; direct DSP calls clamp lower finite rates
  to that floor and replace non-finite ones with 48 kHz, before any other clamp bound is formed.
- Every public control is clamped at use. Non-finite per-note tuning, bend and expression updates
  are ignored. All finite inputs produce finite output bounded by `voice::OUTPUT_BOUND`.
- Subnormal state uses `flush`; release snaps to exact zero. Entering `Activity::Inert` silences both
  envelopes and settles state; inert calls return positive exact zero. Only the envelope connected
  to the VCA owns a tail.
- `all_sound_off` is a latch: it clears event lines and recursive state, keeps the press ledger, and
  wakes only on a fresh note, trigger, HOLD opening or S&H gate re-selection. `reset` restores
  deterministic cold state.
- No shared DSP may be extracted from this first copy.

## Work Guidance

- Preserve the signal order documented above. Do not turn this into a generic modulation matrix.
- Add a test that fails when removing each evidenced restriction or wart. Mark assumptions as chosen,
  approximated or unverified rather than laundering them into facts.
- Keep measurement code in this crate. Do not add effects: the source machine shipped with none.
- Examples write only original generated WAV data, through the collection's encoder `mxm-audio-file`
  — a test-only `[dev-dependencies]` edge. `[dependencies]` holds `mxm-modulation` alone.

## Verification

```bash
cargo test -p mxm-para-07-dsp
cargo run -p mxm-para-07-dsp --example measure --release
cargo run -p mxm-para-07-dsp --example render_para_07 --release
cargo clippy -p mxm-para-07-dsp --all-targets -- -D warnings
cargo fmt --all -- --check
```

What the unit suite covers, with its thresholds: [NOTES.md § Test coverage](NOTES.md#test-coverage).

## Child DOX Index

None.
