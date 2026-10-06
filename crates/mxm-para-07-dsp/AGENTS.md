# AGENTS.md — mxm-para-07 DSP

## Purpose

Framework-free DSP for `mxm-para-07`: two keyed pitches through one shared mixer, HPF, resonant
low-pass, dual envelopes and VCA. It implements the Roland SH-7's function without using that
identity as the product name. Hardware evidence is `research:instruments/sh-7.md`; product decisions
are `docs/briefs/mxm-para-07.md` and `plans/plan-mxm-para-07.md`.

MSRV is Rust 1.87. This crate owns the complete audio machine, tests, measurement harness and render
demo. It has no plugin-framework or UI concerns and **one runtime dependency**,
[`mxm-modulation`](https://github.com/mxm-audio/mxm-kit/blob/main/crates/mxm-modulation/AGENTS.md) — the collection's shared routing, itself
dependency-free and at this same floor. `cargo tree` is the check.

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

- Every press is tracked. VCO-1 takes the highest held key and VCO-2 the lowest; one key doubles;
  middle notes affect the shared gate but neither pitch. Targets collapse immediately when an
  extreme is released and remain at their final values after the last release
  (`research:instruments/sh-7.md` §§4.3, 4.4).
- `voice_id`, channel and key match host events to presses. Every accepted press also receives an
  internal sequence identity, which owns retained high/low state even when ID-less
  duplicates have identical host identities. Id-less release, choke and tuning update the oldest
  matching press; duplicate note-ons remain separate. Capacity is 32 and an accepted extreme owner
  is never evicted.
- Keyboard target selection precedes the two independent portamento lags. NORMAL slews both ways,
  UP only upward and DOWN only downward. Retrigger differentiation reads the post-portamento
  high-key CV, never the host event (`research:instruments/sh-7.md` §§4.5, 4.6).
- The gate, keyboard depression, external trigger, sine-delay dump, keyboard/LFO-reset and
  auto-bend-start are separate lines. An ordinary keyboard-gate assertion dumps the sine-delay
  capacitor and starts auto bend; every accepted physical-key depression—including a legato
  extreme, middle key or duplicate—restarts LFO phase when KYBD TRIG is enabled or an envelope
  selects LFO triggering, without reasserting those gate-only consumers. S&H gate changes and the
  discrete trigger input may trigger envelopes but must not reset the LFO, dump its delay or start
  auto-bend.
  All Notes Off and a final Choke cancel queued keyboard assertion and post-portamento
  retrigger state without consuming an independent discrete trigger. Their release or cut reaches
  the envelopes only when Host is the selected gate; S&H articulation remains independent
  at both clock levels. A trigger pulse while gate is
  low releases on the next sample rather than latching sustain. LFO envelope mode is selected gate
  AND LFO square (`research:instruments/sh-7.md` §§4.3, 4.7, 8.2).
- **There is no EXTERNAL key mode** (the owner, 2026-09-27). The machine's EXT CV/GATE position
  took both VCOs from the rear CV jack and the envelopes from the GATE jack, whose empty contact
  carried the S/H clock (`research:instruments/sh-7.md` §4.6). The plug-in has no jacks — its notes
  are the keyboard — so the mode and the keyboard's last-note writer retired; **the S&H clock is
  the Gate source, chosen in either key mode**, a plug-in extension. The shared per-channel controls
  (bend, pedal, velocity) follow the high owner in both modes.
- Per-note tuning and per-channel bend/expression follow the current extreme owner. A handoff must
  not borrow expression from the released owner. Whole-owner changes are not pitch edges: release
  or choke handoff between equal-key owners must not re-arm retrigger, even while portamento is
  still moving after the real target-change edge was consumed.

### Audio graph

- Both VCOs retain a 32x high-frequency master and uncleared binary divider state. Range selects a
  divider tap. Sync resets VCO-2's core only on VCO-1 core edges; it does not reset VCO-2's divider
  (`research:instruments/sh-7.md` §§4.1, 4.2). This is deliberately unusual and audible.
- VCO-1A's five register taps remain phase-locked and are level-normalised before the bank mixer.
  Each audible saw/triangle/pulse waveshaper has a CV-driven ramp independent of the master-core
  capacitor; its selected divider-output edge alone resets it. PolyBLEP corrects saw and both pulse
  edges locally; triangle folds that independently driven ramp. The voice evaluates folded
  triangles, pulses and active ring paths at 8× and box-decimates them, so triangle corners, moving
  narrow PWM edges and later ring products are filtered without changing divider or core-only-sync
  state.
- PWM is one-sided, 50% toward 10%, with manual/LFO/ENV-1 source selection. PWM always takes the
  undelayed positive LFO triangle (`research:instruments/sh-7.md` §§5.2, 6.1).
- The LFO preserves the hardware's destination offsets: PLUS and 0-CENTER are different. Keyboard
  gate dumps the sine-only delay independently of phase; optional keyboard reset starts all shapes
  at their documented high phase
  (`research:instruments/sh-7.md` §6.1).
- S&H has its own 13 ms–2 s clock and source selector. The held voltage intentionally leaks toward
  zero, **slowly: a chosen 30 s time constant** (the owner, 2026-09-27), and the output lag is a
  separate 500 kohm/1 uF path (`research:instruments/sh-7.md` §6.2). The research's derived 0.7 s
  lost a quarter of every step at the default clock; `the_hold_leaks_slowly` holds the new amount. `Voice::Telemetry` exposes the selected source, held
  voltage and lagged output for the plugin's atomic editor telemetry; no UI state enters this crate.
- Mixer resistor ratios are preserved: register bank 100/82, ordinary VCO/noise unity, ring
  100/33. There is no post-mixer normaliser or limiter; `Telemetry::overload` reports the chosen
  warning threshold (`research:instruments/sh-7.md` §§7.1, 7.2).
- **There is no external input** (the owner, 2026-09-26). The machine's EXT IN, its sensitivity
  switch and its envelope follower were removed with everything on them. The ring modulator is
  VCO-1 times VCO-2 and the mixer's fifth channel carries it — the defaults the removed switches
  had, now fixed paths.
- The HPF is one passive first-order pole before the low-pass. The VCF is four discrete OTA poles
  with diode-limited global resonance and no invented compensation
  (`research:instruments/sh-7.md` §§7.4, 7.5). Filter tracking follows post-portamento high-key CV:
  Keyboard mode scales that contribution, while Pedal mode keeps fixed 1 V/oct high-key tracking
  and scales only the retained owner's expression voltage (`research:instruments/sh-7.md` §3.9).
- ENV-1 alone feeds VCF and PWM. ENV-2 can feed only the VCA. Zero A/D/R remains a 1 ms pulse/click;
  never de-click it away (`research:instruments/sh-7.md` §6.3).
- The one VCA sums selected envelope, HOLD, LFO 0-CENTER and bender CV. It has no invented
  resonance-volume compensation (`research:instruments/sh-7.md` §§7.5, 7.6). Its routed input is
  named for what it is — **VCA level** — and keeps the machine's additive law.
- Bender DIRECT is signed; LFO mode rectifies bend magnitude before scaling the shared 0-CENTER LFO
  signal independently at VCO, VCF and VCA. The evidenced VCO endpoint is mode-local: ±15
  semitones for direct CV and ±10 semitones for a full-scale LFO signal
  (`research:instruments/sh-7.md` §§3.12, 4.7, 6.1).

### The modulation standard

The collection's performance sources and added routes mean here what they mean on every instrument
(`mxm_modulation::standard`; `plans/plan-modulation-standard.md`).

- **Publishers.** Key is `(high key − 60) / 12` (`KEY_UNIT_SEMITONES`, the ⅛-octave frame unchanged);
  **Velocity is `v − 1` of the press that last triggered an envelope** (`envelope_velocity`, latched
  at every ENV-1/ENV-2 trigger including an external retrigger, from the owner the trigger follows)
  — so under GATE a legato press keeps the phrase's. An owner no press has set reads full (the
  keyboard's default owner), so an envelope the S&H clock fires with no key rests;
  the wheel, pressure, lever and the pedal (published as a wheel) go through their standard
  publishers.
- **Reach.** The machine's own pairs keep the machine's spans. Every added pair takes the standard:
  pitch 12 st (it was D9's 24), Pitch ← Key 12 st/oct, cutoff 4 oct. **A pulse width is the
  standard's width**: its control `m` moves the pulse `PULSE_WIDTH_SWING` (40 %) of the cycle a unit,
  the machine's own PWM pairs stay on `m`'s column, and every added performance pair takes 45 % of the
  cycle (9 %/oct from Key) — which the one-edge pulse, narrowing only to a tenth, reaches at 89 % of
  the travel (`ADDED_SCALE`, through `mxm_modulation::sum_split`, bit-identical to the uniform sum
  while no such pair is live). Readings are in percent of the cycle. The Amplitude column is
  normalised by each source's peak, so the noise and the oscillators' audio reach exactly the
  standard swing too.
- **VCA level is the machine's CV amplifier, so it is offered by the standard's rule**: VCA level ←
  Key is refused (a held key would latch the voice open), and VCA level ← Velocity is offered only
  its positive half — a route that can only close the amplifier, never open it (`parked_value`
  returns zero for Velocity). VCA level ← the lever, wheel, pressure and pedal are the machine's own
  amplifier held open by a parked control, and are the release-silence check's declared drones.
- **Amplitude is a new target after the VCA**: `standard::amplitude_factor`, `1 + clamp(Σ, ±1)`,
  multiplied into the output before the `OUTPUT_BOUND` clamp — it cannot open a closed VCA, so
  activity is unchanged. Nothing routes to it at Init.
- **The register bank's Level is a target** (`target::BANK_LEVEL`, the owner, 2026-09-27: not on
  the machine): its sum adds to `bank_level` in the level's own 0…1 and is clamped there, before the
  mixer — bit-identical to the knob alone while nothing is routed, and unable to open the VCA, so
  activity is unchanged. The standard's control (`Kind::Control`), every pair at the standard reach
  through `BANK_LEVEL_SCALE`, normalised by each source's peak as Amplitude's column is
  (`every_bank_level_route_reaches_the_standard_reach_at_its_sources_peak`,
  `the_register_bank_obeys_its_level_routes`, both falsified). Nothing routes to it at Init, and the
  bank is silent at Init anyway: every register starts at zero.
- **The multiplier is `mxm_modulation::product_with_tops`**, each factor neutral at its source's top
  (`PRODUCT_TOPS`) — zero for the standard Velocity, so a velocity factor is the number it was.
- **Init wires Pitch ← Bend on both VCOs at ±2 st** (`INIT_BEND`, the owner's ruling 7), so the
  lever does what a player expects on the first patch. It is an Init deviation from the machine's
  panel, and every factory sound inherits it. **The Sample and hold and Auto bend pitch depths are
  not present at Init** (`INIT_PRESENT`; the owner, 2026-09-27: *just leave LFO*), a second
  deviation: each Pitch stack opens with LFO and Bend. Both pairs stay offered, at zero they were
  silent anyway, and the four factory sounds that use them set their presence themselves.
- `conformance.rs` runs the standard's checks — the declaration, the publishers, the velocity rule,
  the Amplitude factor and release silence — each falsified once.

### Intentional warts and uncertainty seams

- **The VCO-1A register bank stays phase-locked; every audible waveshaper ramp is driven by pitch CV
  independently of the core capacitor and resets only at its selected divider-output edge; range
  selects a tap; sync resets only the high-frequency slave core.** Divider and waveshaper state
  survive a sync edge that does not clock the divider (`research:instruments/sh-7.md` §§5.2–5.6;
  §9 warts 1–5).
- **High/low paraphony, middle-key gate life, upper-key filter tracking, target collapse and two
  directional lags** are circuit behaviour, not voice-allocation bugs
  (`research:instruments/sh-7.md` §§4.2–4.4; §9 warts 6–10).
- **Retrigger is downstream of glide.** The differentiator threshold is chosen, so very slow motion
  can delay or suppress retrigger. This remains a candidate wart until a unit settles it
  (`research:instruments/sh-7.md` §§4.5, 11).
- **Pulse width moves one edge only;** VCO-1A's fixed register bank has no pulse-width control
  (`research:instruments/sh-7.md` §§5.3, 5.5; §9 wart 16).
- **LFO waveforms have incompatible DC offsets and only sine is delayed**
  (`research:instruments/sh-7.md` §6.1; §9 warts 13–15).
- **S&H has an independent clock/source mux and its hold droops;** replacing the capacitor with an
  ideal digital hold is wrong (`research:instruments/sh-7.md` §6.2; §9 wart 18) — but the droop is
  slight: flat at the default clock, a few percent across the slowest.
- **Filter modulation keeps the LFO/S&H and VCO-2/noise selector exclusions, and the S&H clock gates
  envelopes only through EXTERNAL mode's unconnected-gate position** on the machine; here through the
  Gate source, in either key mode (the owner, 2026-09-27)
  (`research:instruments/sh-7.md` §§3.9, 4.6; §9 warts 17, 19).
- **The level-normalised register bank and disproportionately hot ring mixer channel** get
  no convenience gain compensation or limiter (`research:instruments/sh-7.md` §§5.3, 7.1–7.2; §9
  warts 20–21).
- **The HPF stays one passive pole and pink noise stays a load-dependent passive approximation,**
  not a steeper high-pass or ideal −3 dB/oct source (`research:instruments/sh-7.md` §§7.3, 7.4; §9
  warts 22–23).
- **No Q-compensation path was found.** The model deliberately adds none; its exact resonance droop
  remains a chosen topology result pending a unit sweep, not a claimed measurement
  (`research:instruments/sh-7.md` §§7.5, 11).
- **ENV-2's VCA-only restriction and the documented zero-time click are intentional**
  (`research:instruments/sh-7.md` §6.3; §9 wart 11).
- **The bender LFO mode rectifies its control and auto-bend fires only on detached gate assertion,**
  never release, retrigger or legato (`research:instruments/sh-7.md` §§4.7, 5.7; §9 warts 24, 26).
- A perfect switched-inversion LFO saw is used; doubled rate or a reset seam is not invented. Pink
  noise uses a firm passive-network approximation, not a claimed measured transfer. These are
  explicit measurement seams (`research:instruments/sh-7.md` §§6.1, 11).

### Chosen and derived constants

| Constant / choice | Status and reason |
|---|---|
| Keyboard capacity 32 | Chosen fixed bound; enough for overlapping MIDI while preserving realtime operation. |
| DSP sample-rate floor 12 Hz | Chosen integer margin above the 11.12 Hz boundary where the 5 Hz VCO/filter floor meets the 0.45×Nyquist ceiling; the plugin rejects lower rates and direct DSP calls clamp safely. |
| VCO master 32x and five taps | Derived from the divider architecture and 2'/4'/8'/16'/32' relation. |
| VCO 5 Hz safety floor, 0.45×Nyquist ceiling, edge capacity 8, bound 2 | Chosen numeric guardrails; capacity is conservatively above the 3.6 core wraps/sample admitted by the ceiling. |
| VCO shaper-slope trim default unity, allowed 0.8–1.2 | Chosen no-error default pending a measured unit; it changes ramp height/fold/pulse crossing while direct square remains untrimmed. |
| PolyBLEP at each local edge | Chosen antialiasing technique for saw and pulse discontinuities. |
| 8× source/ring oversampling with box decimation | Chosen fixed-cost completion for folded triangle, modulated narrow PWM and ordinary/synced ring products. Tests compare it with both 1× trivial and 64× reference spectra, requiring >18 dB improvement, >90% total wanted energy and >80% upper-band wanted energy; measured minima are 22.91 dB, 99.9% and 98.2% respectively, above a −103.12 dB known-clean-sine harness floor. |
| LFO 0.2–25 Hz, delay max 3 s | Published limits. Delay's three-tau mapping and triangle rounding coefficient 0.18 are chosen. |
| Total tune ±3.5 st; VCO-2 tune ±7.5 st | Service-spec limits, clamped in the audio path. |
| Portamento 0–3 s interpreted as one RC tau | Range/topology published; endpoint interpretation chosen pending a glide trace. |
| Cold targets key 60/channel 0 | Chosen deterministic pre-event calibration required by the plan; replaced by the first accepted writer. |
| S&H 30 s droop, 0.5 s max lag tau | Droop **chosen** (the owner, 2026-09-27), pending a measured unit: the derived 0.068 uF×10 Mohm = 0.68 s took the 10 Mohm the research lists with the sample switch (Q203) as a bleed, where it more likely biases the JFET's gate, and lost 95 % across the slowest clock where the research describes a pitch "not perfectly flat". Lag derived from 1 uF×500 kohm; 2 s control endpoint is four taus. |
| Envelope 1 ms floor; A max 4 s, D/R max 8 s | Published limits and click behaviour. -80 dB idle snap, 0.2 attack overshoot and 1% timing convention are chosen numeric policy. |
| Filter `K_MAX=5` | Derived calibration: ideal onset `k=4` maps to slider 8, inside the published 7–9 adjustment. |
| Filter knee 1, excitation `1e-6`, excitation threshold 0.75 | Chosen bounded nonlinear-loop policy; threshold remains below onset. |
| Filter stage spread ±1.5%, seed `0xCA30_8007` | Chosen deterministic discrete-unit calibration pending a working-unit sweep. |
| Four Newton steps from the linear small-signal root; 0.45×sample-rate cutoff ceiling | Chosen fixed-cost convergence and tangent safety policies. Against a 96-bisection `f64` root oracle over 12 Hz–768 kHz representative rates, cutoff/resonance boundaries and opposite-sign input/state transitions, the measured worst residual is `9.536743e-7` and worst root error is `4.880996e-7`; three steps from the same stable seed reach `1.7881393e-6`, while the old stale-output seed reaches `5.2075863`. |
| Filter input knee 8 and output bound 15.5 | Chosen late safety rail; the bound combines analytic terms with 2.5 transient headroom. |
| Register normalisation `2Σ(gᵢxᵢ)/(1+Σgᵢ)` | Chosen topology-derived approximation of the dual-gang network: one standing ladder load plus loading proportional to aggregate slider opening, calibrated so one fully raised register is unity. It is continuous at zero and bounds five fully raised registers to 1.67× an aligned single-register sample rather than 5×. |
| Pink corners 2.34/10.3 kHz; direct/fast/slow weights 0.18/0.35/0.47 and gain 1.25 | Corners derived from documented passive parts; loaded shelf weights/gain chosen pending measurement. |
| Mod spans: LFO 12 st, S&H/auto 24 st, VCF ENV/mod 4 oct, audio 2 oct | Chosen useful full-depth mappings pending panel-voltage calibration. ENV-1's 120k/47k weight is derived. Routes the machine never had take the collection's standard reach instead (*The modulation standard*). |
| Bender VCO ±15 st direct CV, ±10 st LFO; direct VCF 2 oct | VCO spans are published service-spec limits and mode-specific; the VCF mapping is chosen from the documented route. |
| Auto-bend 20–700 ms and 24 st | Published time range; full-depth span and 1%-settling convention chosen. |
| Retrigger threshold 0.002 semitone/sample | Chosen explicit uncertainty seam; downstream placement is evidenced. |
| Mixer warning at magnitude 2 | Chosen telemetry threshold only; it does not alter audio. |
| VCA linear additive CV, clamped 0–3; linear total volume | Chosen pending gain-CV and panel-level traces; documented dB spans do not establish an exponential law. |
| Whole-voice bound `filter bound × 3` | Derived from filter bound and maximum summed VCA CV. |
| Deterministic RNG seeds `0x0007_1978`, `0x5EED_1101` | Chosen identities for noise and resonance excitation; reproducibility, not measured randomness. |
| Flush threshold `1e-20` | Chosen far-below-audibility denormal policy used by recursive state. |
| `Patch::default` Init calibration | Chosen usable baseline: Two-pitch/Host, no glide/modulation/bend/hold, 8' saw + 8' square at 0.7/0, VCO-2 +0.07 st, HPF 10 Hz, VCF 18 kHz/Q 0, ENV-1 5/200/0.7/200 ms, LFO sine 5 Hz, S&H random 200 ms, volume 0.8. It is not claimed as a factory hardware patch. Its routing adds Pitch ← Bend ±2 st on both VCOs (`INIT_BEND`) and leaves out each VCO's Sample and hold and Auto bend depths, both owner-ruled deviations. |

### Realtime and numeric rules

- `Voice::process` performs no allocation, locking, I/O or formatting.
- Audio/state are `f32`; phase, lag and recursive coefficient calculations use `f64` where error
  accumulates.
- The plugin accepts only finite sample rates at or above 12 Hz. Direct framework-free construction,
  filter processing and public voice/VCO rate setters clamp lower finite rates to that floor and
  replace non-finite rates with 48 kHz, before any cutoff or pitch clamp bounds are formed.
- Every public control is clamped at use. Non-finite per-note tuning, bend and expression updates
  are ignored. All finite inputs produce finite output bounded by `voice::OUTPUT_BOUND`.
- Subnormal recursive state uses `flush`. Release snaps envelopes to exact zero. On the sample that
  enters `Activity::Inert`, both envelopes are silenced and recursive state is settled before the
  host can sleep; later selecting a formerly disconnected envelope cannot revive a frozen release.
  Inert `process` calls return positive exact zero. Only the envelope currently connected to the
  VCA owns an audible tail or its duration; a disconnected envelope is silenced at inert rather than
  processed merely to finish an inaudible release.
- `all_sound_off` is a latch: held notes and autonomous controls do not immediately resurrect sound;
  it clears queued keyboard, external-trigger and post-portamento-retrigger event lines, settles pitch
  lag, and clears VCO/LFO, RNG, pink-network, S&H held/lag, filter and HPF state, while retaining
  the press ledger for later event matching. A fresh note, trigger,
  HOLD opening, or leaving and re-entering the S&H gate selection can wake it;
  selector history therefore advances
  while the latch is set. `reset` restores deterministic cold state.
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

The unit suite covers exact idle silence, selector changes after idle, reset/panic, determinism,
denormal snapping, finite bounded processing at and below the 12 Hz floor plus extreme sweeps from
1 kHz through 768 kHz, envelope timing, filter fixed-step residual against a converged reference
across 12 Hz–768 kHz representative rates and abrupt boundary transitions, filter
response/onset/droop, oscillator
alias and wanted-spectrum retention (including every waveshaper, moving narrow PWM and 1×/8×/64×
ordinary/synced-lock/synced-failure ring comparisons), divider/sync state, direct core-reset
sabotage plus divided-lock and no-divider-clock audio regimes, LFO/S&H restrictions, keyboard
overlap, held extreme/middle/duplicate depression LFO restart, portamento/retrigger (including
release and choke handoff between equal-key owners during an unfinished glide after the legitimate
edge), event-line separation, routing, the ring path and bender modes, including
independent literal full-depth and 40%-depth VCO span checks for direct CV and LFO modes.
ID-less identical-duplicate tuning proves that the oldest press's lanes retune and hand to the
newer press untuned. `the_sample_hold_clock_gates_the_envelopes_in_either_key_mode` holds the Gate
source in both key modes. Register-bank
scores cover zero-crossing continuity, single-slider level control and all-register loading as well
as the routed endpoint. S&H-gate termination tests compare All Notes Off and final Choke at both
clock levels against untouched articulation, with Host release/cut controls. Panic tests compare
unequal pitch-lag, pink and S&H histories after wake while matching retained note-event ownership; the
zero-control envelope test uses an explicit 10 ms de-click sabotage oracle and fixed
48 kHz onset gates (>0.05 peak sample delta and >12 early-window absolute sum), below measured
unsmoothed values 0.077 and 18.98 but above sabotaged values 0.0082 and 4.34.

## Child DOX Index

None.
