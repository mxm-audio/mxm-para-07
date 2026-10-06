//! Card bodies for the editor's fifteen cards.
//!
//! The hardware functions survive, but their panel positions do not: voice assignment leads, the
//! modulation sources and their destination depths follow, then the pitch sources and the shared
//! audio path. The paging renderer decides which cards share a page. `editor::SECTIONS` is the one
//! ownership list; the master Volume is the app bar's, drawn by `editor::panel`.
//!
//! Each card is a `mxm_ui::tree` (plans/plan-layout-tree.md): [`card`] describes its body once, and
//! that one description is both measured — its floor and its height — and drawn, leaf by leaf,
//! through the bindings below ([`paint`]). Nothing is typed and nothing is drawn to learn a size.

use std::collections::HashMap;

use egui::Ui;
use mxm_ui::control::{Size, Wave};
use mxm_ui::space::{SPACE_2, SPACE_3};
use mxm_ui::theme::Tokens;
use mxm_ui::tree::{self, Height, Kind, Node, Share, leaf, pad, pad_all, share, stack, stack_gap};
use nice_plug::prelude::ParamSetter;

use super::Section;
use super::binding::{Bound, segmented_named, segmented_waves_named, toggle_labelled};
use super::visuals;
use crate::params::MxmPara07Params;
use crate::telemetry::Telemetry;
use mxm_para_07_dsp::routing::{TARGET_NAMES, target};

const COMPACT: Size = Size::Compact;
const PRIMARY: Size = Size::Primary;
const STANDARD: Size = Size::Standard;
/// The lines a knob's name box holds (`mxm_ui::control::knob`).
const NAME_LINES: usize = 2;

/// The ladder of the control `id`, if it has a tempo sync (`plans/plan-tempo-sync-controls.md`).
fn ladder_of(id: &str) -> Option<mxm_tempo::Ladder> {
    match id {
        "lforate" => Some(crate::params::LFO_SYNC),
        "shtime" => Some(crate::params::SH_SYNC),
        _ => None,
    }
}

/// A knob and its tempo sync's quarter note beside it, on the knob's grid, then `after` — one flat
/// row, so a card wider than the row leaves its spare width at the end rather than between them.
fn synced_row(
    ui: &Ui,
    params: &MxmPara07Params,
    id: &'static str,
    sync: &'static str,
    after: &[&'static str],
) -> Node<Leaf> {
    tree::row_gap(
        ui.spacing().item_spacing.x,
        vec![
            knobs(ui, params, &[id]),
            tree::switch_beside_knob(knob_size(id), leaf(Leaf::Picture(sync), Kind::SyncToggle)),
            knobs(ui, params, after),
        ],
    )
}

/// Cutoff and Resonance are Primary; **a tempo-syncable knob is Standard**, so it always shows its
/// reading — hertz or seconds free, the note when synced (the owner, 2026-09-27: every LFO shows its
/// time or rhythm while it is adjusted, as mxm-mono-00's do); the rest are Compact.
fn knob_size(id: &str) -> Size {
    if matches!(id, "cutoff" | "resonance") {
        PRIMARY
    } else if ladder_of(id).is_some() {
        STANDARD
    } else {
        COMPACT
    }
}

fn selector(
    ui: &mut Ui,
    tokens: &Tokens,
    params: &MxmPara07Params,
    setter: &ParamSetter<'_>,
    id: &'static str,
) {
    let bound = binding_for(id, params);
    if let Some(waves) = wave_options(id) {
        segmented_waves_named(
            ui,
            tokens,
            id,
            bound.param,
            bound.panel.as_deref(),
            waves,
            None,
            bound.details,
            setter,
        );
    } else {
        let labels = options(params, id);
        let options: Vec<&str> = labels.iter().map(String::as_str).collect();
        segmented_named(
            ui,
            tokens,
            id,
            bound.param,
            bound.panel.as_deref(),
            &options,
            bound.details,
            setter,
            0.0,
        );
    }
}

/// Each target's **painted** name: the prefix its card already carries dropped (design system
/// §7.1) — *Pitch* and *Pulse width* under a card titled *Oscillator 1*. `TARGET_NAMES` stays the
/// canonical name, which the parameters, the host and the accessibility tree read.
const TARGET_PANEL_NAMES: [&str; mxm_para_07_dsp::routing::TARGETS] = [
    "Pitch",
    "Pitch",
    "Pulse width",
    "Pulse width",
    "Cutoff",
    "VCA level",
    "Multiplier",
    "Amplitude",
    "Level",
];

/// One target's route stack, drawn by the shared routing widget.
fn routes(
    ui: &mut Ui,
    tokens: &Tokens,
    target: usize,
    params: &MxmPara07Params,
    setter: &ParamSetter<'_>,
    text_entry: &mut HashMap<&'static str, Option<String>>,
) {
    let group = params.routes.each()[target];
    let entry = text_entry.entry("routes").or_default();
    mxm_modulation_params::ui::stack(
        ui,
        tokens,
        TARGET_NAMES[target],
        TARGET_PANEL_NAMES[target],
        &group.routes(target),
        entry,
        setter,
    );
}

fn caption(ui: &mut Ui, tokens: &Tokens, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .color(tokens.text_secondary)
            .text_style(mxm_ui::typography::caption_style(ui.style())),
    );
}

// ---------------------------------------------------------------------------------------------
// The cards, as trees (plans/plan-layout-tree.md).
// ---------------------------------------------------------------------------------------------

/// What a leaf of this editor's cards draws. Hashed by what it names, which is also what keeps its
/// widget ids stable when a route appears above it.
#[derive(Clone, Debug, Hash)]
pub(crate) enum Leaf {
    Knob(&'static str),
    /// A fader in the collection's fader row.
    Fader(&'static str),
    /// A stepped parameter: word cells, or wave pictures where it has them.
    Switch(&'static str),
    Toggle(&'static str),
    /// A control's tempo sync, the quarter note beside it.
    Picture(&'static str),
    Caption(String),
    Routes(usize),
    FilterResponse,
    SampleHold,
    /// The Mixer's overload acknowledgement, present only while the overload is latched.
    ClearOverload,
}

/// What the cards show that comes from telemetry rather than from parameters, read once per frame
/// so the tree measured is the tree painted: the oscillators' sounding frequencies, which their
/// captions print, and the latched mixer overload, which adds a button.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Readings {
    /// Each oscillator's sounding frequency while a note sounds; `None` at rest, where nothing has
    /// played and a number would only be the last one or zero.
    pub vco_hz: [Option<f32>; 2],
    pub overloaded: bool,
}

impl Readings {
    pub(crate) fn of(telemetry: &Telemetry) -> Self {
        let voice = telemetry.voice_snapshot();
        let sounding = telemetry.sounding();
        Self {
            vco_hz: [sounding.then_some(voice[1]), sounding.then_some(voice[2])],
            overloaded: telemetry.overloaded(),
        }
    }
}

const OVERLOAD: &str = "Mixer overload — clear";

/// Knob rows of at most four, each the collection's knob row (`mxm_ui::tree::knob_row`), and rows an
/// item spacing and `SPACE_3` apart.
/// Levels as the collection's fader row (`tree::fader_row`): the mixer's four sources, an
/// envelope's A, D, S and R.
fn faders(ui: &Ui, params: &MxmPara07Params, ids: &[&'static str]) -> Node<Leaf> {
    mxm_ui::tree::fader_row(
        ui,
        ids.iter()
            .map(|&id| {
                let bound = binding_for(id, params);
                mxm_ui::tree::fader(
                    Leaf::Fader(id),
                    bound.painted(),
                    mxm_ui::control::widest_value(|n| bound.param.format(n as f32)),
                )
            })
            .collect(),
    )
}

fn knobs(ui: &Ui, params: &MxmPara07Params, ids: &[&'static str]) -> Node<Leaf> {
    let mut rows: Vec<Node<Leaf>> = ids
        .chunks(4)
        .map(|chunk| {
            mxm_ui::tree::knob_row(
                ui,
                chunk
                    .iter()
                    .map(|id| (knob_size(id), knob(ui, params, id)))
                    .collect(),
            )
        })
        .collect();
    if rows.len() == 1 {
        rows.remove(0)
    } else {
        stack_gap(2.0 * SPACE_3, rows)
    }
}

/// A knob, in a column that holds what it draws: its name in the name box's two lines, and — where
/// the tier shows it — its widest reading on the one line the knob draws it on, eliding what does not
/// fit. `control::knob_size` counts a word of each, which is narrower than either: *Envelope 1
/// release* would wrap onto a third line above its box, and *20000.0 Hz* would print *20000.…*.
fn knob(ui: &Ui, params: &MxmPara07Params, id: &'static str) -> Node<Leaf> {
    let bound = binding_for(id, params);
    let size = knob_size(id);
    let name = bound.painted().to_owned();
    // A syncable control's column holds its free readings and its divisions.
    let widest = match (size.value_always_visible(), ladder_of(id)) {
        (false, _) => String::new(),
        (true, Some(ladder)) => super::binding::synced_widest(bound.param, ladder.span),
        (true, None) => mxm_ui::control::widest_value(|n| bound.param.format(n as f32)),
    };
    let value = mxm_ui::typography::value_style(ui.style()).resolve(ui.style());
    let reading = ui
        .painter()
        .layout_no_wrap(widest.clone(), value, egui::Color32::PLACEHOLDER)
        .size()
        .x;
    leaf(
        Leaf::Knob(id),
        Kind::Knob {
            column: name_in_lines(ui, &name, NAME_LINES).max(reading),
            name,
            widest,
            size,
        },
    )
}

/// The narrowest width at which `name` wraps into at most `lines` lines in the Body style, as the
/// knob's name box wraps it. The best split of its words into lines is where the search starts —
/// each line measured with the space that follows it, because egui breaks a line when that space no
/// longer fits — and egui's own layout at that width is what decides, a quarter point at a time.
fn name_in_lines(ui: &Ui, name: &str, lines: usize) -> f32 {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let width = |text: String| {
        ui.painter()
            .layout_no_wrap(text, font.clone(), egui::Color32::PLACEHOLDER)
            .size()
            .x
    };
    fn best(words: &[&str], lines: usize, width: &dyn Fn(String) -> f32) -> f32 {
        if lines <= 1 || words.len() <= 1 {
            return width(words.join(" "));
        }
        (1..words.len())
            .map(|cut| {
                width(format!("{} ", words[..cut].join(" "))).max(best(
                    &words[cut..],
                    lines - 1,
                    width,
                ))
            })
            .fold(width(words.join(" ")), f32::min)
    }
    let words: Vec<&str> = name.split_whitespace().collect();
    let one_line = width(name.to_owned());
    let mut column = best(&words, lines, &width);
    while column < one_line
        && ui
            .painter()
            .layout(
                name.to_owned(),
                font.clone(),
                egui::Color32::PLACEHOLDER,
                column,
            )
            .rows
            .len()
            > lines
    {
        column += 0.25;
    }
    column.min(one_line)
}

fn switch(params: &MxmPara07Params, id: &'static str) -> Node<Leaf> {
    let name = binding_for(id, params).painted().to_owned();
    let kind = match wave_options(id) {
        Some(waves) => Kind::Waves {
            label: Some(name),
            count: waves.len(),
            marks: Vec::new(),
            beside: None,
        },
        None => Kind::Segmented {
            label: name,
            options: options(params, id),
            beside: None,
        },
    };
    leaf(Leaf::Switch(id), kind)
}

fn toggle_leaf(params: &MxmPara07Params, id: &'static str) -> Node<Leaf> {
    leaf(
        Leaf::Toggle(id),
        Kind::Toggle {
            label: binding_for(id, params).painted().to_owned(),
        },
    )
}

fn caption_leaf(text: String) -> Node<Leaf> {
    tree::caption(Leaf::Caption(text.clone()), &text)
}

/// A target's routes, `SPACE_3` below what precedes it. A route stack is a composite with a rule of
/// its own — its floor is every route revealed at its widest reading — so it states its size
/// (`stack_size`) and takes the card's width.
fn routes_leaf(ui: &Ui, params: &MxmPara07Params, target: usize) -> Node<Leaf> {
    let size = mxm_modulation_params::ui::stack_size(
        ui,
        TARGET_PANEL_NAMES[target],
        &params.routes.each()[target].routes(target),
    );
    pad(
        SPACE_3,
        leaf(
            Leaf::Routes(target),
            Kind::Custom {
                min_width: size.x,
                height: Height::Fixed(size.y),
                fills: true,
            },
        ),
    )
}

/// A display across the card's width at a fixed height, never narrower than `min_width`.
fn display(key: Leaf, min_width: f32, height: f32) -> Node<Leaf> {
    leaf(
        key,
        Kind::Custom {
            min_width,
            height: Height::Fixed(height),
            fills: true,
        },
    )
}

/// Selectors that stand together and share one cell width, so they read as one set.
fn shared(params: &MxmPara07Params, ids: &[&'static str], gap: f32) -> Node<Leaf> {
    share(
        Share::Cells,
        stack_gap(gap, ids.iter().map(|id| switch(params, id)).collect()),
    )
}

/// A section's body, as a tree, from the parameters and the telemetry it prints.
pub(crate) fn card(
    ui: &Ui,
    section: Section,
    params: &MxmPara07Params,
    readings: &Readings,
) -> Node<Leaf> {
    match section {
        // **The pitch and pulse-width routes are on the card they move** (the owner, 2026-09-24 and
        // 2026-09-27; design system §7.4), each under its oscillator's Tune and Pulse width knobs.
        // Oscillator 1's Tune is the instrument's master tune, which VCO-2 follows. The
        // sounding-frequency caption **reserves its widest reading**, so a frequency changing never
        // re-wraps it (§7.5).
        Section::Oscillator1 => stack(vec![
            switch(params, "vco1range"),
            pad(SPACE_3, switch(params, "vco1wave")),
            pad(SPACE_3, knobs(ui, params, &["tune", "vco1width"])),
            sounding(readings.vco_hz[0]),
            routes_leaf(ui, params, target::VCO_1_PITCH),
            routes_leaf(ui, params, target::VCO_1_PULSE_WIDTH),
        ]),
        Section::Oscillator2 => stack(vec![
            switch(params, "vco2range"),
            pad(SPACE_3, switch(params, "vco2wave")),
            pad(SPACE_3, knobs(ui, params, &["vco2tune", "vco2width"])),
            toggle_leaf(params, "sync"),
            sounding(readings.vco_hz[1]),
            routes_leaf(ui, params, target::VCO_2_PITCH),
            routes_leaf(ui, params, target::VCO_2_PULSE_WIDTH),
        ]),
        // The bank's Level is a target (the owner, 2026-09-27): its routes under the knob they move.
        Section::Registers => stack(vec![
            knobs(
                ui,
                params,
                &["reg32", "reg16", "reg8", "reg4", "reg2", "bank"],
            ),
            routes_leaf(ui, params, target::BANK_LEVEL),
        ]),
        Section::Mixer => {
            // The noise colour sits an item spacing and `SPACE_2` below the faders, and keeps that
            // much room under it — above the overload button when it is there, at the card's foot
            // when it is not.
            let foot = if readings.overloaded {
                SPACE_2
            } else {
                SPACE_3 + SPACE_2
            };
            let mut body = vec![
                faders(ui, params, &["vco1", "vco2", "noise", "fifth"]),
                pad_all(SPACE_3, 0.0, foot, switch(params, "noisecolour")),
            ];
            if readings.overloaded {
                body.push(leaf(
                    Leaf::ClearOverload,
                    Kind::Button {
                        label: OVERLOAD.to_owned(),
                        min: egui::Vec2::ZERO,
                        fills: false,
                    },
                ));
            }
            stack(body)
        }
        // The cutoff's routes beneath the knobs they move (the owner, 2026-09-24).
        Section::Filter => stack(vec![
            display(
                Leaf::FilterResponse,
                visuals::DISPLAY_MIN_WIDTH,
                visuals::FILTER_HEIGHT,
            ),
            pad(SPACE_3, knobs(ui, params, &["hpf", "cutoff", "resonance"])),
            routes_leaf(ui, params, target::CUTOFF),
        ]),
        Section::Lfo => stack(vec![
            switch(params, "lfoshape"),
            // Each synced control with its tempo sync's quarter note beside it.
            pad(
                SPACE_3,
                synced_row(ui, params, "lforate", "lfosync", &["lfodelay"]),
            ),
            toggle_leaf(params, "lfokeytrigger"),
        ]),
        Section::SampleHold => stack(vec![
            display(
                Leaf::SampleHold,
                visuals::DISPLAY_MIN_WIDTH,
                visuals::SH_HEIGHT,
            ),
            pad(SPACE_3, switch(params, "shsource")),
            synced_row(ui, params, "shtime", "shsync", &["shlag"]),
        ]),
        Section::Envelope1 | Section::Envelope2 => {
            let first = section == Section::Envelope1;
            let ids = if first {
                ["env1attack", "env1decay", "env1sustain", "env1release"]
            } else {
                ["env2attack", "env2decay", "env2sustain", "env2release"]
            };
            stack(vec![
                faders(ui, params, &ids),
                switch(params, if first { "env1trigger" } else { "env2trigger" }),
            ])
        }
        Section::AmplifierMod => stack(vec![
            knobs(ui, params, &["hold"]),
            routes_leaf(ui, params, target::VCA_LEVEL),
            // The collection's standard Amplitude, a factor after the VCA this card levels
            // (`plans/plan-modulation-standard.md`).
            routes_leaf(ui, params, target::AMPLITUDE),
        ]),
        Section::Keyboard => stack(vec![
            shared(params, &["keymode", "gatesource"], tree::GAP),
            toggle_leaf(params, "triggerinput"),
        ]),
        // Two timed pitch motions, each a time and a direction: portamento between notes, auto bend
        // into each (one card, R2's call, the owner's to overrule). Two modules, so their names keep
        // their prefixes.
        Section::Portamento => stack(vec![
            knobs(ui, params, &["portamento"]),
            switch(params, "portamentomode"),
            pad(SPACE_3, knobs(ui, params, &["autobendtime"])),
            switch(params, "autobenddirection"),
        ]),
        // **The lever reaches its destinations from their own stacks now**, as `Bend` for the
        // signed path and as `Multiplier` for the rectified one. What is left here is the module
        // itself: the two factors whose product that second path is.
        Section::Bender => routes_leaf(ui, params, target::MULTIPLIER),
    }
}

/// An oscillator's sounding-frequency reading while a note sounds, and nothing at rest — its room
/// reserved at the widest reading it can print, five digits of hertz, so a note starting or the
/// frequency moving never moves the card.
fn sounding(hz: Option<f32>) -> Node<Leaf> {
    let text = |hz: f32| format!("Sounding {hz:.1} Hz");
    tree::reserve(
        caption_leaf(hz.map(text).unwrap_or_default()),
        vec![caption_leaf(text(99_999.9))],
    )
}

/// Everything a leaf draws with: the parameters and their host, and the live telemetry.
pub(crate) struct Live<'a, 'b> {
    pub params: &'a MxmPara07Params,
    pub telemetry: &'a Telemetry,
    pub setter: &'a ParamSetter<'b>,
    pub entries: &'a mut HashMap<&'static str, Option<String>>,
}

/// Draws one leaf, in the `Ui` the tree bounded to `rect`, through the bindings above — so the
/// controls, their gestures and their names are exactly what they were.
pub(crate) fn paint(
    ui: &mut Ui,
    tokens: &Tokens,
    leaf: &Leaf,
    rect: egui::Rect,
    live: &mut Live<'_, '_>,
) {
    let params = live.params;
    match *leaf {
        // Synced to a tempo, a knob reads its division; the host still reads its value.
        Leaf::Knob(id) => {
            let size = knob_size(id);
            let bound = binding_for(id, params);
            let division = {
                use nice_plug::prelude::Param as _;
                let synced: Option<(bool, &nice_plug::prelude::FloatParam, mxm_tempo::Ladder)> =
                    match id {
                        "lforate" => Some((
                            params.lfo_sync.value(),
                            &params.lfo_rate,
                            crate::params::LFO_SYNC,
                        )),
                        "shtime" => Some((
                            params.sh_sync.value(),
                            &params.sh_time,
                            crate::params::SH_SYNC,
                        )),
                        _ => None,
                    };
                synced
                    .filter(|(on, _, _)| *on)
                    .and_then(|(_, param, ladder)| {
                        ladder.shown(
                            param.unmodulated_normalized_value(),
                            live.telemetry.tempo.get(),
                            f64::from(param.preview_plain(0.0)),
                            f64::from(param.preview_plain(1.0)),
                        )
                    })
            };
            match division {
                Some(division) => bound.knob_with_reading(
                    ui,
                    tokens,
                    live.setter,
                    size,
                    rect.width(),
                    live.entries,
                    division.label(),
                ),
                None => bound.knob(ui, tokens, live.setter, size, rect.width(), live.entries),
            }
        }
        Leaf::Picture(id) => {
            super::binding::sync_picture(
                ui,
                tokens,
                id,
                binding_for(id, params).param,
                live.setter,
            );
        }
        Leaf::Fader(id) => {
            let bound = binding_for(id, params);
            bound.slider_vertical(
                ui,
                tokens,
                live.setter,
                live.entries,
                bound.painted(),
                rect.width(),
                mxm_ui::control::FADER_HEIGHT,
            );
        }
        Leaf::Switch(id) => selector(ui, tokens, params, live.setter, id),
        Leaf::Toggle(id) => {
            let bound = binding_for(id, params);
            toggle_labelled(
                ui,
                tokens,
                id,
                bound.param,
                bound.painted(),
                bound.description,
                live.setter,
                0.0,
            );
        }
        Leaf::Caption(ref text) => caption(ui, tokens, text),
        Leaf::Routes(target) => routes(ui, tokens, target, params, live.setter, live.entries),
        // Where the voice is now shows only while a note sounds: at rest nothing has been played
        // to show.
        Leaf::FilterResponse => visuals::filter_response(
            ui,
            tokens,
            params.cutoff.value(),
            live.telemetry
                .sounding()
                .then(|| live.telemetry.voice_snapshot()[0].max(1.0)),
            params.resonance.value(),
        ),
        // The scope's time base follows the sample time the voice is running, synced or free.
        Leaf::SampleHold => {
            let time_s = params
                .synced_sh_time(live.telemetry.tempo.get())
                .unwrap_or_else(|| params.sh_time.value());
            visuals::sample_hold(ui, tokens, live.telemetry, params.sh_source.value(), time_s);
            // A frame each refresh while it moves; at rest it holds still and the editor's
            // 50 ms cadence is enough.
            if live.telemetry.sounding() {
                ui.ctx().request_repaint();
            }
        }
        Leaf::ClearOverload => {
            if ui.button(OVERLOAD).clicked() {
                live.telemetry.clear_overload();
            }
        }
    }
}

fn wave_options(id: &str) -> Option<&'static [(Wave, &'static str)]> {
    match id {
        "vco1wave" | "vco2wave" => Some(&[
            (Wave::Triangle, "Triangle"),
            (Wave::RampUp, "Sawtooth"),
            (Wave::Square, "Square"),
            (Wave::Pulse, "Pulse"),
        ]),
        // This LFO's saw **falls** (`mxm-para-07-dsp`'s `lfo.rs`: `1 - 2p`), so it is `RampDown`.
        "lfoshape" => Some(&[
            (Wave::RampDown, "Sawtooth"),
            (Wave::Square, "Square"),
            (Wave::Sine, "Sine"),
        ]),
        // The sample-and-hold's inputs: the LFO's falling saw, its triangle, and raw noise.
        "shsource" => Some(&[
            (Wave::RampDown, "LFO saw"),
            (Wave::Triangle, "LFO triangle"),
            (Wave::Noise, "Random"),
        ]),
        _ => None,
    }
}

/// A stepped parameter's cells, **labelled by the parameter itself**: each its option's own
/// formatted value, so a cell reads what the host's automation list reads.
fn options(params: &MxmPara07Params, id: &'static str) -> Vec<String> {
    let param = binding_for(id, params).param;
    let last = param
        .steps()
        .unwrap_or_else(|| panic!("{id} is not a selector"));
    (0..=last)
        .map(|option| param.format(option as f32 / last as f32))
        .collect()
}

/// What a control **paints**, where its card already says the rest (design system §7.1): *Rate*
/// on the LFO, *Attack* on an envelope, *Range* on an oscillator, *32'* on the register bank.
/// `None` paints the parameter's own name, which a host, a tooltip and a screen reader always read.
/// A card holding two modules keeps both prefixes.
fn panel_label(id: &str) -> Option<&'static str> {
    match id {
        "lfoshape" => Some("Shape"),
        "lforate" => Some("Rate"),
        "lfodelay" => Some("Delay"),
        "lfokeytrigger" => Some("Keyboard trigger"),
        // The envelopes' faders, by the convention (the owner, 2026-09-25).
        "env1attack" | "env2attack" => Some("A"),
        "env1decay" | "env2decay" => Some("D"),
        "env1sustain" | "env2sustain" => Some("S"),
        "env1release" | "env2release" => Some("R"),
        "env1trigger" | "env2trigger" => Some("Trigger"),
        "shsource" => Some("Source"),
        "shtime" => Some("Time"),
        "shlag" => Some("Lag"),
        "vco1width" | "vco2width" => Some("Pulse width"),
        "vco1range" | "vco2range" => Some("Range"),
        "vco1wave" | "vco2wave" => Some("Wave"),
        "vco2tune" => Some("Tune"),
        "reg32" => Some("32'"),
        "reg16" => Some("16'"),
        "reg8" => Some("8'"),
        "reg4" => Some("4'"),
        "reg2" => Some("2'"),
        "bank" => Some("Level"),
        _ => None,
    }
}

/// How the keyboard steps a parameter: a semitone and an octave in hertz for the two
/// filter corners,
/// the owner's ruling of 2026-09-23. Everything not named keeps its own step. See
/// [`crate::editor::binding::StepLaw`].
fn step_law(id: &str) -> super::binding::StepLaw {
    use super::binding::StepLaw;
    match id {
        "hpf" | "cutoff" => StepLaw::Hertz,
        _ => StepLaw::Own,
    }
}

/// One parameter and the plain-language sentence required in its tooltip.
pub fn binding_for<'a>(id: &'static str, params: &'a MxmPara07Params) -> Bound<'a> {
    let param = crate::params::all_parameters(params)
        .into_iter()
        .find_map(|(candidate, param)| (candidate == id).then_some(param))
        .unwrap_or_else(|| panic!("no parameter {id}"));
    Bound {
        id,
        param,
        description: description(id),
        panel: panel_label(id).map(std::borrow::Cow::Borrowed),
        bipolar: matches!(id, "tune" | "vco2tune"),
        law: step_law(id),
        stepped: None,
        details: details_of(id),
    }
}

/// What each option of a stepped control does, one sentence per cell in the parameter's own order
/// (design system §7.3; the owner, 2026-09-27: the cells of a row do not share one sentence).
/// Empty for everything drawn as a knob or a toggle.
fn details_of(id: &str) -> &'static [&'static str] {
    match id {
        "keymode" => &[
            "VCO-1 plays your highest key and VCO-2 your lowest: two notes at once.",
            "Both oscillators play your highest key.",
        ],
        "gatesource" => &[
            "Your keys start the envelopes.",
            "The sample and hold's clock starts them, so the sound plays by itself.",
        ],
        "portamentomode" => &[
            "Glides both up and down.",
            "Glides only when the next note is higher.",
            "Glides only when the next note is lower.",
        ],
        "vco1range" | "vco2range" => &[
            "Two octaves below the note played.",
            "One octave below the note played.",
            "The note as played.",
            "One octave above the note played.",
            "Two octaves above the note played.",
        ],
        "vco1wave" | "vco2wave" => &[
            "Soft and round: a few quiet odd harmonics.",
            "Bright and buzzy: every harmonic.",
            "Hollow: odd harmonics only.",
            "Thin and nasal; Pulse width sets how thin.",
        ],
        "noisecolour" => &["Bright, hissing noise.", "Darker, softer noise."],
        "env1trigger" | "env2trigger" => &[
            "Every new key restarts the envelope.",
            "Only the first key starts it; it restarts after all keys are up.",
            "While a key is held, the LFO restarts it on every cycle.",
        ],
        "lfoshape" => &[
            "Falls, then jumps back up: a repeating downward sweep.",
            "Jumps between two values, like a trill.",
            "A smooth wobble, and the only shape Delay fades in.",
        ],
        "shsource" => &[
            "Samples the LFO's falling saw: steps that walk down and repeat.",
            "Samples the LFO's triangle: steps that walk up and down.",
            "Samples noise: a new random step every time.",
        ],
        "autobenddirection" => &[
            "Each note swoops down into its pitch.",
            "Each note swoops up into its pitch.",
        ],
        _ => &[],
    }
}

/// A control's tooltip: **what it does to the sound, in the player's words** (design system §7.6)
/// — never the circuit's or the research's vocabulary.
fn description(id: &str) -> &'static str {
    match id {
        "keymode" => {
            "Two-pitch: VCO-1 plays your highest key and VCO-2 your lowest. One-pitch: both play \
             the highest."
        }
        "gatesource" => {
            "What starts the envelopes: your keys, or the sample-and-hold clock, so the sound \
             plays by itself."
        }
        "triggerinput" => "Starts the envelopes once each time it is switched on.",
        "portamento" => "Glide time between notes; zero turns glide off.",
        "portamentomode" => "Glide in both directions, only going up, or only going down.",
        "tune" => "Tunes the whole instrument; VCO-2 follows, keeping its own offset.",
        "vco1range" | "vco2range" => "The oscillator's octave.",
        "vco1wave" | "vco2wave" => "The oscillator's waveform.",
        "vco1width" | "vco2width" => {
            "How narrow the pulse is, from square to thin; modulation moves it from here."
        }
        "vco2tune" => {
            "Tunes VCO-2 against VCO-1: a little detunes and beats, more makes an interval."
        }
        "sync" => "Locks VCO-2 to VCO-1 for the hard-sync sound; retune VCO-2 to sweep it.",
        "reg32" | "reg16" | "reg8" | "reg4" | "reg2" => {
            "Level of this octave in the register bank, an organ-like stack of square waves."
        }
        "bank" => "Level of the register bank in the mixer.",
        "vco1" | "vco2" | "noise" => "Level of this source in the mixer.",
        "fifth" => "Level of the ring modulator in the mixer: a metallic, bell-like tone.",
        "noisecolour" => "Bright white noise, or darker pink noise.",
        "hpf" => "Takes low end away before the main filter.",
        "cutoff" => "The filter's cutoff: lower is darker.",
        "resonance" => "Emphasis at the cutoff; turned up far, the filter whistles on its own.",
        "env1attack" | "env2attack" => "How long the envelope takes to rise.",
        "env1decay" | "env2decay" => "How long it takes to fall to the sustain level.",
        "env1sustain" | "env2sustain" => "The level held while a key is down.",
        "env1release" | "env2release" => "How long it takes to fade after the key is released.",
        "env1trigger" | "env2trigger" => {
            "When the envelope restarts: on every new key, only after all keys are up, or over \
             and over at the LFO's rate while a key is held."
        }
        "hold" => "Keeps the amplifier open with no key, for drones.",
        "lfoshape" => "The LFO's shape.",
        "lforate" => "How fast the LFO runs.",
        "lfosync" | "shsync" => super::binding::SYNC_DESCRIPTION,
        "lfodelay" => "Fades the LFO in when you start playing; only the sine shape fades.",
        "lfokeytrigger" => "Restarts the LFO on every key.",
        "shsource" => {
            "What the sample-and-hold samples: the LFO's saw or triangle, or noise for random \
             steps."
        }
        "shtime" => "Time between the sample-and-hold's steps.",
        "shlag" => "Smooths the sample-and-hold's steps into glides.",
        "autobendtime" => "How long the pitch swoop at the start of each note lasts.",
        "autobenddirection" => "Whether each note swoops up into its pitch or down into it.",
        "volume" => "The instrument's output level.",
        _ => "Sets this part of the sound.",
    }
}

#[cfg(test)]
pub const ALL_IDS: &[&str] = &[
    "keymode",
    "gatesource",
    "triggerinput",
    "portamento",
    "portamentomode",
    "tune",
    "vco1range",
    "vco1wave",
    "vco1width",
    "vco2range",
    "vco2wave",
    "vco2width",
    "vco2tune",
    "sync",
    "reg32",
    "reg16",
    "reg8",
    "reg4",
    "reg2",
    "bank",
    "vco1",
    "vco2",
    "noise",
    "fifth",
    "noisecolour",
    "hpf",
    "cutoff",
    "resonance",
    "env1attack",
    "env1decay",
    "env1sustain",
    "env1release",
    "env1trigger",
    "env2attack",
    "env2decay",
    "env2sustain",
    "env2release",
    "env2trigger",
    "hold",
    "lfoshape",
    "lforate",
    "lfosync",
    "lfodelay",
    "lfokeytrigger",
    "shsource",
    "shtime",
    "shsync",
    "shlag",
    "autobendtime",
    "autobenddirection",
    "volume",
];

#[cfg(test)]
mod tests {
    use super::*;
    use nice_plug::prelude::Params;

    #[test]
    fn dense_continuous_controls_use_compact_without_reducing_the_pointer_floor() {
        let params = MxmPara07Params::default();
        for id in ALL_IDS {
            if binding_for(id, &params).param.steps().is_some()
                || wave_options(id).is_some()
                || matches!(*id, "triggerinput" | "sync" | "lfokeytrigger")
            {
                continue;
            }
            let size = knob_size(id);
            if matches!(*id, "cutoff" | "resonance") {
                assert_eq!(size, Size::Primary, "{id} lost primary hierarchy");
            } else if ladder_of(id).is_some() {
                // A syncable rate or time always shows its reading: hertz or seconds free, the
                // note when synced (the owner, 2026-09-27).
                assert!(
                    size.value_always_visible(),
                    "{id} is syncable and hides its reading"
                );
            } else {
                assert_eq!(size, Size::Compact, "{id} is not compact");
            }
            assert!(size.diameter() >= mxm_ui::space::MIN_TARGET);
        }
    }

    /// Every parameter is owned exactly once: by one task card, or by the app bar.
    #[test]
    fn every_parameter_has_one_owner_and_a_tooltip_sentence() {
        let params = MxmPara07Params::default();
        let declared: Vec<_> = params
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            // A routing parameter is drawn by its target's stack, not by this list.
            .filter(|id| !id.starts_with("mod_"))
            .collect();
        let drawn: Vec<_> = super::super::SECTIONS
            .iter()
            .flat_map(|section| section.parameters())
            .chain(super::super::BAR_PARAMETERS)
            .copied()
            .collect();
        for id in &declared {
            assert_eq!(
                drawn.iter().filter(|candidate| *candidate == id).count(),
                1,
                "{id} is not owned by exactly one task card or the app bar"
            );
            assert!(
                description(id).ends_with('.'),
                "{id} has no tooltip sentence"
            );
        }
        assert_eq!(drawn.len(), declared.len());
        assert_eq!(ALL_IDS.len(), declared.len());
    }
}
