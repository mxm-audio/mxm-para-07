//! Golden rendered score for mxm-para-07 through the real application path.
//!
//! Update the digest only with an intentional audio change after listening to the emitted WAV.

use mxm_player_harness::app_harness;

use mxm_player::session::{FRAMES_PER_BLOCK, Session};
use std::path::PathBuf;

const PLUGIN: &str = "dk.mxm.mxm-para-07";
/// First pinned with the non-editor plugin shell. The score covers two-pitch assignment, a middle
/// press that owns only gate life, high-key collapse and the shared release. Measured by the
/// behaviour suite; not yet compared to hardware, so fidelity remains UNVERIFIED.
const GOLDEN_DIGEST: &str = "23e7b8f79328a975";

/// Whether this platform's render can match the pinned digests. They are Windows': each platform's
/// maths library rounds in its own way, so the same score renders different bits on Linux and macOS.
/// The owner pinned them on Windows only, where the sound was recorded and approved (2026-10-06);
/// elsewhere every other check in these tests still runs.
const DIGESTS_PINNED_HERE: bool = cfg!(target_os = "windows");
const GOLDEN_SAMPLES: usize = 72 * FRAMES_PER_BLOCK * 2;

fn bundle() -> Option<(PathBuf, PathBuf)> {
    let dir = app_harness::workspace_root().join("target").join("bundled");
    let file = dir.join("mxm-para-07.clap");
    file.exists().then_some((dir, file))
}
fn score(session: &mut Session) -> Result<(), String> {
    session.advance_blocks(2)?;
    session.app().note_on(48, 1.0);
    session.advance_blocks(10)?;
    session.app().note_on(67, 1.0);
    session.advance_blocks(14)?;
    session.app().note_on(57, 1.0);
    session.advance_blocks(8)?;
    session.app().note_off(67);
    session.advance_blocks(10)?;
    session.app().note_off(57);
    session.advance_blocks(6)?;
    session.app().note_off(48);
    session.advance_blocks(22)
}
fn digest(samples: &[f32]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for sample in samples {
        for byte in sample.to_bits().to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    format!("{hash:016x}")
}
fn render(name: &str) -> Option<(Vec<f32>, PathBuf)> {
    let (dir, file) = bundle()?;
    let mut s = Session::scratch(name, vec![dir]);
    s.load(&file, PLUGIN);
    score(&mut s).unwrap();
    let samples = s.captured();
    let (wav, _) = s.write_artifacts(name).unwrap();
    Some((samples, wav))
}

#[test]
fn the_two_pitch_score_still_sounds_the_same() {
    let Some((samples, wav)) = render("golden-para-07") else {
        eprintln!("skipping: run `cargo xtask bundle mxm-para-07 --release`");
        return;
    };
    assert_eq!(samples.len(), GOLDEN_SAMPLES);
    assert!(samples.iter().any(|x| x.abs() > 1e-4));
    let actual = digest(&samples);
    if DIGESTS_PINNED_HERE {
        assert_eq!(
            actual,
            GOLDEN_DIGEST,
            "render changed; listen to {} and, only if intentional, pin {actual}",
            wav.display()
        );
    }
}

#[test]
fn the_golden_oracle_is_sensitive_to_the_sound() {
    let Some((dir, file)) = bundle() else {
        eprintln!("skipping: run `cargo xtask bundle mxm-para-07 --release`");
        return;
    };
    let mut s = Session::scratch("golden-para-07-sensitivity", vec![dir]);
    s.load(&file, PLUGIN);
    let p = s.state().param("Cutoff").unwrap().clone();
    s.app()
        .engine_mut()
        .push_gui_event(mxm_player::events::input::Payload::ParamValue {
            param_id: p.id,
            value: p.min + 0.05 * (p.max - p.min),
        });
    score(&mut s).unwrap();
    if DIGESTS_PINNED_HERE {
        assert_ne!(digest(&s.captured()), GOLDEN_DIGEST);
    }
}
