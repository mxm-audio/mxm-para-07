# mxm-para-07

A two-pitch paraphonic CLAP synthesizer: the highest and lowest held keys drive two divider-derived
oscillator pitches, then every source passes through one shared high-pass filter, resonant low-pass
filter, envelope pair and amplifier. Architecture inspired by the Roland SH-7; the product identity
and interface are original. Not affiliated with or endorsed by Roland.

The five-register oscillator bank, core-only sync, ring modulation (VCO-1 times VCO-2), independent
sample-and-hold, directional portamento and audio-rate filter modulation are all part of the
instrument. It adds no effects, sequencer or arpeggiator. The original's external audio input, with
its sensitivity switch and envelope follower, is not: the owner removed it on 2026-09-26.

Two audio layouts are advertised, stereo and mono, with no audio input. Stereo output is
bit-identical dual mono. The stereo layout is what MXM Player discovers without product-specific
code.

The Init patch sounds VCO-1 saw through an open filter. Every modulation amount starts at zero;
VCO-2 is silent and seven cents sharp so raising it immediately reveals beating. Fifty categorized
factory sounds are compiled into the plugin. Init is generated from parameter defaults and has no
file to drift or delete.

The software-native editor lays eighteen cards out on pages derived from the window: two-pitch
assignment, portamento and Setup first, then the modulation sources and their destination depths,
the oscillators and register bank, and the shared mixer and filter. The master volume sits in the
app bar beside the output meter, on every page. Rows keep the parallel oscillator and envelope pairs
together whenever a row can hold them, and a lone card is width-capped. It opens at the quarter-4K
budget hugged to its cards; smaller or increased-zoom windows keep every card reachable through more
pages and scrolling. The keyboard cursor, the shared
preset browser, independent 75–200% zoom, the collection's theme control, two-pitch and S&H
telemetry, and the leaf-green identity accent are persistent editor features.

**Fidelity is UNVERIFIED.** The software model is measured and tested, but no working hardware unit
or trustworthy reference recording has been compared.

## Building

```bash
cargo test -p mxm-para-07
cargo xtask bundle mxm-para-07 --release
clap-validator validate "target/bundled/mxm-para-07.clap"
```

MIT licensed — see [LICENSE](LICENSE). All implementation code is original.
