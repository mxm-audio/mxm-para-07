//! mxm-para-07 rendered through the actual MXM Player host.
//!
//! The player contains no product registration: this suite gives discovery an ordinary search
//! directory and loads the bundle path/CLAP id returned by the same generic host path.

use mxm_player_harness::app_harness;

use mxm_player::control_map::schema::{InstrumentMap, Layout};
use mxm_player::events::input::Payload;
use mxm_player::offline::{self, EventKind, RenderConfig, ScheduledEvent};
use mxm_player::session::{FRAMES_PER_BLOCK, Session};
use serde_json::{Value, json};
use std::path::PathBuf;

const PLUGIN: &str = "dk.mxm.mxm-para-07";
const SAMPLE_RATE: f64 = 48_000.0;
const SKIP: &str = "skipping: run `cargo xtask bundle mxm-para-07 --release`";

fn bundle() -> Option<(PathBuf, PathBuf)> {
    let dir = app_harness::workspace_root().join("target").join("bundled");
    let file = dir.join("mxm-para-07.clap");
    file.exists().then_some((dir, file))
}
fn session(name: &str) -> Option<Session> {
    let (dir, file) = bundle()?;
    let mut session = Session::scratch(name, vec![dir]);
    session.load(&file, PLUGIN);
    Some(session)
}
fn left(samples: &[f32]) -> Vec<f32> {
    samples.iter().step_by(2).copied().collect()
}
fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0f32, |v, x| v.max(x.abs()))
}
fn magnitude_at(samples: &[f32], hz: f64) -> f64 {
    let (mut re, mut im) = (0.0, 0.0);
    for (i, sample) in samples.iter().enumerate() {
        let a = std::f64::consts::TAU * hz * i as f64 / SAMPLE_RATE;
        re += f64::from(*sample) * a.cos();
        im += f64::from(*sample) * a.sin();
    }
    (re * re + im * im).sqrt() / samples.len().max(1) as f64
}
fn hz(note: u8) -> f64 {
    440.0 * 2f64.powf((f64::from(note) - 69.0) / 12.0)
}
fn brightness(samples: &[f32]) -> f32 {
    samples.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f32>() / (samples.len() - 1).max(1) as f32
}
fn set(session: &mut Session, name: &str, normalised: f64) -> String {
    let p = session
        .state()
        .param(name)
        .unwrap_or_else(|| panic!("missing parameter {name}"))
        .clone();
    set_id(session, p.id, p.min, p.max, normalised);
    session.advance_blocks(4).unwrap();
    session.state().param(name).unwrap().text.clone()
}
fn set_id(session: &mut Session, id: u32, min: f64, max: f64, normalised: f64) {
    session
        .app()
        .engine_mut()
        .push_gui_event(Payload::ParamValue {
            param_id: id,
            value: min + normalised * (max - min),
        });
}
fn nice_param_id(stable_id: &str) -> u32 {
    stable_id.bytes().fold(0u32, |hash, byte| {
        hash.wrapping_mul(31).wrapping_add(u32::from(byte))
    }) & !(1 << 31)
}
/// nice-plug's CLAP state: a little-endian length, then uncompressed JSON.
fn decode_state(bytes: &[u8]) -> Value {
    let size = u64::from_le_bytes(bytes[..8].try_into().expect("CLAP state length prefix"));
    assert_eq!(size as usize, bytes.len() - 8);
    serde_json::from_slice(&bytes[8..]).expect("uncompressed nice-plug state")
}
fn encode_state(value: &Value) -> Vec<u8> {
    let json = serde_json::to_vec(value).expect("state JSON");
    let mut bytes = Vec::with_capacity(json.len() + 8);
    bytes.extend_from_slice(&(json.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&json);
    bytes
}
fn capture(session: &mut Session, blocks: u64) -> Vec<f32> {
    session.clear_capture();
    session.advance_blocks(blocks).unwrap();
    let audio = session.captured();
    let skip = (FRAMES_PER_BLOCK * 2 * 3).min(audio.len());
    left(&audio[skip..])
}

#[test]
fn discovery_loads_it_as_an_instrument_and_idle_is_exact_silence() {
    let Some(mut s) = session("para07-discovery") else {
        eprintln!("{SKIP}");
        return;
    };
    let state = s.state();
    let loaded = state.plugin.as_ref().unwrap();
    assert_eq!(loaded.id, PLUGIN);
    assert_eq!(loaded.channels, 2);
    assert_eq!(loaded.selection, "configuration 0 \"Stereo\" (stereo out)");
    assert!(loaded.note_input.is_some());
    assert!(loaded.note_output.is_none());
    s.advance_blocks(24).unwrap();
    assert_eq!(peak(&s.captured()), 0.0);
}

#[test]
fn one_note_is_dual_mono_at_pitch_and_releases_to_exact_silence() {
    let Some(mut s) = session("para07-note") else {
        eprintln!("{SKIP}");
        return;
    };
    s.app().note_on(57, 1.0);
    s.clear_capture();
    s.advance_blocks(45).unwrap();
    let audio = s.captured();
    for pair in audio.as_chunks::<2>().0 {
        assert_eq!(
            pair[0].to_bits(),
            pair[1].to_bits(),
            "stereo must be bit-identical dual mono"
        );
    }
    let mono = left(&audio[(FRAMES_PER_BLOCK * 2 * 3).min(audio.len())..]);
    let at = magnitude_at(&mono, hz(57));
    let below = magnitude_at(&mono, hz(56));
    let above = magnitude_at(&mono, hz(58));
    assert!(
        peak(&mono) > 0.03 && at > 1.7 * below && at > 1.7 * above,
        "note pitch: {at} vs {below}/{above}"
    );
    s.app().note_off(57);
    s.advance_blocks(180).unwrap();
    s.clear_capture();
    s.advance_blocks(32).unwrap();
    assert_eq!(peak(&s.captured()), 0.0);
}

#[test]
fn highest_and_lowest_keys_drive_the_two_oscillators_through_one_gate() {
    let Some(mut s) = session("para07-extremes") else {
        eprintln!("{SKIP}");
        return;
    };
    set(&mut s, "VCO-2", 0.7);
    s.app().note_on(48, 1.0);
    s.app().note_on(67, 1.0);
    let both = capture(&mut s, 70);
    let low = magnitude_at(&both, hz(48) * 2f64.powf(0.07 / 12.0));
    let high = magnitude_at(&both, hz(67));
    assert!(
        low > 0.003 && high > 0.003,
        "both extremes must sound: low={low} high={high}"
    );
    s.app().note_on(57, 1.0);
    let middle = capture(&mut s, 30);
    assert!(
        magnitude_at(&middle, hz(48) * 2f64.powf(0.07 / 12.0)) > 0.002
            && magnitude_at(&middle, hz(67)) > 0.002,
        "a middle press keeps the shared gate but owns neither pitch"
    );
    s.app().note_off(67);
    let collapsed = capture(&mut s, 45);
    assert!(
        magnitude_at(&collapsed, hz(57)) > magnitude_at(&collapsed, hz(67)),
        "releasing the high extreme retargets it to the middle survivor"
    );
    s.app().note_off(57);
    s.app().note_off(48);
}

#[test]
fn host_parameter_events_change_the_audio_and_do_not_need_block_aligned_notes() {
    let Some(mut s) = session("para07-events") else {
        eprintln!("{SKIP}");
        return;
    };
    s.app().note_on(45, 1.0);
    let open = capture(&mut s, 45);
    let text = set(&mut s, "Cutoff", 0.22);
    let closed = capture(&mut s, 45);
    assert!(
        brightness(&closed) < 0.55 * brightness(&open),
        "host cutoff {text} did not darken the held note"
    );
    set(&mut s, "Cutoff", 0.9);
    // The player's monotonic event queue lands these at callback offsets rather than calling DSP
    // methods directly. A release followed immediately by a new press exercises same-chunk split
    // ordering and must leave the new note sounding, not cut by the old release.
    s.app().note_off(45);
    s.app().note_on(64, 1.0);
    let replacement = capture(&mut s, 40);
    assert!(
        peak(&replacement) > 0.01
            && magnitude_at(&replacement, hz(64)) > magnitude_at(&replacement, hz(45)),
        "event split/order lost the replacement note"
    );
    s.app().note_off(64);
}

#[test]
fn trigger_parameter_edges_are_sample_accurate_and_restore_only_a_level() {
    let Some((dir, file)) = bundle() else {
        eprintln!("{SKIP}");
        return;
    };
    let config = RenderConfig {
        sample_rate: SAMPLE_RATE,
        block_size: 64,
        total_frames: 384,
        state: None,
    };
    let trigger = nice_param_id("triggerinput");
    let event = |frame, value| ScheduledEvent {
        frame,
        kind: EventKind::ParamValue {
            param_id: trigger,
            value,
        },
    };
    let immediate = offline::render(
        &file,
        PLUGIN,
        config.clone(),
        &[event(0, 1.0), event(2, 0.0)],
    )
    .unwrap();
    let delayed = offline::render(
        &file,
        PLUGIN,
        config.clone(),
        &[event(17, 1.0), event(19, 0.0)],
    )
    .unwrap();
    let explicit_low_high_low = offline::render(
        &file,
        PLUGIN,
        config.clone(),
        &[event(5, 0.0), event(17, 1.0), event(19, 0.0)],
    )
    .unwrap();
    let rising_only = offline::render(&file, PLUGIN, config.clone(), &[event(17, 1.0)]).unwrap();
    assert!(
        immediate.channels[0].iter().any(|sample| *sample != 0.0),
        "the low-high-low automation pulse was lost"
    );
    assert!(
        delayed.channels[0][..17]
            .iter()
            .all(|sample| *sample == 0.0),
        "the trigger moved ahead of its host event offset"
    );
    assert_eq!(
        &delayed.channels[0][17..],
        &immediate.channels[0][..(config.total_frames as usize - 17)],
        "moving both edges by 17 frames must move the one trigger by exactly 17 frames"
    );
    assert_eq!(
        explicit_low_high_low.channels, delayed.channels,
        "distinct low-high-low events at nonzero offsets must emit only the rising edge"
    );
    assert_eq!(
        delayed.channels, rising_only.channels,
        "the falling event must change only the retained input level, not emit a second trigger"
    );

    let mut source = Session::scratch("para07-trigger-state", vec![dir]);
    source.load(&file, PLUGIN);
    let parameter = source.state().param("Trigger input").unwrap().clone();
    set_id(&mut source, parameter.id, parameter.min, parameter.max, 1.0);
    source.advance_blocks(1).unwrap();
    let state = source.app().engine_mut().capture_state().unwrap();
    let restored = offline::render(
        &file,
        PLUGIN,
        RenderConfig {
            state: Some(state.clone()),
            ..config.clone()
        },
        &[],
    )
    .unwrap();
    assert!(
        restored
            .channels
            .iter()
            .flatten()
            .all(|sample| *sample == 0.0),
        "restoring a high trigger level manufactured an edge"
    );
    let falling_after_restore = offline::render(
        &file,
        PLUGIN,
        RenderConfig {
            state: Some(state),
            ..config
        },
        &[event(17, 0.0)],
    )
    .unwrap();
    assert!(
        falling_after_restore
            .channels
            .iter()
            .flatten()
            .all(|sample| *sample == 0.0),
        "a restored high level followed by a falling event emitted a trigger"
    );
}

#[test]
fn hold_and_the_normalled_clock_keep_the_host_awake_without_a_key() {
    let Some(mut s) = session("para07-autonomous") else {
        eprintln!("{SKIP}");
        return;
    };
    set(&mut s, "Hold", 0.65);
    let held = capture(&mut s, 100);
    assert!(peak(&held) > 0.01, "Hold must sound with no press");
    set(&mut s, "Hold", 0.0);
    s.advance_blocks(180).unwrap();
    s.clear_capture();
    s.advance_blocks(24).unwrap();
    assert_eq!(peak(&s.captured()), 0.0);
    // The S&H clock gates the envelopes in either key mode (the owner, 2026-09-27): no External
    // mode to select first.
    assert_eq!(set(&mut s, "Gate source", 1.0), "S&H clock");
    set(&mut s, "Sample time", 0.05);
    let clocked = capture(&mut s, 160);
    assert!(
        peak(&clocked) > 0.005,
        "the autonomous clock must keep the plugin processing"
    );
}

#[test]
fn both_audio_configurations_route_as_advertised_and_emit_no_events() {
    let Some((_dir, file)) = bundle() else {
        eprintln!("{SKIP}");
        return;
    };
    let render_config = RenderConfig {
        sample_rate: SAMPLE_RATE,
        block_size: 64,
        total_frames: 8192,
        state: None,
    };
    let note = ScheduledEvent {
        frame: 0,
        kind: EventKind::NoteOn {
            channel: 0,
            key: 57,
            velocity: 1.0,
            note_id: 1,
        },
    };

    let stereo =
        offline::render_configuration(&file, PLUGIN, render_config.clone(), &[note], 0, None)
            .unwrap();
    let mono =
        offline::render_configuration(&file, PLUGIN, render_config.clone(), &[note], 1, None)
            .unwrap();
    assert_eq!(stereo.envelope.selected_config.unwrap().get(), 0);
    assert_eq!(mono.envelope.selected_config.unwrap().get(), 1);
    assert_eq!((stereo.channels.len(), mono.channels.len()), (2, 1));
    assert_eq!(
        stereo.channels[0], stereo.channels[1],
        "stereo is not dual mono"
    );
    assert_eq!(
        stereo.channels[0], mono.channels[0],
        "mono and stereo renders differ"
    );
    assert_eq!(stereo.output_event_count, 0, "stereo emitted host events");
    assert_eq!(mono.output_event_count, 0, "mono emitted host events");
    assert!(stereo.envelope.note_output.is_none());
    assert!(mono.envelope.note_output.is_none());
    // No third configuration: the external input's two went with it (the owner, 2026-09-26).
    assert!(
        offline::render_configuration(&file, PLUGIN, render_config, &[note], 2, None).is_err(),
        "a configuration beyond stereo and mono is still advertised"
    );
}

/// **A session saved while the external input was here restores everything else** (the owner,
/// 2026-09-26). Its state names the seventeen retired ids — the input's two source switches and
/// sensitivity, and the follower's route pair on all seven targets — and the wrapper skips them:
/// the same patch without them renders to the bit.
#[test]
fn a_session_naming_the_retired_external_ids_restores_everything_else() {
    let Some((dir, file)) = bundle() else {
        eprintln!("{SKIP}");
        return;
    };
    let mut source = Session::scratch("para07-retired-state", vec![dir]);
    source.load(&file, PLUGIN);
    set(&mut source, "Hold", 0.7);
    set(&mut source, "VCO-2", 0.5);
    set(&mut source, "Ring", 0.6);
    set(&mut source, "Cutoff", 0.4);
    let current = source.app().engine_mut().capture_state().unwrap();

    let mut old = decode_state(&current);
    let params = old["params"].as_object_mut().expect("parameter map");
    params.insert("ringinput".into(), json!({"string": "external"}));
    params.insert("fifthinput".into(), json!({"string": "external"}));
    params.insert("extsensitivity".into(), json!({"string": "high"}));
    for target in [
        "vco1pitch",
        "vco2pitch",
        "vco1pw",
        "vco2pw",
        "cutoff",
        "amp",
        "mult",
    ] {
        params.insert(format!("mod_{target}_follow"), json!({"f32": 0.9}));
        params.insert(format!("mod_{target}_followon"), json!({"bool": true}));
    }

    let config = RenderConfig {
        sample_rate: SAMPLE_RATE,
        block_size: 64,
        total_frames: 8192,
        state: None,
    };
    let from_current = offline::render(
        &file,
        PLUGIN,
        RenderConfig {
            state: Some(current),
            ..config.clone()
        },
        &[],
    )
    .unwrap();
    let from_old = offline::render(
        &file,
        PLUGIN,
        RenderConfig {
            state: Some(encode_state(&old)),
            ..config
        },
        &[],
    )
    .unwrap();
    assert!(
        peak(&from_current.channels[0]) > 0.01,
        "the restored patch must sound, or equality proves nothing"
    );
    assert_eq!(
        from_old.channels, from_current.channels,
        "the retired ids changed what the session restored"
    );
}

#[test]
fn the_instrument_map_uses_current_and_frozen_old_player_roles_and_loaded_parameter_ids() {
    let Some(mut s) = session("para07-control-map") else {
        eprintln!("{SKIP}");
        return;
    };
    let root = app_harness::workspace_root();
    let map_text =
        std::fs::read_to_string(root.join("plugins/mxm-para-07/control-map.json")).unwrap();
    let map: serde_json::Value = serde_json::from_str(&map_text).unwrap();
    let instrument_map = InstrumentMap::parse(&map_text).unwrap();
    let current = Layout::parse(mxm_player::control_map::schema::SHIPPED).unwrap();
    let old_player = Layout::parse(include_str!(
        "fixtures/control-map-old-player-ten-page.json"
    ))
    .unwrap();
    assert_eq!(old_player.pages.len(), 10, "the old-player fixture moved");
    for instrument in &instrument_map.instruments {
        current.check_instrument(instrument).unwrap();
        old_player
            .check_instrument(instrument)
            .expect("the frozen ten-page old player must accept the complete instrument map");
    }
    let claims = map["instruments"][0]["params"].as_object().unwrap();
    let loaded_state = s.state();
    let host_params = &loaded_state.plugin.as_ref().unwrap().params;
    for (role, stable_id) in claims {
        assert!(
            current.roles.contains_key(role),
            "unknown collection role {role}"
        );
        assert!(
            old_player.roles.contains_key(role),
            "old player lacks role {role}"
        );
        let stable_id = stable_id.as_str().unwrap();
        assert!(
            host_params
                .iter()
                .any(|param| param.id == nice_param_id(stable_id)),
            "map names missing plugin parameter {stable_id}"
        );
    }
    for omitted_id in [
        "filtermod",
        "filteraudio",
        "gatesource",
        "triggerinput",
        "keysource",
        "hpf",
        "shtime",
        "shlag",
        "follower",
    ] {
        assert!(
            !claims.values().any(|value| value == omitted_id),
            "intentionally unmapped control was claimed: {omitted_id}"
        );
    }
    // Keep the instance alive through one callback: this is the loaded bundle's parameter list,
    // not merely two JSON files agreeing with each other.
    s.advance_blocks(1).unwrap();
}

#[test]
fn every_factory_preset_sounds_through_the_host_and_state_is_deterministic() {
    let Some((dir, file)) = bundle() else {
        eprintln!("{SKIP}");
        return;
    };
    let preset_dir = app_harness::workspace_root()
        .join("plugins")
        .join("mxm-para-07")
        .join("presets");
    let mut files: Vec<_> = std::fs::read_dir(preset_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    assert_eq!(files.len(), 50);
    for preset_path in files {
        let mut s = Session::scratch("para07-factory", vec![dir.clone()]);
        s.load(&file, PLUGIN);
        let json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&preset_path).unwrap()).unwrap();
        assert_eq!(json["plugin"], PLUGIN);
        let params = json["params"].as_object().unwrap();
        assert_eq!(
            params.len(),
            s.state().plugin.as_ref().unwrap().params.len()
        );
        for (id, value) in params {
            let normalised = value["v"].as_f64().unwrap();
            let p = s
                .state()
                .plugin
                .as_ref()
                .unwrap()
                .params
                .iter()
                .find(|param| param.id == nice_param_id(id))
                .unwrap_or_else(|| panic!("{} names missing {id}", preset_path.display()))
                .clone();
            set_id(&mut s, p.id, p.min, p.max, normalised);
        }
        s.advance_blocks(5).unwrap();
        let state_a = s.app().engine_mut().capture_state().unwrap();
        let state_b = s.app().engine_mut().capture_state().unwrap();
        assert_eq!(
            state_a,
            state_b,
            "nondeterministic state for {}",
            preset_path.display()
        );
        s.clear_capture();
        s.app().note_on(60, 1.0);
        s.advance_blocks(100).unwrap();
        assert!(
            peak(&s.captured()) > 1.0e-5,
            "factory preset rendered silence: {}",
            preset_path.display()
        );
    }
}
