//! Render a short evidence tour to `mxm-para-07-demo.wav`.
//! Run with `cargo run -p mxm-para-07-dsp --example render_para_07`.
#![allow(clippy::field_reassign_with_default)] // each assignment reads as a score setting
use mxm_para_07_dsp::keyboard::NoteId;
use mxm_para_07_dsp::oscillator::{Range, Waveform};
use mxm_para_07_dsp::routing::{self, Routing};
use mxm_para_07_dsp::voice::{NoiseColour, Patch, PortamentoMode, TriggerMode, Voice};

fn note(v: i32, key: u8) -> NoteId {
    NoteId {
        voice_id: Some(v),
        channel: 0,
        key,
    }
}
fn main() -> std::io::Result<()> {
    const FS: u32 = 48_000;
    // Seven seconds of stereo at 48 kHz, written once through the collection's encoder.
    let mut out: Vec<f32> = Vec::with_capacity(FS as usize * 7 * 2);
    let mut voice = Voice::new(FS as f32);
    let mut p = Patch::default();
    p.osc1.waveform = Waveform::Saw;
    p.osc2.waveform = Waveform::Pulse;
    p.osc1.range = Range::Feet8;
    p.osc2.range = Range::Feet4;
    p.osc1_level = 0.7;
    p.osc2_level = 0.55;
    p.register_levels = [0.18, 0.35, 1.0, 0.55, 0.2];
    p.bank_level = 0.35;
    p.noise_colour = NoiseColour::Pink;
    p.noise_level = 0.035;
    p.cutoff_hz = 420.0;
    p.resonance = 0.72;
    p.env1.attack = 0.012;
    p.env1.decay = 0.34;
    p.env1.sustain = 0.48;
    p.env1.release = 0.75;
    p.env2.attack = 0.08;
    p.env2.decay = 0.6;
    p.env2.sustain = 0.75;
    p.env2.release = 1.0;
    p.env2.trigger = TriggerMode::Gate;
    p.portamento_s = 0.055;
    p.portamento_mode = PortamentoMode::Normal;
    p.lfo_rate_hz = 4.3;
    p.fifth_level = 0.12;
    p.volume = 0.62;

    // **Armed, which it never was.** A voice routes nothing until its topology is set, so this tour
    // has been rendering without any modulation at all. The three depths this score used to set as
    // patch fields are routes now, and all three pairs are ones Init already wires.
    let mut wiring = Routing::init();
    wiring.amounts[routing::target::VCO_2_PULSE_WIDTH][routing::source::LFO_TRIANGLE] = 0.65;
    wiring.amounts[routing::target::CUTOFF][routing::source::ENVELOPE_1] = 0.32;
    wiring.amounts[routing::target::VCO_2_PITCH][routing::source::LFO] = 0.018;
    voice.set_topology(&wiring);
    let mut active = [false; 3];
    for frame in 0..(FS as usize * 7) {
        match frame {
            0 => {
                voice.note_on(note(1, 45), 0.0);
                active[0] = true;
            }
            x if x == FS as usize => {
                voice.note_on(note(2, 57), 0.0);
                active[1] = true;
            }
            x if x == 2 * FS as usize => {
                voice.note_on(note(3, 64), 0.0);
                active[2] = true;
            }
            x if x == 3 * FS as usize => {
                voice.note_off(Some(3), 0, 64);
                active[2] = false;
            }
            x if x == 4 * FS as usize => {
                voice.note_off(Some(1), 0, 45);
                active[0] = false;
            }
            x if x == 5 * FS as usize => {
                voice.note_off(Some(2), 0, 57);
                active[1] = false;
            }
            _ => {}
        }
        // Mono hardware output copied into stereo; no effect is invented.
        let y = voice.process(&p);
        // Clamped here, where the hand-written writer used to clamp: the encoder applies no policy.
        out.extend([y.clamp(-1.0, 1.0); 2]);
    }
    debug_assert!(!active.into_iter().any(|x| x));
    mxm_audio_file::write(
        "mxm-para-07-demo.wav",
        &out,
        2,
        FS,
        mxm_audio_file::Target::WavFloat32,
    )
    .map_err(std::io::Error::other)?;
    println!(
        "wrote mxm-para-07-demo.wav (7 s: low note, split dyad, extreme-owner handoffs, release)"
    );
    Ok(())
}
