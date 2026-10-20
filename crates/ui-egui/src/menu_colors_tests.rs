//! Popup menus are dark in both themes: their frame is filled with the menu colour under the white
//! row labels (it used to take egui's `window_fill`, the white panel in the light theme, so the
//! labels vanished), and what they draw reads the dark tokens. Drawn headless.

use egui::{Event, PointerButton, Pos2, RawInput, Rect, Shape, pos2, vec2};

use crate::right::{context_popup, menu_item, menu_popup};
use crate::theme::{self, Tokens};

fn input(t: &mut f64, events: Vec<Event>) -> RawInput {
    *t += 1.0 / 60.0;
    RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))), time: Some(*t), events, ..Default::default() }
}

fn press(p: Pos2, pressed: bool, button: PointerButton) -> Vec<Event> {
    vec![Event::PointerMoved(p), Event::PointerButton { pos: p, button, pressed, modifiers: Default::default() }]
}

/// Open a menu (`context` for a secondary click) from a button in the theme `tokens`; the rects
/// filled in the last frame and the tokens the menu's content saw.
fn open_menu(tokens: Tokens, context: bool) -> (Vec<(Rect, egui::Color32)>, Option<Tokens>) {
    let ctx = egui::Context::default();
    theme::apply(&ctx, tokens);
    let mut t = 0.0;
    let mut seen = None;
    let mut fills = Vec::new();
    let mut frame = |events: Vec<Event>, seen: &mut Option<Tokens>| {
        let mut out = ctx.run_ui(input(&mut t, events), |ui| {
            let (_, resp) = ui.allocate_exact_size(vec2(80.0, 24.0), egui::Sense::click());
            let content = |ui: &mut egui::Ui| {
                *seen = Some(Tokens::get(ui.ctx()));
                menu_item(ui, "Rectangle", "R");
                menu_item(ui, "Ellipse", "O");
            };
            if context {
                context_popup(&resp).show(content);
            } else {
                menu_popup(&resp).show(content);
            }
        });
        out.textures_delta.clear();
        fills.clear();
        for c in &out.shapes {
            rects(&c.shape, &mut fills);
        }
    };
    let button = if context { PointerButton::Secondary } else { PointerButton::Primary };
    let at = pos2(20.0, 12.0);
    frame(vec![], &mut seen);
    frame(press(at, true, button), &mut seen);
    frame(press(at, false, button), &mut seen);
    for _ in 0..30 {
        frame(vec![], &mut seen);
    }
    // Outside the menu the app's own tokens are back.
    assert_eq!(Tokens::get(&ctx).dark, tokens.dark);
    (fills, seen)
}

/// The filled rects in `shape`, nested ones too.
fn rects(shape: &Shape, out: &mut Vec<(Rect, egui::Color32)>) {
    match shape {
        Shape::Rect(r) => out.push((r.rect, r.fill)),
        Shape::Vec(v) => v.iter().for_each(|s| rects(s, out)),
        _ => {}
    }
}

#[test]
fn menus_are_dark_with_readable_rows_in_both_themes() {
    for (name, tokens) in [("light", Tokens::light()), ("dark", Tokens::dark())] {
        for context in [false, true] {
            let (fills, seen) = open_menu(tokens, context);
            let menu = tokens.menu;
            // The menu's frame: a filled rect holding both rows (2 × 26 px).
            let frame = fills.iter().find(|(r, fill)| *fill == menu && r.height() >= 52.0);
            assert!(frame.is_some(), "{name} theme (context menu: {context}): no menu-coloured frame among {fills:?}");
            assert!(
                !fills.iter().any(|(r, fill)| *fill == tokens.panel && tokens.panel != menu && r.height() >= 52.0 && r.min.y > 24.0),
                "{name} theme: a panel-coloured popup frame"
            );
            let seen = seen.unwrap_or_else(|| panic!("{name} theme: the menu's content ran"));
            assert!(seen.dark, "{name} theme: the menu's content reads the dark tokens");
        }
    }
}

#[test]
fn menu_labels_have_contrast_on_the_menu_frame() {
    let fill = theme::menu_visuals().window_fill;
    for tokens in [Tokens::light(), Tokens::dark()] {
        assert_eq!(fill, tokens.menu);
        let ratio = contrast(tokens.menu_text, fill);
        assert!(ratio >= 4.5, "menu labels {ratio:.2}:1 on the menu");
        let dim = contrast(tokens.menu_dim, fill);
        assert!(dim >= 4.5, "dim menu text {dim:.2}:1 on the menu");
    }
}

/// WCAG contrast ratio of two opaque colours.
fn contrast(a: egui::Color32, b: egui::Color32) -> f32 {
    let lum = |c: egui::Color32| {
        let ch = |v: u8| {
            let v = f32::from(v) / 255.0;
            if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * ch(c.r()) + 0.7152 * ch(c.g()) + 0.0722 * ch(c.b())
    };
    let (x, y) = (lum(a), lum(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}
