# mxm-para-07 — pre-conversion reference, captured at C0

`plans/plan-mxm-para-07-modulation.md` §10, captured before a line of routing exists. Everything here
comes from `plugins/mxm-para-07/src/lib.rs`'s `#[ignore]`d `baseline` module, which runs unchanged on
the converted tree:

```
cargo test -p mxm-para-07 --release --lib baseline -- --ignored --nocapture --test-threads=1
MXM_C0_DUMP=<dir> …the same…   # also writes every render as raw little-endian f32
```

**Not portable across machines.** Other programs were not controlled; the figures count only from a
quiet one.

## The seam

`mxm-para-07` had no plugin-side measurement seam. C0 adds one — `next_patch`, the trigger-input edge
refresh, `render_sample` — with `render_block_for_test` around it for the throughput probe. What is
left out is only the block-rate telemetry publish and the peak/overload accumulation, neither of
which the conversion touches.

**Not a second `process()`.** At C0 the two merely agreed, each holding its own copy of the same four
steps; C1 made `process` call the seam outright (now `render_one`), because a per-sample amount
advance added to one and not the other would have split the measured path from the host's silently.
Either way, a figure taken here is a figure the host pays.

## The score

Seventy-two blocks of 512 frames, the host golden's own shape
(`apps/mxm-player/tests/t4_golden_audio_para_07.rs`), so the two measure the same gestures: a low
press, a high press above it, a middle press that moves the shared gate but neither pitch, then the
extremes released in the order that collapses ownership. That is the two-pitch story this machine
exists to tell.

**Plus what the host golden does not send**: the pedal (CC 11) and the lever, and a 173 Hz tone at
0.25 into the external port throughout. Without the pedal, `keysource` in Pedal mode renders exactly
as Keyboard mode; without the lever, every bender route renders exactly as Off; without the external
tone the follower reads zero. `mxm-mono-08`'s M0 shipped with that last fault and rendered six of its
sounds byte-identically to one another before it was caught.

**The external port and the follower retired on 2026-09-26** (the owner removed the external input
and everything on it). The score no longer drives the port and the four `cutoff-follower-*` oracles
below are gone. Every other digest recorded here — Init, all fifty factory sounds and the remaining
focused paths — re-rendered bit-identical without it: at every factory setting the port reached only
the follower, whose Cutoff route sat at zero.

## What this reference is blind to, and necessarily so

**Velocity, the wheel and pressure.** They are the three MIDI paths the conversion *adds* (§3.1):
today `handle_event` ignores velocity, accepts and discards CC 1, and has no pressure path at all.
They have no before to be identical to, and C1 introduces them. Nothing else the machine can do is
left undriven.

## Throughput

Three passes each, same machine, same day, nothing else building. Two held keys.

| Case | Pass 1 | Pass 2 | Pass 3 |
|---|---|---|---|
| Init, 1× source render | 336.850 | 334.139 | 340.744 ns/sample |
| Init with a pulse VCO-1, 8× source render | 706.212 | 711.875 | 708.690 |
| `Audio contour` | 330.781 | 332.714 | 339.152 |
| `Delayed vibrato` | 336.715 | 328.528 | 339.055 |

**Both source paths, because they differ.** `render_sources` renders 1× for linear waveshapes with the
ring silent and 8× for a triangle, a pulse or an active ring (`voice.rs:870-880`). Init is the 1× case
— saw and square, and `fifth` at zero leaves `ring_active` false — so the 8× case is Init with a pulse
VCO-1. **The 8× path costs about 2.11× the 1× path, not 8×**, because only source rendering
oversamples. A routing sum measured only against the 8× figure would be a third as visible.

**Two routed patches, where §10 named one.** §10 asks for "a factory preset using filter audio
modulation and LFO pitch". **No shipped preset uses both**: `filteraudio` is non-zero only in
`audio-contour` and `filter-chatter`, LFO pitch only in `delayed-vibrato`, `glide-lead`, `moving-pad`,
`pulse-motion` and `slow-strings`, and the two sets are disjoint. Holding one of each serves the
intent, costs one more reading, and keeps both patches shipped rather than invented.

Spreads are 6.6 ns on Init, 5.7 ns on the 8× case, 8.4 and 10.5 ns on the routed patches — quiet
enough to compare against at C4.

## Factory bank — fifty-one reference digests

FNV-1a over the raw sample bits, the digest the player's golden uses. `Applied` is how many of the
80 stored ids each file carries.

| Sound | Digest | Peak | Applied |
|---|---|---|---|
| *(Init)* | `66f9453f59bc4b68` | 0.6560 | 0 |
| `open-saw` | `76f8c4e6f9084566` | 0.5893 | 80 |
| `split-octaves` | `959d01529de8cbfe` | 1.1149 | 80 |
| `wide-pair` | `89088f77bfd4b92e` | 1.1537 | 80 |
| `narrow-beating` | `486ffe8fd67849a5` | 1.1074 | 80 |
| `square-pair` | `66807bfafebee9f8` | 0.9239 | 80 |
| `triangle-air` | `a87bad4789a65c0d` | 0.5420 | 80 |
| `pulse-motion` | `602fdc5986a18a48` | 0.9321 | 80 |
| `envelope-pulse` | `b81a9a8f22a30c09` | 0.7728 | 80 |
| `core-lock` | `4b8ce60b7b2affcb` | 1.5549 | 80 |
| `broken-lock` | `584fcc67939d8dc3` | 0.6130 | 80 |
| `register-organ` | `629c1decc3123439` | 1.3115 | 80 |
| `deep-registers` | `79044510d888036e` | 1.2166 | 80 |
| `bright-registers` | `6d75df0cd477c50a` | 1.1028 | 80 |
| `hollow-stack` | `5d1e22ff5b9de19e` | 1.2738 | 80 |
| `full-stack` | `08fef4544c5d17cb` | 1.2397 | 80 |
| `ring-bell` | `5ad88a830dd94e73` | 0.8263 | 80 |
| `metallic-ring` | `0085ebb0f276a2db` | 1.6823 | 80 |
| `low-ring` | `95f8a0300014baba` | 1.1283 | 80 |
| `white-wind` | `e270e26e0c4bd5c6` | 0.1724 | 80 |
| `pink-breath` | `5742f07e91ce3121` | 0.1185 | 80 |
| `round-bass` | `1970c55019d61e1b` | 0.5445 | 80 |
| `register-bass` | `14c72569a060a464` | 0.5625 | 80 |
| `split-bass` | `ae6e673311b60b63` | 0.9668 | 80 |
| `tracking-bass` | `6d5a7ff70eb40ec0` | 0.3855 | 80 |
| `rubber-pulse` | `fd90d474d9b21e0e` | 0.9126 | 80 |
| `clear-lead` | `921abfc7b1e68cb3` | 0.9216 | 80 |
| `glide-lead` | `a05ded455fb92fbc` | 0.9180 | 80 |
| `upward-glide` | `a6625d7133667cf0` | 0.8602 | 80 |
| `downward-glide` | `590dbd198c04855a` | 0.8307 | 80 |
| `auto-rise` | `596fc58fc6d26a0e` | 0.5180 | 80 |
| `auto-fall` | `b0fcb06bcb90a6c6` | 0.4038 | 80 |
| `delayed-vibrato` | `65e2dff1d633fe4c` | 0.5571 | 80 |
| `stepped-pitch` | `61cc0214402b7af4` | 0.5049 | 80 |
| `stepped-filter` | `e2108ded2373f655` | 0.3271 | 80 |
| `clocked-gate` | `99bac65e384901e7` | 0.4855 | 80 |
| `random-drift` | `4d5568a139e51a09` | 0.4953 | 80 |
| `filter-chatter` | `dbb3eebeee3f6cc4` | 0.4083 | 80 |
| `audio-contour` | `2f8dd6d7a739c789` | 0.3085 | 80 |
| `soft-brass` | `79992045ae2132b6` | 1.1279 | 80 |
| `hard-brass` | `37d0d754990105dd` | 1.1420 | 80 |
| `slow-strings` | `b7f4a7c1607ca7d8` | 1.0257 | 80 |
| `square-pad` | `b6c2adc815431ccd` | 1.1003 | 80 |
| `moving-pad` | `63777565192318fd` | 1.2474 | 80 |
| `dual-envelope` | `2d3982a3b6a89d90` | 1.0561 | 80 |
| `second-envelope` | `8b22aa7a6c544ebf` | 0.5546 | 80 |
| `short-click` | `012923bda43aa957` | 0.4007 | 80 |
| `filter-whistle` | `7903da9f6ed5f1e8` | 0.7376 | 80 |
| `held-drone` | `51933fadf9396212` | 1.3456 | 80 |
| `clocked-drone` | `0180262cecbb8d9e` | 0.7091 | 80 |
| `noise-drone` | `9ff29749aed9ac82` | 0.1683 | 80 |

**Several peaks exceed unity** — `metallic-ring` reaches 1.6823 — which is this machine's mixer, not a
fault. Recorded so a later reader can tell a conversion that changes level from one that does not.

## Focused oracles — every §1 row at partial and full depth

These are the single-term paths §3.3 claims stay bit-identical through the conversion, so each needs a
before. Partial is normalised 0.5 and full is 1.0 on the parameter that carries the term; the
selectors that choose the path are set alongside it.

| Case | Digest | Peak |
|---|---|---|
| `vco1-pitch-lfo-partial` | `a8e9fba0573a396e` | 0.6442 |
| `vco1-pitch-lfo-full` | `4f832d3b215e4991` | 0.6498 |
| `vco2-pitch-lfo-partial` | `6c1b039e1e8f902d` | 1.2112 |
| `vco2-pitch-lfo-full` | `b85ade808d5c0d5e` | 1.2133 |
| `vco1-pitch-sh-partial` | `457b8e158442b04a` | 0.6384 |
| `vco1-pitch-sh-full` | `f65d05559ab983c9` | 0.6384 |
| `vco1-pitch-autobend-partial` | `dd14ee902fe3c51b` | 0.6286 |
| `vco1-pitch-autobend-full` | `004313d800d2e8de` | 0.6407 |
| `pitch-bend-direct-partial` | `99b1ab468119c921` | 0.6547 |
| `pitch-bend-direct-full` | `9b5c9317ccfeeb71` | 0.6399 |
| `pitch-bend-lfo-partial` | `b713dc78c2c155fd` | 0.6529 |
| `pitch-bend-lfo-full` | `835609c2008bfaed` | 0.6522 |
| `pw-lfo-triangle-partial` | `d017dfad4ef5617a` | 0.8309 |
| `pw-lfo-triangle-full` | `96fefd215d600fff` | 0.9500 |
| `pw-env1-partial` | `a4dca8663ac3ea25` | 0.8522 |
| `pw-env1-full` | `099b92c2e2f628d6` | 0.9999 |
| `cutoff-env1-positive-partial` | `217cfabc0264568e` | 0.6520 |
| `cutoff-env1-positive-full` | `49962c8a905bc68e` | 0.6520 |
| `cutoff-env1-inverted-partial` | `cfdd5da70628cd8a` | 0.3776 |
| `cutoff-env1-inverted-full` | `76fcd6c2dccbead4` | 0.2152 |
| `cutoff-lfo-partial` | `2deae45839afe274` | 0.5015 |
| `cutoff-lfo-full` | `9900ded695956be1` | 0.5135 |
| `cutoff-sh-partial` | `927de4c509321709` | 0.5452 |
| `cutoff-sh-full` | `c6613f24e4ac7678` | 0.5889 |
| `cutoff-key-partial` | `ae929651679c5058` | 0.4592 |
| `cutoff-key-full` | `fb9c8c88d430d0f9` | 0.4301 |
| `cutoff-pedal-partial` | `3ba79d040176ddbd` | 0.4404 |
| `cutoff-pedal-full` | `988d0083a914f9c5` | 0.4587 |
| `cutoff-vco2-audio-partial` | `14082895de72bb27` | 0.9933 |
| `cutoff-vco2-audio-full` | `5348a5574d8c65d7` | 1.0630 |
| `cutoff-noise-audio-partial` | `2d223b3014afcab9` | 0.6530 |
| `cutoff-noise-audio-full` | `cae6b50d5db961b6` | 0.8187 |
| `cutoff-follower-positive-partial` | `5a31ac1d5587b149` | 0.4848 |
| `cutoff-follower-positive-full` | `5827c06e0d34e915` | 0.5032 |
| `cutoff-follower-inverted-partial` | `f90019da1398d712` | 0.4786 |
| `cutoff-follower-inverted-full` | `fd817a3864448121` | 0.4774 |
| `cutoff-bend-direct-partial` | `d3169b86a797261f` | 0.4798 |
| `cutoff-bend-direct-full` | `52b05333b9420b3a` | 0.4798 |
| `cutoff-bend-lfo-partial` | `1cfdf87fa646db7f` | 0.4798 |
| `cutoff-bend-lfo-full` | `90b7c3de8e9fe175` | 0.4798 |
| `amp-hold-partial` | `f41eb565d0c76ec0` | 0.9640 |
| `amp-hold-full` | `112edac6897f580b` | 1.2938 |
| `amp-vca-lfo-partial` | `15af7e92ebf8176e` | 0.7810 |
| `amp-vca-lfo-full` | `e599ece2c1f912b9` | 1.0143 |
| `amp-bend-direct-partial` | `0e9adbb8cb4f7659` | 0.6560 |
| `amp-bend-direct-full` | `d68722d39475dd63` | 0.7557 |
| `amp-bend-lfo-partial` | `b1f943ad5c047dcb` | 0.6560 |
| `amp-bend-lfo-full` | `226e8e9c7a97969a` | 0.6560 |
| `amp-env2` | `56906c75d8410691` | 0.6629 |

`amp-env2` is §1's one row that is a connection rather than a depth — which envelope reaches the VCA —
so it is rendered once: a two-way selector has no partial position.

**The raw renders are not committed.** With `MXM_C0_DUMP` set the module writes each of these as raw
little-endian `f32`; at C4 they are regenerated from a worktree at this commit rather than stored,
which is `plan-mxm-mono-00-modulation.md`'s A0 shape and the reason the harness itself is committed.

## Two oracles that measured nothing, and how they were caught

**Both looked exactly like data.** This is why `the_focused_paths` now asserts that no case renders
Init's own digest and that no case renders alike at partial and full depth: a hundred digests cannot
be checked by eye, and a reference that cannot move is worse than none because it passes.

- **`vco2-pitch-lfo` rendered Init's digest at both depths.** VCO-2's mixer level is **0.0** at Init,
  so the oscillator is silent and modulating its pitch is inaudible. Fixed by raising `vco2` with the
  case, as `cutoff-vco2-audio` already did.
- **`cutoff-bend-direct` hashed identically at both depths.** Init's cutoff is 18 kHz against a
  ceiling of `min(20 kHz, 0.45 × fs)` — about 0.152 octaves of headroom, where that term alone reaches
  two octaves and the filter envelope four. Both depths pinned the filter at its ceiling, so the
  oracle measured the clamp. **Every cutoff case was compromised, not just the one that announced
  itself**: the others differed only because their sources swing negative or start from zero and so
  spent some time below the ceiling. All twelve now open the filter part-way first, and all twelve
  digests moved when they did.

## What this does not establish

- **Nothing about how it sounds.** A digest proves *unchanged*; it cannot prove *good*.
- **Nothing about the host path, except by sharing the golden's gestures.** The bank is the plugin
  library, in process; the golden is the real bundle in MXM Player.
- **Nothing about velocity, the wheel or pressure**, which do not exist yet.
- **Nothing about the editor**, whose cards and coverage the conversion also moves (plan §7, C3).
