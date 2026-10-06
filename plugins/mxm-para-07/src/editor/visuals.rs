//! The editor's two displays: the filter's response and the sample and hold's scope.
//!
//! **Pictures, not readouts** (the owner, 2026-09-27: *these numbers make little sense* — and no
//! help text on the panel): each draws what the sound does, and says what it shows in its hover
//! text, written for the player. Live values come from telemetry atomics and the scope's ring; none
//! returns to DSP.

use egui::{Pos2, Sense, Stroke, Ui, WidgetInfo, WidgetType, pos2, vec2};
use mxm_ui::space::RADIUS;
use mxm_ui::theme::Tokens;

use crate::params::ShSourceKind;
use crate::telemetry::{SCOPE_LEN, SCOPE_RATE_HZ, Telemetry};

pub const FILTER_HEIGHT: f32 = 84.0;
pub const SH_HEIGHT: f32 = 84.0;
/// The narrowest either display is drawn. Both fill the card's width at their height; nothing in
/// them is text, so nothing sets a wider floor.
pub const DISPLAY_MIN_WIDTH: f32 = 120.0;
/// The room between a display's frame and what it draws.
const INSET: f32 = 8.0;

fn display(ui: &mut Ui, tokens: &Tokens, name: &str, height: f32) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, name));
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, RADIUS as f32, tokens.surface_2);
    painter.rect_stroke(
        rect,
        RADIUS as f32,
        Stroke::new(1.0, tokens.border),
        egui::StrokeKind::Inside,
    );
    response
}

/// What the filter display shows, on hover rather than printed (design system §7.6).
pub const FILTER_HOVER: &str = "The filter's shape: the solid curve is where Cutoff is set, the \
     dashed one where the filter is while a note plays.";

/// The filter's response at its set cutoff, and — while a note sounds — at the cutoff the voice is
/// playing now. An analytic four-pole curve: a guide to the shape, not a measurement.
pub fn filter_response(
    ui: &mut Ui,
    tokens: &Tokens,
    base_hz: f32,
    sounding_hz: Option<f32>,
    resonance: f32,
) {
    let response = display(
        ui,
        tokens,
        "Filter response: set and sounding",
        FILTER_HEIGHT,
    )
    .on_hover_text(FILTER_HOVER);
    let plot = response.rect.shrink(INSET);
    let curve = |cutoff: f32| -> Vec<Pos2> {
        (0..96)
            .map(|index| {
                let t = index as f32 / 95.0;
                let hz = 5.0 * (20_000.0f32 / 5.0).powf(t);
                let ratio = hz / cutoff.max(1.0);
                let magnitude = 1.0 / (1.0 + ratio.powi(8)).sqrt();
                let peak =
                    1.0 + 2.5 * resonance.clamp(0.0, 1.0) * (-8.0 * ratio.log2().powi(2)).exp();
                let db = 20.0 * (magnitude * peak).max(0.004).log10();
                pos2(
                    plot.left() + t * plot.width(),
                    plot.bottom() - ((db + 48.0) / 60.0).clamp(0.0, 1.0) * plot.height(),
                )
            })
            .collect()
    };
    let painter = ui.painter_at(response.rect);
    painter.add(egui::Shape::line(
        curve(base_hz),
        Stroke::new(1.5, tokens.accent),
    ));
    if let Some(hz) = sounding_hz {
        let sounding = curve(hz);
        for segment in sounding.windows(2).step_by(2) {
            painter.line_segment(
                [segment[0], segment[1]],
                Stroke::new(1.5, tokens.mod_envelope),
            );
        }
    }
}

/// How many sample-and-hold steps the scope spans: its time base follows Sample time, so a step is
/// always wide enough to read.
const SH_STEPS: f32 = 8.0;
/// The scope's shortest time base, in points: a quarter of a second, so the shortest sample times
/// still show a run of steps rather than a blur.
const SH_WINDOW_MIN: usize = 250;

/// What the sample-and-hold display shows, on hover rather than printed (design system §7.6).
pub const SH_HOVER: &str = "The sample and hold's output as it plays, newest on the right. When it \
     samples the LFO, the LFO is drawn faintly behind it.";

/// How many of the scope's points the display spans at a sample time.
fn window(time_s: f32) -> usize {
    ((SH_STEPS * time_s * SCOPE_RATE_HZ) as usize).clamp(SH_WINDOW_MIN, SCOPE_LEN)
}

/// `values`, the newest last, placed in `window` evenly spaced slots that end at the right edge of
/// a `width` wide plot, and reduced to what a pixel column can show: each column's first, lowest,
/// highest and last point, in the order they came. `(x, value)` pairs, `x` from the plot's left.
///
/// Every point would be up to seventy to a pixel at the longest time base; one in each column
/// would alias a fast LFO into a slow one. The extremes keep an LFO's full swing, and the first and
/// last keep a step's edge upright between two columns.
fn columns(values: &[f32], window: usize, width: f32) -> Vec<(f32, f32)> {
    let window = window.max(values.len()).max(2);
    let offset = window - values.len();
    let x = |index: usize| (offset + index) as f32 / (window - 1) as f32 * width;
    let mut path = Vec::with_capacity(4 * (width.max(0.0) as usize + 1));
    let mut start = 0;
    while start < values.len() {
        let column = x(start).floor();
        let mut end = start + 1;
        while end < values.len() && x(end).floor() == column {
            end += 1;
        }
        let extreme = |better: fn(f32, f32) -> bool| {
            (start..end).fold(start, |best, index| {
                if better(values[index], values[best]) {
                    index
                } else {
                    best
                }
            })
        };
        let mut picks = [start, extreme(|a, b| a < b), extreme(|a, b| a > b), end - 1];
        picks.sort_unstable();
        for (n, &index) in picks.iter().enumerate() {
            if n == 0 || picks[n - 1] != index {
                path.push((x(index), values[index]));
            }
        }
        start = end;
    }
    path
}

/// The sample and hold's output as an oscilloscope: the last [`SH_STEPS`] sample times of it,
/// newest at the right, read from the audio thread's ring — and, when it samples the LFO, the LFO
/// faintly behind it. It moves while the voice runs and holds the last thing played at rest, where
/// the voice's LFO is parked too.
pub fn sample_hold(
    ui: &mut Ui,
    tokens: &Tokens,
    telemetry: &Telemetry,
    source: ShSourceKind,
    time_s: f32,
) {
    let response = display(ui, tokens, "Sample and hold trace", SH_HEIGHT).on_hover_text(SH_HOVER);
    let plot = response.rect.shrink(INSET);
    let painter = ui.painter_at(response.rect);
    // §7.5's neutral zero line, so the trace's swing either side of it can be judged.
    painter.line_segment(
        [
            pos2(plot.left(), plot.center().y),
            pos2(plot.right(), plot.center().y),
        ],
        Stroke::new(1.0, tokens.border),
    );
    let window = window(time_s);
    let (mut sources, mut outs) = (Vec::new(), Vec::new());
    telemetry.scope_recent(window, &mut sources, &mut outs);
    let line = |values: &[f32]| -> Vec<Pos2> {
        columns(values, window, plot.width())
            .into_iter()
            .map(|(x, value)| {
                pos2(
                    plot.left() + x,
                    plot.center().y - value.clamp(-1.0, 1.0) * plot.height() * 0.5,
                )
            })
            .collect()
    };
    // The random source is noise between its samples: a band, not a waveform worth drawing.
    if source != ShSourceKind::Random {
        painter.add(egui::Shape::line(
            line(&sources),
            Stroke::new(1.0, tokens.mod_lfo.gamma_multiply(0.6)),
        ));
    }
    painter.add(egui::Shape::line(
        line(&outs),
        Stroke::new(1.5, tokens.accent),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Rect;

    /// **The time base follows Sample time**: eight steps across, never shorter than a quarter of
    /// a second nor longer than the ring.
    #[test]
    fn the_time_base_follows_sample_time() {
        assert_eq!(window(0.2), 1600);
        assert_eq!(window(0.013), SH_WINDOW_MIN);
        assert_eq!(window(2.0), SCOPE_LEN);
    }

    /// **The newest point is at the right edge**, and a scope that has run for less than its time
    /// base starts part-way across rather than stretching what it has.
    #[test]
    fn the_newest_point_is_at_the_right_edge() {
        let path = columns(&[0.1, 0.2, 0.3], 101, 100.0);
        assert_eq!(path.len(), 3);
        assert!(
            (path[0].0 - 98.0).abs() < 1e-3 && path[0].1 == 0.1,
            "{path:?}"
        );
        assert_eq!(path.last(), Some(&(100.0, 0.3)));
    }

    /// **A step is drawn upright**: the held value and the next meet within a pixel.
    #[test]
    fn a_step_is_drawn_upright() {
        let values: Vec<f32> = (0..1000).map(|k| if k < 500 { 0.0 } else { 1.0 }).collect();
        let path = columns(&values, 1000, 100.0);
        let rise = path
            .windows(2)
            .find(|pair| pair[0].1 == 0.0 && pair[1].1 == 1.0)
            .expect("the step was not drawn");
        assert!(rise[1].0 - rise[0].0 < 1.0, "a slanted step: {rise:?}");
    }

    /// **A fast LFO keeps its full swing**: at the longest time base a 25 Hz saw puts a whole cycle
    /// in each pixel column, and every column still reaches both its top and its bottom rather than
    /// one sampled value aliasing into a slow wave.
    #[test]
    fn a_fast_lfo_keeps_its_swing_in_every_column() {
        let saw: Vec<f32> = (0..SCOPE_LEN)
            .map(|k| 1.0 - 2.0 * (k as f32 * 25.0 / SCOPE_RATE_HZ).rem_euclid(1.0))
            .collect();
        let width = 200.0;
        let path = columns(&saw, SCOPE_LEN, width);
        assert!(
            path.len() <= 4 * (width as usize + 1),
            "{} points",
            path.len()
        );
        let mut by_column = std::collections::BTreeMap::<i64, (f32, f32)>::new();
        for (x, value) in path {
            let span = by_column.entry(x.floor() as i64).or_insert((value, value));
            *span = (span.0.min(value), span.1.max(value));
        }
        // The last column holds the one point at the right edge.
        let full = by_column
            .values()
            .filter(|span| span.0 < -0.9 && span.1 > 0.9);
        assert!(
            full.count() >= by_column.len() - 1,
            "columns lost the saw's swing: {by_column:?}"
        );
    }

    #[test]
    fn telemetry_overlays_leave_following_controls_below_the_painted_display() {
        for tokens in [mxm_ui::theme::LIGHT, mxm_ui::theme::DARK] {
            for width in [248.0, 282.0, 480.0] {
                for zoom in [0.75, 1.0, 1.25, 1.5, 2.0] {
                    for (name, height) in [("filter", FILTER_HEIGHT), ("sample hold", SH_HEIGHT)] {
                        let telemetry = Telemetry::new();
                        for k in 0..600 {
                            telemetry.push_scope((k as f32 * 0.01).sin(), (k / 100) as f32 * 0.2);
                        }
                        let ctx = egui::Context::default();
                        mxm_ui::typography::apply(&ctx);
                        ctx.set_zoom_factor(zoom);
                        for _ in 0..3 {
                            let mut following = Rect::NOTHING;
                            let mut output = ctx.run_ui(
                                egui::RawInput {
                                    screen_rect: Some(Rect::from_min_size(
                                        Pos2::ZERO,
                                        vec2(width, 600.0),
                                    )),
                                    ..Default::default()
                                },
                                |ui| {
                                    ui.set_max_width(width);
                                    match name {
                                        "filter" => filter_response(
                                            ui,
                                            &tokens,
                                            18_000.0,
                                            Some(1234.5),
                                            0.7,
                                        ),
                                        _ => sample_hold(
                                            ui,
                                            &tokens,
                                            &telemetry,
                                            ShSourceKind::Saw,
                                            0.1,
                                        ),
                                    }
                                    ui.add_space(mxm_ui::space::SPACE_3);
                                    following = ui.label("Following control").rect;
                                },
                            );
                            // Read the real painted background, not a predicted card height.
                            let display = output
                                .shapes
                                .iter()
                                .find_map(|shape| match &shape.shape {
                                    egui::Shape::Rect(rect)
                                        if rect.fill == tokens.surface_2
                                            && (rect.rect.height() - height).abs() < 0.5 =>
                                    {
                                        Some(rect.rect)
                                    }
                                    _ => None,
                                })
                                .expect("the display was not painted");
                            output.textures_delta.clear();
                            assert!(
                                following.top() >= display.bottom() + mxm_ui::space::SPACE_3 - 0.5,
                                "{name} at width {width}, zoom {zoom}: following control {following:?} overlaps display {display:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}
