//! Console measurement harness for the evidence-backed DSP seams.
//! Run with `cargo run -p mxm-para-07-dsp --example measure --release`.
#![allow(clippy::field_reassign_with_default)] // the report exposes one route at a time
use mxm_para_07_dsp::envelope::{Adsr, Stage};
use mxm_para_07_dsp::filter::{K_MAX, Ladder};
use mxm_para_07_dsp::keyboard::NoteId;
use mxm_para_07_dsp::oscillator::{Range, Vco, Waveform};
use mxm_para_07_dsp::voice::{OscPatch, Patch, Voice, midi_hz};

const FS: f32 = 48_000.0;
fn note(v: i32, key: u8) -> NoteId {
    NoteId {
        voice_id: Some(v),
        channel: 0,
        key,
    }
}
fn tone_amplitude(x: &[f32], hz: f32) -> f64 {
    let mut re = 0.0;
    let mut im = 0.0;
    for (i, &s) in x.iter().enumerate() {
        let a = std::f64::consts::TAU * f64::from(hz) * i as f64 / f64::from(FS);
        re += f64::from(s) * a.cos();
        im -= f64::from(s) * a.sin();
    }
    2.0 * (re * re + im * im).sqrt() / x.len() as f64
}

fn main() {
    println!("mxm-para-07 DSP measurement @ {FS:.0} Hz");

    let mut vco = Vco::new(FS);
    let mut rises = 0;
    let mut was = false;
    for _ in 0..FS as usize {
        let y = vco.render(Range::Feet8, Waveform::Square, 0.5);
        let high = y > 0.0;
        rises += (high && !was) as usize;
        was = high;
        vco.advance_master(440.0);
    }
    println!("VCO 8' divider: {rises} rising edges for a 440 Hz pitch target (expected 439..441)");
    assert!((439..=441).contains(&rises));

    let mut env = Adsr::new();
    env.set_sample_rate(FS);
    env.trigger();
    let mut attack = 0;
    while env.stage() == Stage::Attack {
        env.process(0.1, 0.2, 0.5, 0.3);
        attack += 1;
    }
    env.release();
    let mut release = 0;
    while env.is_active() && release < FS as usize * 3 {
        env.process(0.1, 0.2, 0.5, 0.3);
        release += 1;
    }
    println!(
        "ENV: attack {:.2} ms, release-to-exact-zero {:.2} ms",
        1000.0 * attack as f32 / FS,
        1000.0 * release as f32 / FS
    );
    assert!((90.0..110.0).contains(&(1000.0 * attack as f32 / FS)));
    assert_eq!(env.process(0.1, 0.2, 0.5, 0.3), 0.0);

    let mut f = Ladder::matched();
    let input = 0.001;
    let resonance = 0.5;
    let mut y = 0.0;
    for _ in 0..96_000 {
        y = f.process(input, 4000.0, resonance, FS);
    }
    let measured = y / input;
    let expected = 1.0 / (1.0 + resonance * K_MAX);
    println!("VCF no-Q-comp DC gain: {measured:.4} (1/(1+k) = {expected:.4})");
    assert!((measured - expected).abs() < 0.01);

    let mut voice = Voice::new(FS);
    voice.note_on(note(1, 48), 0.0);
    voice.note_on(note(2, 72), 0.0);
    let p = Patch::default();
    voice.process(&p);
    let a = voice.assignment();
    println!(
        "split keyboard: high={}, low={}",
        a.high.unwrap().id.key,
        a.low.unwrap().id.key
    );
    assert_eq!((a.high.unwrap().id.key, a.low.unwrap().id.key), (72, 48));

    // The ring modulator is VCO-1 times VCO-2: a saw at the resting key against a triangle a fifth
    // above it, heard through the mixer's fifth channel alone.
    let mut voice = Voice::new(FS);
    let mut p = Patch::default();
    p.hold = 1.0;
    p.osc1_level = 0.0;
    p.osc2_level = 0.0;
    p.noise_level = 0.0;
    p.fifth_level = 0.5;
    p.osc2 = OscPatch {
        waveform: Waveform::Triangle,
        ..OscPatch::default()
    };
    p.vco2_tune_semitones = 7.0;
    p.cutoff_hz = 20_000.0;
    p.hpf_hz = 10.0;
    p.volume = 0.35;
    voice.note_on(note(1, 60), 0.0);
    let vco1_hz = midi_hz(60.0);
    let vco2_hz = midi_hz(67.0);
    for _ in 0..FS as usize {
        voice.process(&p);
    }
    let n = FS as usize * 2;
    let ring: Vec<f32> = (0..n).map(|_| voice.process(&p)).collect();
    let difference = tone_amplitude(&ring, vco2_hz - vco1_hz);
    let sum = tone_amplitude(&ring, vco2_hz + vco1_hz);
    println!(
        "ring modulator: difference {:.5} @ {:.2} Hz, sum {:.5} @ {:.2} Hz",
        difference,
        vco2_hz - vco1_hz,
        sum,
        vco2_hz + vco1_hz
    );
    assert!(difference > 1e-4 && sum > 1e-4);

    println!(
        "PASS — run the unit suite for alias, sync/divider, modulation-line, sample-rate, panic and idle-silence guards"
    );
}
