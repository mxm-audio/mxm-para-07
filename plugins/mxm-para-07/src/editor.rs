//! mxm-para-07's software-native editor, implementing `docs/briefs/mxm-para-07.md`.
//!
//! Thirteen cards, each with one primary category, on pages the collection's paging renderer
//! derives from the space available: the keyboard first, then the modulation system, the pitch
//! sources and the shared audio path. The master Volume is the app bar's (design system §3.1), and
//! every other permanent parameter appears exactly once across the cards.
//! This is a panel, not a window: [`panel`] draws into a `Ui`, while nice-plug owns the transient
//! floating host window.
//!
//! Every parameter edit is bracketed once in [`binding::Bound`], and every control names itself to
//! the keyboard cursor there. DSP publishes atomics through `telemetry.rs`; DSP never reads this
//! module's page, browser, cursor or text state.

pub(crate) mod binding;
pub(crate) mod sections;
mod visuals;

use std::collections::HashMap;
use std::sync::Arc;

use egui::Ui;
use mxm_ui::paging::{Category, Key};
use mxm_ui::space::SPACE_5;
use mxm_ui::theme::Tokens;
use nice_plug::context::gui::GuiContext;
use nice_plug::prelude::*;
use nice_plug_egui::{EguiEditorState, NiceEguiApp, create_egui_editor};

use crate::params::MxmPara07Params;
use crate::telemetry::Telemetry;

/// The opening frame: **the quarter-4K budget, hugged** (owner, 2026-09-09) — the panel laid out at
/// the budget, less the slack around what it drew. Measured by
/// `tests::the_opening_size_is_the_budget_hugged`, not chosen.
const REFERENCE: (u32, u32) = (1721, 1064);

/// The window floor: the wider of the widest card floor with the workspace's two gutters, and the app
/// bar at its last compact step. Neither is chosen: `the_window_minimum_holds_one_widest_card_and_its_gutters`
/// holds the cards and `the_app_bar_holds_in_the_minimum_window` measures the bar, which decides it.
const MINIMUM: (u32, u32) = (446, 320);

/// The keyboard cursor's card for the app bar's Volume, outside the paging keys.
const VOLUME_CARD: u64 = 64;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum Section {
    Oscillator1,
    Oscillator2,
    Registers,
    Mixer,
    Filter,
    Lfo,
    SampleHold,
    Envelope1,
    Envelope2,
    AmplifierMod,
    /// Key mode, gate source and trigger input: how the keys play the voice.
    Keyboard,
    /// Portamento and auto bend, one card (R2's call, the owner's to overrule): each is a timed
    /// pitch motion, a time and a direction, and hugged, Auto bend was one knob and a switch.
    Portamento,
    Bender,
}

/// The one ownership list, and the stable inventory the pages are derived from: category-first,
/// then signal-chain order inside each category, so a card's index here is its paging [`Key`] and
/// its place in the derived order. How the keys play the voice leads, before either oscillator.
pub(crate) const SECTIONS: &[Section] = &[
    // Performance
    Section::Keyboard,
    Section::Portamento,
    // Modulators
    Section::Lfo,
    Section::SampleHold,
    Section::Envelope1,
    Section::Envelope2,
    Section::AmplifierMod,
    Section::Bender,
    // Generators
    Section::Oscillator1,
    Section::Oscillator2,
    Section::Registers,
    // Tone
    Section::Mixer,
    Section::Filter,
];

/// Parallel branches stay side by side while a row can hold them: the two envelopes and the two
/// oscillators. Each pair is contiguous and inside one category, as the planner requires; at a
/// width that cannot hold a pair it splits rather than squeeze either card under its floor.
const GROUPS: &[&[Key]] = &[&[Key(4), Key(5)], &[Key(8), Key(9)]];

impl Section {
    pub(crate) const fn title(self) -> &'static str {
        match self {
            Self::Oscillator1 => "Oscillator 1",
            Self::Oscillator2 => "Oscillator 2",
            Self::Registers => "Register bank",
            Self::Mixer => "Mixer",
            Self::Filter => "Filter",
            Self::Lfo => "LFO",
            Self::SampleHold => "Sample & Hold",
            Self::Envelope1 => "Envelope 1",
            Self::Envelope2 => "Envelope 2",
            Self::AmplifierMod => "Amplifier modulation",
            Self::Keyboard => "Keyboard",
            Self::Portamento => "Portamento and auto bend",
            Self::Bender => "Bender routes",
        }
    }

    /// The card's one primary category (mxm-kit's `docs/plugin-conventions.md`, *Editor contract*).
    /// Bender routes is mixed-purpose and
    /// placed by what it decides: the lever's destination depths, including rectified LFO depth,
    /// beside the other destination depths.
    const fn category(self) -> Category {
        match self {
            Self::Keyboard | Self::Portamento => Category::Performance,
            Self::Lfo
            | Self::SampleHold
            | Self::Envelope1
            | Self::Envelope2
            | Self::AmplifierMod
            | Self::Bender => Category::Modulators,
            Self::Oscillator1 | Self::Oscillator2 | Self::Registers => Category::Generators,
            Self::Mixer | Self::Filter => Category::Tone,
        }
    }

    #[cfg(test)]
    pub(crate) const fn parameters(self) -> &'static [&'static str] {
        match self {
            Self::Oscillator1 => &["vco1range", "vco1wave", "tune", "vco1width"],
            Self::Oscillator2 => &["vco2range", "vco2wave", "vco2tune", "vco2width", "sync"],
            Self::Registers => &["reg32", "reg16", "reg8", "reg4", "reg2", "bank"],
            Self::Mixer => &["vco1", "vco2", "noise", "fifth", "noisecolour"],
            Self::Filter => &["hpf", "cutoff", "resonance"],
            Self::Lfo => &[
                "lfoshape",
                "lforate",
                "lfosync",
                "lfodelay",
                "lfokeytrigger",
            ],
            Self::SampleHold => &["shsource", "shtime", "shsync", "shlag"],
            Self::Envelope1 => &[
                "env1attack",
                "env1decay",
                "env1sustain",
                "env1release",
                "env1trigger",
            ],
            Self::Envelope2 => &[
                "env2attack",
                "env2decay",
                "env2sustain",
                "env2release",
                "env2trigger",
            ],
            Self::AmplifierMod => &["hold"],
            Self::Keyboard => &["keymode", "gatesource", "triggerinput"],
            Self::Portamento => &[
                "portamento",
                "portamentomode",
                "autobendtime",
                "autobenddirection",
            ],
            Self::Bender => &[],
        }
    }
}

/// Which parameters the app bar draws: the master Volume, beside the level meter (design system
/// §3.1 slot 6). It belongs to no card.
#[cfg(test)]
pub(crate) const BAR_PARAMETERS: &[&str] = &["volume"];

pub fn create(params: Arc<MxmPara07Params>, telemetry: Arc<Telemetry>) -> Option<MxmPara07Editor> {
    let state = EguiEditorState::from_size(
        nice_plug::editor::dpi::LogicalSize::new(REFERENCE.0, REFERENCE.1),
        1.0,
    );
    create_egui_editor(
        state,
        nice_plug_egui::RepaintNotifier::new(),
        nice_plug_egui::EguiNiceSettings {
            title: crate::NAME.to_owned(),
            resize_hint: ResizeHint {
                size_constraints: nice_plug::editor::SizeConstraints::min_logical_size(
                    nice_plug::editor::dpi::LogicalSize::new(MINIMUM.0 as f32, MINIMUM.1 as f32),
                ),
                ..ResizeHint::RESIZABLE
            },
            ..Default::default()
        },
        MxmPara07App::new(params, telemetry),
    )
}

pub type MxmPara07Editor = nice_plug_egui::EguiEditor<MxmPara07App>;
pub use mxm_preset::PresetUi;

pub struct MxmPara07App {
    params: Arc<MxmPara07Params>,
    telemetry: Arc<Telemetry>,
    gui_context: Option<GuiContext>,
    view: usize,
    text_entry: HashMap<&'static str, Option<String>>,
    presets: PresetUi,
    /// Where the keyboard is: a card, and a parameter inside it. Transient, like the text buffers —
    /// it is not a parameter and nothing durable reads it.
    nav: mxm_ui::navigation::State,
}

impl MxmPara07App {
    pub fn new(params: Arc<MxmPara07Params>, telemetry: Arc<Telemetry>) -> Self {
        let presets = PresetUi::new(params.as_ref());
        Self {
            params,
            telemetry,
            gui_context: None,
            view: 0,
            text_entry: HashMap::new(),
            presets,
            nav: mxm_ui::navigation::State::default(),
        }
    }
}

impl NiceEguiApp for MxmPara07App {
    fn build(
        &mut self,
        egui_ctx: egui::Context,
        nice_gui_ctx: GuiContext,
        _frame: &mut nice_plug_egui::Frame,
    ) -> Result<(), nice_plug_egui::baseview::HandlerError> {
        mxm_ui::theme::apply(&egui_ctx);
        mxm_ui::typography::apply(&egui_ctx);
        // Light by default, overridable with `MXM_EDITOR_THEME`; the reasoning, and why the
        // default is not `System`, lives on `mxm_ui::theme::preference`.
        egui_ctx.set_theme(mxm_ui::theme::preference());
        self.gui_context = Some(nice_gui_ctx);
        self.telemetry.set_editor_open(true);
        Ok(())
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut nice_plug_egui::Frame) {
        let Some(gui_context) = self.gui_context.clone() else {
            return;
        };
        panel(
            ui,
            &self.params,
            &self.telemetry,
            &gui_context.param_setter(),
            &mut self.view,
            &mut self.text_entry,
            &mut self.presets,
            &mut self.nav,
        );
    }

    fn editor_closed(&mut self) {
        self.gui_context = None;
        self.telemetry.set_editor_open(false);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn panel(
    ui: &mut Ui,
    params: &MxmPara07Params,
    telemetry: &Telemetry,
    setter: &ParamSetter<'_>,
    view: &mut usize,
    text_entry: &mut HashMap<&'static str, Option<String>>,
    presets: &mut PresetUi,
    nav: &mut mxm_ui::navigation::State,
) {
    // A frame every 50 ms while open: telemetry and developer requests change between input events.
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(50));

    // One question, and both layers suspend on it: does another surface own this frame's keyboard?
    let busy = presets.holds_the_keyboard() || text_entry.values().any(Option::is_some);
    mxm_ui::paging::editor::hold(ui.ctx(), busy);
    mxm_ui::paging::editor::developer_request(ui.ctx(), view, telemetry.take_view_request());
    // The cursor moves before anything is drawn, so a navigation arrow is consumed here rather than
    // also reaching a control. The Parameters surface has no cards: stop the cursor there, or its
    // sliders lose bare-arrow editing to an invisible stale musician cursor — the app bar's Volume
    // card included.
    if *view == mxm_ui::paging::PARAMETERS {
        mxm_ui::navigation::stop(ui.ctx());
    } else {
        mxm_ui::navigation::paged_with_bar(ui.ctx(), nav, busy, &[VOLUME_CARD]);
    }
    if let Some(open) = telemetry.take_browser_request() {
        presets.set_browser_open(open);
    }
    // Applied and never stored: a capture run must not rewrite the choice made in the control.
    if let Some(preference) = telemetry
        .take_theme_request()
        .and_then(mxm_ui::theme::from_index)
    {
        ui.ctx().set_theme(preference);
    }
    // This editor has no disclosure: CC 118's request is consumed and changes nothing
    // (mxm-kit's `docs/plugin-conventions.md`, *A developer channel in every editor*).
    let _ = telemetry.take_disclosure_request();

    let tokens = tokens_for(ui);
    let peak = telemetry.take_peak();
    let clipped = telemetry.clipped();
    mxm_ui::AppBar::new(crate::NAME).show_with(
        ui,
        &tokens,
        |ui| mxm_preset::ui::preset_row(ui, &tokens, params, setter, presets),
        |ui| {
            if mxm_ui::shell::level_meter(ui, &tokens, peak, clipped) {
                telemetry.clear_clip();
            }
            // Design system §3.1 slot 6: the master output sits beside its meter.
            mxm_ui::navigation::bar_card(ui, VOLUME_CARD, |ui| {
                ui.scope(|ui| {
                    sections::binding_for("volume", params)
                        .slider_inline(ui, &tokens, setter, text_entry, 96.0);
                })
                .response
                .rect
            });
            mxm_ui::shell::zoom_control(ui);
            mxm_ui::shell::editor_theme_control(ui);
        },
    );
    mxm_preset::ui::overlays(ui, &tokens, params, setter, presets);

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(tokens.canvas)
                .inner_margin(egui::Margin::same(SPACE_5 as i8)),
        )
        .show(ui, |ui| {
            if *view == mxm_ui::paging::PARAMETERS {
                parameters_view(ui, &tokens, params, setter, text_entry);
            } else {
                paged_view(ui, &tokens, params, telemetry, setter, text_entry);
            }
        });
}

/// The inventory the paging renderer plans: stable keys, floors and each card's primary category.
/// Each floor is **computed from the card's tree** in `ui`'s fonts (plans/plan-layout-tree.md) — the
/// tree's narrowest plus the card's chrome, every route revealed at its widest reading — and each
/// card is exactly as wide as it: its ceiling is its floor (`plans/plan-editor-standard.md` A1).
pub(crate) fn page_items(
    ui: &Ui,
    params: &MxmPara07Params,
    readings: &sections::Readings,
) -> Vec<mxm_ui::paging::Item<'static>> {
    SECTIONS
        .iter()
        .enumerate()
        .map(|(index, section)| {
            let content = mxm_ui::tree::card_floor(
                ui,
                section.title(),
                &sections::card(ui, *section, params, readings),
            );
            mxm_ui::paging::Item {
                key: Key(index as u64),
                card: mxm_ui::flow::Card::new(section.title(), content).capped(content),
                category: section.category(),
                kind: section.title(),
            }
        })
        .collect()
}

fn paged_view(
    ui: &mut Ui,
    tokens: &Tokens,
    params: &MxmPara07Params,
    telemetry: &Telemetry,
    setter: &ParamSetter<'_>,
    entries: &mut HashMap<&'static str, Option<String>>,
) {
    // What the cards print from telemetry, and whether the Mixer carries its overload button, is
    // read once, before anything is built, so the tree measured is the tree painted.
    let readings = sections::Readings::of(telemetry);
    let items = page_items(ui, params, &readings);
    let text_editing = entries.values().any(Option::is_some);
    let mut live = sections::Live {
        params,
        telemetry,
        setter,
        entries,
    };
    mxm_ui::paging::editor::show(
        ui,
        tokens,
        &items,
        GROUPS,
        text_editing,
        &mut |ui, index| sections::card(ui, SECTIONS[index], params, &readings),
        &mut |ui, _, leaf, rect| sections::paint(ui, tokens, leaf, rect, &mut live),
    );
}

/// The paging items as the editor computes them, from a context set up as an editor's is — three
/// passes in, so the weighted font cuts are bound — for tests, which have no editor `Ui` to hand.
#[cfg(test)]
pub(crate) fn test_items() -> Vec<mxm_ui::paging::Item<'static>> {
    let ctx = egui::Context::default();
    mxm_ui::typography::apply(&ctx);
    mxm_ui::theme::apply(&ctx);
    let params = MxmPara07Params::default();
    let readings = sections::Readings::of(&Telemetry::default());
    let mut items = Vec::new();
    for _ in 0..3 {
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            items = page_items(ui, &params, &readings);
        });
        output.textures_delta.clear();
    }
    items
}

/// The cards' floors in [`SECTIONS`] order, as [`test_items`] computes them.
#[cfg(test)]
pub(crate) fn test_floors() -> Vec<f32> {
    test_items().iter().map(|item| item.card.floor).collect()
}

/// The developer Parameters surface: every parameter as a slider, in declaration order. It has no
/// tab; developer CC 119 value 127 reaches it (`plugins/AGENTS.md`).
fn parameters_view(
    ui: &mut Ui,
    tokens: &Tokens,
    params: &MxmPara07Params,
    setter: &ParamSetter<'_>,
    text_entry: &mut HashMap<&'static str, Option<String>>,
) {
    mxm_ui::shell::scroll_list(ui).show(ui, |ui| {
        let columns = if ui.available_width() >= 1000.0 { 3 } else { 2 };
        let entries: Vec<_> = crate::params::all_parameters(params)
            .into_iter()
            .map(|(id, _)| sections::binding_for(id, params))
            .collect();
        let per_column = entries.len().div_ceil(columns);
        ui.columns(columns, |uis| {
            for (index, chunk) in entries.chunks(per_column).enumerate() {
                let Some(column) = uis.get_mut(index) else {
                    continue;
                };
                for entry in chunk {
                    entry.slider(column, tokens, setter, text_entry);
                }
            }
        });
    });
}

fn tokens_for(ui: &Ui) -> Tokens {
    let dark = ui.visuals().dark_mode;
    let base = if dark { mxm_ui::DARK } else { mxm_ui::LIGHT };
    base.with_identity(mxm_ui::theme::LEAF_GREEN, dark)
}

#[cfg(test)]
mod tests {
    use mxm_plugin_test::keyboard_checks;
    use mxm_plugin_test::{opening_size, paging_checks};

    use super::*;
    use kittest::Queryable;
    use nice_plug::params::internals::ParamPtr;
    use nice_plug::prelude::{PluginApi, PluginState};

    struct NoHost;
    impl nice_plug::context::gui::GuiContextInner for NoHost {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        unsafe fn raw_begin_set_parameter(&self, _param: ParamPtr) {}
        unsafe fn raw_set_parameter_normalized(&self, _param: ParamPtr, _normalized: f32) {}
        unsafe fn raw_end_set_parameter(&self, _param: ParamPtr) {}
        fn get_state(&self) -> PluginState {
            PluginState {
                version: String::new(),
                params: Default::default(),
                fields: Default::default(),
            }
        }
        fn set_state(&self, _state: PluginState) {}
    }

    /// Everything `panel` borrows between frames, owned by one test.
    struct Rig {
        params: MxmPara07Params,
        telemetry: Arc<Telemetry>,
        view: usize,
        text_entry: HashMap<&'static str, Option<String>>,
        presets: PresetUi,
        nav: mxm_ui::navigation::State,
    }

    impl Rig {
        fn new() -> Self {
            let params = MxmPara07Params::default();
            let presets = PresetUi::at(mxm_preset::Library::at(None), &params);
            Self {
                params,
                telemetry: Arc::new(Telemetry::default()),
                view: 0,
                text_entry: HashMap::new(),
                presets,
                nav: mxm_ui::navigation::State::default(),
            }
        }

        fn draw(&mut self, ui: &mut Ui, setter: &ParamSetter<'_>) {
            panel(
                ui,
                &self.params,
                &self.telemetry,
                setter,
                &mut self.view,
                &mut self.text_entry,
                &mut self.presets,
                &mut self.nav,
            );
        }
    }

    fn key_of(section: Section) -> Key {
        let index = SECTIONS
            .iter()
            .position(|candidate| *candidate == section)
            .expect("every section is in the inventory");
        Key(index as u64)
    }

    /// **The editor opens at the quarter-4K budget, hugged** — the owner's rule, 2026-09-09.
    #[test]
    fn the_opening_size_is_the_budget_hugged() {
        let host = NoHost;
        let setter = ParamSetter::new(&host);
        let mut rig = Rig::new();
        opening_size::is_the_budget_hugged(
            egui::vec2(REFERENCE.0 as f32, REFERENCE.1 as f32),
            &|_| {},
            &mut |ui| rig.draw(ui, &setter),
        );
    }

    /// **The app bar holds in the narrowest window**: its `…` menu whole and nothing drawn over
    /// anything else, from `MINIMUM` up (`opening_size::bar_holds_from_the_minimum`).
    #[test]
    fn the_app_bar_holds_in_the_minimum_window() {
        let host = NoHost;
        let setter = ParamSetter::new(&host);
        let mut rig = Rig::new();
        opening_size::bar_holds_from_the_minimum(
            egui::vec2(MINIMUM.0 as f32, MINIMUM.1 as f32),
            &mut |ui| rig.draw(ui, &setter),
        );
    }

    /// A control whose `navigation::at` scope was forgotten paints exactly as before and is simply
    /// unreachable from the keyboard. Nothing else would say so.
    fn reveal_every_route(params: &MxmPara07Params) {
        use nice_plug::params::InternalParamMut;
        for group in params.routes.each() {
            for source in 0..mxm_para_07_dsp::routing::SOURCES {
                // SAFETY: a test owns these parameters outright; nothing else holds them.
                unsafe { group.presence_param(source)._internal_set_plain_value(true) };
            }
        }
    }

    /// Every parameter the panel should draw: the cards' own, and a presence and an amount for
    /// every route that is present.
    fn drawn_ids(params: &MxmPara07Params) -> Vec<&'static str> {
        let mut ids: Vec<&'static str> = crate::params::all_parameters(params)
            .iter()
            .map(|(id, _)| *id)
            .collect();
        for (t, group) in params.routes.each().into_iter().enumerate() {
            for (s, present) in group.presences(t).into_iter().enumerate() {
                if present {
                    let (amount, presence) = crate::routes::ROUTE_IDS[t][s];
                    ids.push(amount);
                    ids.push(presence);
                }
            }
        }
        ids
    }

    #[test]
    fn the_keyboard_cursor_reaches_and_operates_every_parameter() {
        let host = keyboard_checks::Recorder::default();
        let setter = ParamSetter::new(&host);
        let mut rig = Rig::new();
        // **Every route present, as the `‹ modulate ›` menu would add them one at a time.** An
        // absent route draws nothing at all, so a check at the init patch would reach fourteen of
        // the hundred and sixty-one and call the other hundred and forty-seven covered.
        reveal_every_route(&rig.params);
        let ids = drawn_ids(&rig.params);
        keyboard_checks::the_cursor_reaches_and_operates(
            egui::vec2(REFERENCE.0 as f32, REFERENCE.1 as f32),
            &test_items(),
            keyboard_checks::Coverage::Exactly(&ids),
            &|_| {},
            &host,
            &mut |ui| rig.draw(ui, &setter),
        );
    }

    /// **Volume is the app bar's, and no card draws it** (design system §3.1 slot 6). Whichever
    /// card is requested, it registers exactly once, under the bar card, whose key no page uses.
    #[test]
    fn the_master_volume_is_drawn_once_in_the_app_bar() {
        let items = test_items();
        assert!(
            items.iter().all(|item| item.key.0 != VOLUME_CARD),
            "the bar card's key collides with a paging key"
        );
        let host = NoHost;
        let setter = ParamSetter::new(&host);
        let mut rig = Rig::new();
        let mut draw = |ui: &mut Ui| rig.draw(ui, &setter);
        let session =
            keyboard_checks::Session::new(egui::vec2(REFERENCE.0 as f32, REFERENCE.1 as f32));
        for item in &items {
            mxm_ui::paging::editor::request_card(session.context(), item.key);
            session.settle(&mut draw);
            let cards: Vec<u64> = mxm_ui::navigation::spots(session.context())
                .into_iter()
                .filter(|spot| spot.key == "volume")
                .map(|spot| spot.card)
                .collect();
            assert_eq!(
                cards,
                [VOLUME_CARD],
                "with {} requested, Volume registers under {cards:?}",
                item.card.title
            );
        }
    }

    /// Every derived page fits, in both themes at 1× and 2× with the simulated physical window
    /// fixed, at the opening size, a mid-sized window and the one-card minimum.
    #[test]
    fn every_dynamic_page_fits_and_every_card_is_reachable() {
        let host = NoHost;
        let setter = ParamSetter::new(&host);
        let mut rig = Rig::new();
        paging_checks::verify(
            &test_items(),
            &[
                egui::vec2(REFERENCE.0 as f32, REFERENCE.1 as f32),
                egui::vec2(1280.0, 800.0),
                egui::vec2(MINIMUM.0 as f32, MINIMUM.1 as f32),
            ],
            |ui| rig.draw(ui, &setter),
        );
    }

    /// **The parallel branches share a row wherever one can hold them, and every row ends level** —
    /// read off the drawn rectangles on a canvas tall enough to put all eighteen cards on one page,
    /// because the paged checks only ever see the cards of the page they asked for.
    #[test]
    fn parallel_branches_share_a_row_and_every_row_ends_level() {
        let host = NoHost;
        let setter = ParamSetter::new(&host);
        let mut rig = Rig::new();
        let ctx = egui::Context::default();
        mxm_ui::theme::apply(&ctx);
        mxm_ui::typography::apply(&ctx);
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(REFERENCE.0 as f32, 20_000.0),
            )),
            ..Default::default()
        };
        for _ in 0..4 {
            let mut output = ctx.run_ui(input.clone(), |ui| rig.draw(ui, &setter));
            output.textures_delta.clear();
        }
        let placed = paging_checks::all_rects(&ctx, SECTIONS.len());

        for group in GROUPS {
            let (first, second) = (group[0].0 as usize, group[1].0 as usize);
            let (a, b) = (placed[first], placed[second]);
            assert!(
                (a.top() - b.top()).abs() < 0.75 && a.right() <= b.left() + 0.75,
                "{} and {} do not share a row: {a:?} {b:?}",
                SECTIONS[first].title(),
                SECTIONS[second].title()
            );
        }
        for (index, a) in placed.iter().enumerate() {
            for b in placed.iter().skip(index + 1) {
                if (a.top() - b.top()).abs() < 0.75 {
                    assert!(
                        (a.bottom() - b.bottom()).abs() < 0.75,
                        "a row ends ragged: {a:?} {b:?}"
                    );
                }
            }
        }
    }

    /// The window's minimum holds one widest card and the workspace's two gutters. The app bar at
    /// its last compact step is wider and decides the number (`the_app_bar_holds_in_the_minimum_window`).
    #[test]
    fn the_window_minimum_holds_one_widest_card_and_its_gutters() {
        let widest = test_floors().into_iter().fold(0.0_f32, f32::max);
        let minimum = MINIMUM.0 as f32;
        assert!(
            minimum >= widest + 2.0 * SPACE_5,
            "the widest card's floor is {widest:.1}, so the window minimum is at least {:.0}, not {minimum}",
            (widest + 2.0 * SPACE_5).ceil()
        );
        assert!(MINIMUM.0 < REFERENCE.0);
    }

    /// A host that applies what it is sent, so a test can move parameters through the setter.
    #[derive(Default)]
    struct ApplyingHost(std::sync::Mutex<Vec<&'static str>>);

    impl nice_plug::context::gui::GuiContextInner for ApplyingHost {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        unsafe fn raw_begin_set_parameter(&self, _param: ParamPtr) {
            self.0.lock().unwrap().push("begin");
        }
        unsafe fn raw_set_parameter_normalized(&self, param: ParamPtr, normalized: f32) {
            self.0.lock().unwrap().push("set");
            unsafe { param._internal_set_normalized_value(normalized) };
        }
        unsafe fn raw_end_set_parameter(&self, _param: ParamPtr) {
            self.0.lock().unwrap().push("end");
        }
        fn get_state(&self) -> PluginState {
            PluginState {
                version: String::new(),
                params: Default::default(),
                fields: Default::default(),
            }
        }
        fn set_state(&self, _state: PluginState) {}
    }

    /// The voice telemetry that prints the longest text each display can show: a note sounding,
    /// the clamp's top cutoff, the widest sounding frequencies, full-scale S&H values and the clock
    /// high.
    fn longest_telemetry(telemetry: &Telemetry) {
        telemetry.publish_sounding(true);
        use mxm_para_07_dsp::voice::Telemetry as VoiceTelemetry;
        telemetry.publish_block(
            0.0,
            VoiceTelemetry {
                cutoff_hz: 20_000.0,
                vco1_hz: 19_999.9,
                vco2_hz: 19_999.9,
                sh_source_value: -1.0,
                sh_held: -1.0,
                sh_out: -1.0,
                sh_clock: true,
                ..VoiceTelemetry::default()
            },
        );
    }

    /// Every card, in every state that changes what it holds, passes the layout tree's checks
    /// (plans/plan-layout-tree.md §4.3, `tree_checks::card`): its floor holds its content with
    /// nothing painted outside the card, the content floor is exact, the height its tree states is
    /// the height it draws, and every leaf stays in the room it was given.
    ///
    /// The states are this editor's structural-state matrix: the init patch; every route revealed at
    /// full negative depth — the widest reading a row can show; the mixer overload latched, which
    /// adds its acknowledgement button; and the telemetry text at its longest. The floors are the Init floors in every state: a route stack's floor is already
    /// every route revealed, and a display states its extent for its longest text.
    #[test]
    fn every_card_passes_the_tree_checks_in_every_state() {
        let floors = test_floors();
        for state in [
            "init",
            "every route revealed",
            "the mixer overload latched",
            "telemetry text at its longest",
            "both syncs on, no tempo",
            "both syncs on at a tempo",
        ] {
            let params = MxmPara07Params::default();
            let host = ApplyingHost::default();
            let setter = ParamSetter::new(&host);
            if state == "every route revealed" {
                for (target, group) in params.routes.each().into_iter().enumerate() {
                    for route in &group.routes(target) {
                        mxm_modulation_params::add(route, &setter);
                        route.amount.set(&setter, 0.0);
                    }
                }
            }
            host.0.lock().unwrap().clear();
            let telemetry = Telemetry::default();
            if state == "the mixer overload latched" {
                telemetry.publish_block(
                    0.0,
                    mxm_para_07_dsp::voice::Telemetry {
                        overload: true,
                        ..Default::default()
                    },
                );
            }
            if state == "telemetry text at its longest" {
                longest_telemetry(&telemetry);
            }
            if state.starts_with("both syncs on") {
                // SAFETY: the parameters are this test's own and nothing else reads them.
                unsafe {
                    use nice_plug::params::InternalParamMut;
                    let _ = params.lfo_sync._internal_set_normalized_value(1.0);
                    let _ = params.sh_sync._internal_set_normalized_value(1.0);
                }
            }
            if state == "both syncs on at a tempo" {
                telemetry.tempo.publish(Some(120.0));
            }
            let readings = sections::Readings::of(&telemetry);
            let setup = |_: &egui::Context| {};
            for (index, section) in SECTIONS.iter().enumerate() {
                let mut entries = HashMap::new();
                let mut live = sections::Live {
                    params: &params,
                    telemetry: &telemetry,
                    setter: &setter,
                    entries: &mut entries,
                };
                tree_checks::card(
                    &setup,
                    state,
                    section.title(),
                    floors[index],
                    &|ui| sections::card(ui, *section, &params, &readings),
                    &mut |ui, leaf, rect| {
                        sections::paint(ui, &mxm_ui::LIGHT, leaf, rect, &mut live);
                    },
                );
            }
            assert!(
                host.0.lock().unwrap().is_empty(),
                "drawing a card emitted a host gesture"
            );
        }
    }

    #[test]
    fn the_keyboard_leads_and_the_inventory_is_category_first() {
        assert_eq!(&SECTIONS[..2], &[Section::Keyboard, Section::Portamento]);
        let items = test_items();
        assert_eq!(items[0].category, Category::Performance);
        assert!(
            items
                .windows(2)
                .all(|pair| pair[0].category <= pair[1].category),
            "SECTIONS is not category-first, so a card's key is not its place in the derived order"
        );
    }

    #[test]
    fn the_preferred_groups_are_the_parallel_branches() {
        let items = test_items();
        let sections: Vec<Vec<Section>> = GROUPS
            .iter()
            .map(|group| group.iter().map(|key| SECTIONS[key.0 as usize]).collect())
            .collect();
        assert_eq!(
            sections,
            [
                vec![Section::Envelope1, Section::Envelope2],
                vec![Section::Oscillator1, Section::Oscillator2],
            ]
        );
        for group in GROUPS {
            let category = items[group[0].0 as usize].category;
            assert!(
                group
                    .iter()
                    .all(|key| items[key.0 as usize].category == category),
                "a preferred group crosses a category: {group:?}"
            );
        }
    }

    /// **The displays are pictures, named for the accessibility tree** — no readings printed on
    /// them (the owner, 2026-09-27: *these numbers make little sense*).
    #[test]
    fn the_displays_are_named_and_print_no_readings() {
        use mxm_para_07_dsp::voice::Telemetry as VoiceTelemetry;

        let mut rig = Rig::new();
        rig.telemetry.publish_block(
            0.0,
            VoiceTelemetry {
                cutoff_hz: 1234.5,
                sh_source_value: -0.5,
                sh_held: 0.25,
                sh_out: 0.125,
                sh_clock: true,
                ..VoiceTelemetry::default()
            },
        );
        rig.telemetry.publish_sounding(true);
        let host = NoHost;
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(REFERENCE.0 as f32, REFERENCE.1 as f32))
            .build_ui(|ui| {
                mxm_ui::theme::apply(ui.ctx());
                mxm_ui::typography::apply(ui.ctx());
                let setter = ParamSetter::new(&host);
                rig.draw(ui, &setter);
            });
        harness.run_steps(4);

        for (section, name, gone) in [
            (
                Section::Filter,
                "Filter response: set and sounding",
                "Sounding cutoff: 1234.5 Hz (dashed)",
            ),
            (Section::SampleHold, "Sample and hold trace", "Held: 0.250"),
        ] {
            // Ask for the card rather than assuming which page the opening size puts it on.
            mxm_ui::paging::editor::request_card(&harness.ctx, key_of(section));
            harness.run_steps(4);
            assert!(
                harness.query_by_label(name).is_some(),
                "{} lacks its display",
                section.title()
            );
            assert!(
                harness.query_by_label(gone).is_none(),
                "{} still prints {gone}",
                section.title()
            );
        }
        // The bender reaches its destinations from their own stacks; what its card shows is the
        // multiplier module, whose rows read `<target> from <source>`.
        mxm_ui::paging::editor::request_card(&harness.ctx, key_of(Section::Bender));
        harness.run_steps(4);
        assert!(
            harness
                .query_by_label("Multiplier from Bend magnitude")
                .is_some()
        );
    }

    /// **No help text on the panel** (the owner, 2026-09-27; design system §7.6): the only caption
    /// a card may carry is a live reading. A sentence explaining a control is its tooltip.
    ///
    /// Falsified before trusted: restoring Portamento's caption fails it.
    #[test]
    fn no_card_prints_help_text() {
        let params = MxmPara07Params::default();
        let readings = sections::Readings::default();
        let mut harness = egui_kittest::Harness::builder().build_ui(|ui| {
            for section in SECTIONS {
                for key in sections::card(ui, *section, &params, &readings).keys() {
                    if let sections::Leaf::Caption(text) = key {
                        assert!(
                            text.is_empty() || text.starts_with("Sounding "),
                            "{} prints {text:?}",
                            section.title()
                        );
                    }
                }
            }
        });
        harness.run_steps(1);
    }

    #[test]
    fn developer_requests_reach_the_preset_browser_and_the_parameters_surface() {
        let mut rig = Rig::new();
        let telemetry = Arc::clone(&rig.telemetry);
        let host = NoHost;
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(REFERENCE.0 as f32, REFERENCE.1 as f32))
            .build_ui(|ui| {
                mxm_ui::theme::apply(ui.ctx());
                mxm_ui::typography::apply(ui.ctx());
                let setter = ParamSetter::new(&host);
                rig.draw(ui, &setter);
            });
        harness.run_steps(4);

        // This editor has no disclosure: CC 118's request is consumed and changes nothing.
        telemetry.request_disclosure(true);
        harness.run_steps(4);
        assert_eq!(
            telemetry.take_disclosure_request(),
            None,
            "the request was consumed"
        );

        telemetry.request_browser(true);
        harness.run_steps(4);
        assert!(harness.query_by_label("Banks").is_some());
        telemetry.request_browser(false);
        harness.run_steps(4);
        assert!(harness.query_by_label("Banks").is_none());

        assert!(
            !mxm_ui::navigation::spots(&harness.ctx).is_empty(),
            "the premise: the paged surface registers its controls with the cursor"
        );
        telemetry.request_view(mxm_ui::paging::PARAMETERS as u8);
        harness.run_steps(4);
        assert!(
            mxm_ui::navigation::spots(&harness.ctx).is_empty(),
            "the Parameters surface has no cards, so nothing registers with the cursor"
        );
        assert!(
            harness.query_by_label("Key mode").is_some(),
            "the Parameters surface lacks Key mode"
        );
        // Volume is named twice on this surface: once in its complete list, and once by the app
        // bar, which every view keeps.
        assert_eq!(
            harness.query_all_by_label("Volume").count(),
            2,
            "the Parameters surface lacks Volume beside the app bar's"
        );
    }

    use mxm_plugin_test::tree_checks;

    /// Every page at the opening size, light and dark, for the owner's review of the layout-tree
    /// conversion (plans/plan-layout-tree.md §4.3): `target/layout-tree/mxm-para-07/<tag>/`, where
    /// `MXM_PICTURES` names the tag — `before` on the unconverted editor, `after` on the tree.
    ///
    /// `MXM_PICTURES=after cargo test -p mxm-para-07 --lib tree_pictures -- --ignored`
    #[test]
    #[ignore = "renders through wgpu; run by hand"]
    fn tree_pictures() {
        let tag = std::env::var("MXM_PICTURES").unwrap_or_else(|_| "after".to_owned());
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/layout-tree/mxm-para-07")
            .join(tag);
        let host = NoHost;
        let setter = ParamSetter::new(&host);
        let mut rig = Rig::new();
        // A played scope, so the pictures show a trace rather than an empty frame: the Init sample
        // time's steps, sampling a 1.3 Hz saw.
        for k in 0..2_000 {
            let saw = |t: f32| 1.0 - 2.0 * (t * 1.3).rem_euclid(1.0);
            let t = k as f32 / crate::telemetry::SCOPE_RATE_HZ;
            rig.telemetry
                .push_scope(saw(t), saw((t / 0.2).floor() * 0.2));
        }
        tree_checks::pictures(
            &|_| {},
            egui::vec2(REFERENCE.0 as f32, REFERENCE.1 as f32),
            &dir,
            &mut |ui| rig.draw(ui, &setter),
        );
    }

    #[test]
    fn fresh_editor_state_is_the_first_page_with_no_private_transient_open() {
        let params = Arc::new(MxmPara07Params::default());
        let app = MxmPara07App::new(params, Telemetry::shared());
        assert_eq!(app.view, 0);
        assert!(app.text_entry.is_empty());
        assert!(!app.presets.is_browser_open());
        assert!(app.nav.card().is_none());
        assert!(app.gui_context.is_none());
    }
}
