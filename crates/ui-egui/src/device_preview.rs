//! The device preview (`device.preview`, `docs/device-profiles.md` › Device preview): a floating
//! pane over the canvas showing the selected top-level frame as it would be on an imported device.
//!
//! The engine works the frame out on a copy of the document by the steps `frame.device.set` takes
//! (`figcraft_engine::cmd::device_sim::simulate`), so the file is untouched; the pane draws that
//! copy with the device's body, safe area and cutouts (the canvas's own drawing, View ▸ Safe areas
//! and Device bezels apply) and outlines the layers the engine warns about. It follows edits and
//! the editor's selection, and is worked out again only when the document or the device changes.

use std::sync::Arc;

use egui::{Align2, Color32, CornerRadius, Id, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use figcraft_doc::{DeviceFrame, Document, NodeId};
use figcraft_engine::Session;
use figcraft_engine::cmd::device_sim::{SimWarning, WarningKind, simulate, snapshot, warnings};
use figcraft_geom::Affine;
use serde_json::{Value, json};

use crate::FigApp;
use crate::a11y::Tip;
use crate::i18n::{tr, trn};
use crate::icons::{self, Icon};
use crate::theme::{Tokens, regular, semibold};
use crate::widgets;

/// The top bar's and the status line's heights.
const BAR_H: f32 = 36.0;
const STATUS_H: f32 = 24.0;
/// Space kept round the device.
const PAD: f32 = 14.0;
/// The most pixels the frame's texture holds (a big screen is drawn smaller and scaled up).
const MAX_TEXTURE_PIXELS: f64 = 8_000_000.0;

/// The device the preview shows, kept while the app runs (closing the pane keeps it).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Choice {
    /// An id of the session's devices (empty until one is imported).
    pub device: String,
    /// The orientation's name (`portrait`…); the screen's first when it doesn't turn that way.
    pub orientation: String,
    /// Which of the device's screens (a foldable has two).
    pub screen: usize,
}

impl Choice {
    /// Keep the choice on something the session has: the first device when this one is gone, its
    /// first screen when the index is past the end, the first orientation the screen supports.
    fn settle(&mut self, s: &Session) {
        if s.devices.get(&self.device).is_none() {
            self.device = s.devices.iter().next().map(|d| d.id.clone()).unwrap_or_default();
            self.screen = 0;
        }
        let Some(d) = s.devices.get(&self.device) else { return };
        if d.screen(self.screen).is_none() {
            self.screen = 0;
        }
        let Some(sc) = d.screen(self.screen) else { return };
        let supported = sc.supported();
        if !supported.iter().any(|o| o.name() == self.orientation) {
            self.orientation = supported.first().map(|o| o.name().to_string()).unwrap_or_default();
        }
    }

    /// The next orientation the screen supports after this one (`None` when it has only one).
    fn turned(&self, s: &Session) -> Option<String> {
        let supported = s.devices.get(&self.device)?.screen(self.screen)?.supported();
        let at = supported.iter().position(|o| o.name() == self.orientation)?;
        if supported.len() < 2 {
            return None;
        }
        supported.get((at + 1) % supported.len()).map(|o| o.name().to_string())
    }
}

/// A frame worked out for a device.
struct Sim {
    /// The editor's document it was worked out from, and for what.
    source: Arc<Document>,
    key: (NodeId, Choice),
    snap: DeviceFrame,
    doc: Document,
    /// The frame's size on the device, in points.
    size: (f64, f64),
    warnings: Vec<SimWarning>,
    /// The frame drawn at a pixel scale (its bits).
    tex: Option<(u64, egui::TextureHandle)>,
}

/// The device preview pane.
pub struct DevicePreview {
    /// The open file it previews (another tab closes it).
    doc_uid: u64,
    /// The top-level frame shown.
    pub frame: NodeId,
    /// The top-level frame last selected in the editor.
    selected: Option<NodeId>,
    sim: Option<Sim>,
}

/// What the pane asked for.
enum Action {
    Close,
    Device(String),
    Turn,
    Screen,
    Import,
    Select(Vec<NodeId>),
}

/// The top-level frame of the editor's first selected layer.
fn selected_frame(ds: &figcraft_engine::DocState) -> Option<NodeId> {
    ds.selection.first().and_then(|i| ds.doc.top_level_frame(*i))
}

/// The pane's window id (its position and size persist under it).
pub fn pane_id() -> Id {
    Id::unique("device_preview_pane")
}

/// `device.preview` asked for the pane: `{frame, device?, orientation?, screen?}`.
pub fn open(app: &mut FigApp, data: &Value) {
    let Some(frame) = data.get("frame").and_then(figcraft_engine::params::id_of) else { return };
    if let Some(device) = data["device"].as_str() {
        app.ui.device_choice = Choice {
            device: device.to_string(),
            orientation: data["orientation"].as_str().unwrap_or_default().to_string(),
            screen: data["screen"].as_u64().and_then(|n| usize::try_from(n).ok()).unwrap_or(0),
        };
    }
    let (doc_uid, selected) = app.session.ds().map(|ds| (ds.uid, selected_frame(ds))).unwrap_or((0, None));
    app.ui.device_preview = Some(DevicePreview { doc_uid, frame, selected, sim: None });
}

/// Devices ▸ Import devices…: a folder of device files, imported again at the next start.
pub fn import_devices(app: &mut FigApp) {
    app.pick_folder(tr("Import devices…"), |app, dir| {
        if let Some(r) = app.run("device.import", json!({"path": dir.to_string_lossy()})) {
            let n = r["imported"].as_array().map_or(0, Vec::len);
            app.toast(trn("Imported {n} devices", n, &[]));
            // The folder is imported again at the next start.
            app.save_prefs();
        }
    });
}

impl DevicePreview {
    /// Work the frame out again when the document, the frame or the device changed.
    fn refresh(&mut self, s: &Session, doc: &Arc<Document>, choice: &Choice) {
        let key = (self.frame, choice.clone());
        if self.sim.as_ref().is_some_and(|m| Arc::ptr_eq(&m.source, doc) && m.key == key) {
            return;
        }
        self.sim = None;
        let Some(snap) = snapshot(s, &choice.device, choice.screen, &choice.orientation) else { return };
        let Ok(mut sim) = simulate(doc, self.frame, &snap) else { return };
        let found = warnings(doc, &sim, self.frame);
        // The screen is upright whatever way the frame is turned on the page.
        let m = sim.world_transform(self.frame).as_coeffs();
        if let Ok(n) = sim.get_mut(self.frame)
            && (m[1].abs() > 1e-9 || m[2].abs() > 1e-9)
            && n.parent.is_some_and(|p| doc.node(p).is_some_and(figcraft_doc::Node::is_page))
        {
            n.transform = Affine::translate((m[4], m[5]));
        }
        let Some(size) = sim.node(self.frame).map(|n| (n.width, n.height)).filter(|(w, h)| w.is_finite() && h.is_finite() && *w > 0.0 && *h > 0.0)
        else {
            return;
        };
        self.sim = Some(Sim { source: doc.clone(), key, snap, doc: sim, size, warnings: found, tex: None });
    }

    /// What `ui.inspect` reports under `devicePreview`.
    pub fn inspect(&self, choice: &Choice) -> Value {
        json!({
            "frame": self.frame,
            "device": choice.device,
            "orientation": choice.orientation,
            "screen": choice.screen,
            "size": self.sim.as_ref().map(|m| [m.size.0, m.size.1]),
            "safeArea": self.sim.as_ref().map(|m| {
                let i = m.snap.insets_for(m.size.0, m.size.1);
                json!({"top": i.top, "right": i.right, "bottom": i.bottom, "left": i.left})
            }),
            "warnings": self.sim.as_ref().map(|m| m.warnings.iter().map(SimWarning::to_json).collect::<Vec<_>>()),
        })
    }
}

/// The frame's texture at `px` pixels per point (drawn again when the scale changes).
fn texture(ctx: &egui::Context, sim: &mut Sim, frame: NodeId, px: f64) -> Option<egui::TextureHandle> {
    let (w, h) = sim.size;
    let side = ctx.input(|i| i.max_texture_side) as f64 - 2.0;
    let px = px.min(side / w.max(h)).min((MAX_TEXTURE_PIXELS / (w * h)).sqrt());
    if !(px.is_finite() && px > 0.0) {
        return None;
    }
    if let Some((k, tex)) = &sim.tex
        && *k == px.to_bits()
    {
        return Some(tex.clone());
    }
    let area = sim.doc.world_bounds(frame)?;
    let pix = figcraft_render::render_area(&sim.doc, &[frame], area, px)?;
    let img = crate::render_worker::screen_image(&pix, sim.doc.color_profile);
    let tex = ctx.load_texture("device-preview", img, egui::TextureOptions::LINEAR);
    sim.tex = Some((px.to_bits(), tex.clone()));
    Some(tex)
}

/// Show the pane while `app.ui.device_preview` is set (call once per frame).
pub fn show(app: &mut FigApp, ctx: &egui::Context) {
    let Some(mut pv) = app.ui.device_preview.take() else { return };
    let Ok(ds) = app.session.ds() else { return };
    if ds.uid != pv.doc_uid {
        return;
    }
    let (doc, selected, view) = (ds.doc.clone(), selected_frame(ds), (ds.view.safe_areas, ds.view.device_bezels));
    // Selecting another top-level frame in the editor shows that frame.
    if selected.is_some() && selected != pv.selected {
        pv.selected = selected;
        pv.frame = selected.unwrap_or(pv.frame);
    }
    // The frame was deleted: nothing to show.
    if !doc.node(pv.frame).is_some_and(|n| n.kind.is_frame_like()) {
        return;
    }
    let mut choice = std::mem::take(&mut app.ui.device_choice);
    choice.settle(&app.session);
    pv.refresh(&app.session, &doc, &choice);
    let t = Tokens::get(ctx);
    let canvas = app.canvas.rect;
    let style = ctx.global_style();
    let mut action = None;
    egui::Window::new(tr("Device preview"))
        .id(pane_id())
        .title_bar(false)
        .collapsible(false)
        .resizable(true)
        .default_size(vec2(340.0, 640.0))
        .min_size(vec2(240.0, 280.0))
        .default_pos(pos2(canvas.min.x + 16.0, canvas.min.y + 16.0))
        .constrain_to(ctx.content_rect())
        .frame(egui::Frame::window(&style).fill(t.panel).inner_margin(egui::Margin::same(0)))
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
            action = pane(&app.session, &mut pv, &choice, ui, rect, view, style.visuals.window_corner_radius.nw);
        });
    match action {
        Some(Action::Close) => {
            app.ui.device_choice = choice;
            return;
        }
        Some(Action::Device(id)) => choice = Choice { device: id, orientation: choice.orientation, screen: 0 },
        Some(Action::Turn) => {
            if let Some(o) = choice.turned(&app.session) {
                choice.orientation = o;
            }
        }
        Some(Action::Screen) => {
            let n = app.session.devices.get(&choice.device).map_or(1, |d| d.screens.len().max(1));
            choice.screen = (choice.screen + 1) % n;
        }
        Some(Action::Import) => import_devices(app),
        Some(Action::Select(ids)) => {
            app.run("select.set", json!({"ids": ids}));
        }
        None => {}
    }
    app.ui.device_choice = choice;
    app.ui.device_preview = Some(pv);
}

/// A text button of the bar at `r`: hover highlight, `label` left-aligned and clipped.
fn bar_button(ui: &mut Ui, id: &str, r: Rect, label: &str, tip: &str, color: Color32) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let resp = ui.interact(r, ui.scope_id().with(("device_preview", id)), Sense::click()).tip(tip);
    if resp.hovered() {
        ui.painter().rect_filled(r, CornerRadius::same(5), t.hover);
    }
    ui.painter().with_clip_rect(r).text(pos2(r.min.x + 8.0, r.center().y), Align2::LEFT_CENTER, label, regular(11.5), color);
    resp
}

/// The pane in `rect`: the bar (device, screen, turn, close), the device, the status line.
fn pane(
    s: &Session,
    pv: &mut DevicePreview,
    choice: &Choice,
    ui: &mut Ui,
    rect: Rect,
    (safe_areas, bezels): (bool, bool),
    round: u8,
) -> Option<Action> {
    let t = Tokens::get(ui.ctx());
    let mut action = None;
    let bar = Rect::from_min_size(rect.min, vec2(rect.width(), BAR_H));
    let status = Rect::from_min_max(pos2(rect.min.x, rect.max.y - STATUS_H), rect.max);
    let body = Rect::from_min_max(bar.left_bottom(), status.right_top());
    ui.painter().rect_filled(bar, CornerRadius { nw: round, ne: round, sw: 0, se: 0 }, t.panel);
    ui.painter().rect_filled(body, CornerRadius::ZERO, t.canvas);
    ui.painter().line_segment([bar.left_bottom(), bar.right_bottom()], Stroke::new(1.0, t.border));
    ui.painter().line_segment([status.left_top(), status.right_top()], Stroke::new(1.0, t.border));
    let profile = s.devices.get(&choice.device);
    // Right: close, turn, the screen of a device with several.
    let mut x = bar.max.x - 6.0;
    let mut slot = |w: f32| {
        let r = Rect::from_min_size(pos2(x - w, bar.center().y - 12.0), vec2(w, 24.0));
        x = r.min.x - 2.0;
        r
    };
    if widgets::icon_button_at(ui, ui.scope_id().with("device_preview_close"), slot(24.0), Icon::Close, tr("Close device preview")) {
        action = Some(Action::Close);
    }
    if choice.turned(s).is_some()
        && widgets::icon_button_at(ui, ui.scope_id().with("device_preview_turn"), slot(24.0), Icon::Rotate, tr("Rotate device"))
    {
        action = Some(Action::Turn);
    }
    let screens = profile.map_or(0, |d| d.screens.len());
    if screens > 1 {
        let label = format!("{}/{}", choice.screen + 1, screens);
        let w = ui.painter().layout_no_wrap(label.clone(), regular(11.5), t.text).size().x + 16.0;
        if bar_button(ui, "screen", slot(w), &label, tr("Next screen"), t.text).clicked() {
            action = Some(Action::Screen);
        }
    }
    // Left: the device, with the devices to pick from.
    let name = profile.map_or(tr("No device"), |d| d.name.as_str()).to_owned();
    let wanted = ui.painter().layout_no_wrap(name.clone(), semibold(11.5), t.text).size().x + 34.0;
    let dr = Rect::from_min_max(pos2(bar.min.x + 6.0, bar.center().y - 12.0), pos2((bar.min.x + 6.0 + wanted).min(x - 4.0), bar.center().y + 12.0));
    let resp = ui.interact(dr, ui.scope_id().with("device_preview_device"), Sense::click()).tip(tr("Devices"));
    if resp.hovered() {
        ui.painter().rect_filled(dr, CornerRadius::same(5), t.hover);
    }
    let text_clip = Rect::from_min_max(dr.min, pos2(dr.max.x - 20.0, dr.max.y));
    ui.painter().with_clip_rect(text_clip).text(pos2(dr.min.x + 8.0, dr.center().y), Align2::LEFT_CENTER, &name, semibold(11.5), t.text);
    icons::paint(ui.painter(), Rect::from_center_size(pos2(dr.right() - 11.0, dr.center().y), vec2(17.0, 17.0)), Icon::Chevron, t.icon_secondary);
    let menu = crate::right::menu_popup(&resp).width(260.0).show(|ui| {
        let mut picked = None;
        egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
            for d in s.devices.iter() {
                let size = d
                    .screen(0)
                    .and_then(|sc| Some(sc.size_pt(*sc.supported().first()?)))
                    .map_or(String::new(), |(w, h)| format!("{}×{}", w.round(), h.round()));
                if crate::right::menu_check(ui, d.id == choice.device, &d.name, &size) {
                    picked = Some(Action::Device(d.id.clone()));
                    ui.close();
                }
            }
            if !s.devices.is_empty() {
                ui.separator();
            }
            if crate::right::menu_item_beside_checks(ui, tr("Import devices…"), "") {
                picked = Some(Action::Import);
                ui.close();
            }
        });
        picked
    });
    action = action.or(menu.and_then(|m| m.inner));
    // The device.
    let painter = ui.painter().with_clip_rect(body);
    let frame = pv.frame;
    let Some(sim) = pv.sim.as_mut() else {
        let note = tr("This frame can't be shown on the device");
        painter.text(body.center(), Align2::CENTER_CENTER, note, regular(11.5), t.text_secondary);
        return action;
    };
    let (w, h) = sim.size;
    let bezel = if bezels { crate::device_bezel::bezel_points(&sim.snap, w, h).0 } else { 0.0 };
    let avail = body.shrink(PAD);
    let z = (avail.width() / (w as f32 + 2.0 * bezel)).min(avail.height() / (h as f32 + 2.0 * bezel)).min(1.0);
    if z.is_finite() && z > 0.0 {
        let screen = Rect::from_center_size(avail.center(), vec2(w as f32 * z, h as f32 * z));
        if let Some(tex) = texture(ui.ctx(), sim, frame, f64::from(z * ui.ctx().pixels_per_point())) {
            painter.image(tex.id(), screen, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }
        let at = |x: f64, y: f64| pos2(screen.min.x + x as f32 * z, screen.min.y + y as f32 * z);
        if safe_areas {
            crate::safe_area::draw_with(&painter, &t, &sim.snap, w, h, at);
        }
        // The layers that need a look: outlined, named on hover, selected by a click.
        for wn in &sim.warnings {
            let r =
                Rect::from_min_max(at(wn.rect.x, wn.rect.y), at(wn.rect.x + wn.rect.width, wn.rect.y + wn.rect.height)).intersect(screen.expand(2.0));
            if !r.is_positive() {
                continue;
            }
            painter.rect_stroke(r, CornerRadius::same(2), Stroke::new(1.5, t.danger), StrokeKind::Outside);
            let why = match wn.kind {
                WarningKind::SafeArea => tr("crosses the safe area"),
                WarningKind::Offscreen => tr("leaves the screen"),
            };
            let hit = ui
                .interact(r.intersect(body), ui.scope_id().with(("device_preview_warning", wn.id)), Sense::click())
                .tip(format!("{}: {why}", wn.name));
            if hit.clicked() {
                action = Some(Action::Select(vec![wn.id]));
            }
        }
        if bezels {
            crate::device_bezel::draw_at(&painter, &sim.snap, w, h, screen);
        }
    }
    // The status line: the screen in points, and how many layers need a look.
    let size = format!("{} × {} pt", (w * 10.0).round() / 10.0, (h * 10.0).round() / 10.0);
    ui.painter().text(pos2(status.min.x + 10.0, status.center().y), Align2::LEFT_CENTER, size, regular(11.0), t.text_secondary);
    if !sim.warnings.is_empty() {
        let label = trn("{n} layers to check", sim.warnings.len(), &[]);
        let lw = ui.painter().layout_no_wrap(label.clone(), regular(11.0), t.danger).size().x + 16.0;
        // Clear of the window's resize grip in the corner.
        let r = Rect::from_min_max(pos2(status.max.x - lw - 18.0, status.min.y + 2.0), pos2(status.max.x - 18.0, status.max.y - 2.0));
        if bar_button(ui, "warnings", r, &label, tr("Select the layers that cross the safe area or leave the screen"), t.danger).clicked() {
            action = Some(Action::Select(sim.warnings.iter().map(|w| w.id).collect()));
        }
    }
    action
}
