//! The Design tab's Unity UI Toolkit section (`docs/unity-uitk.md`): with nothing selected, the
//! file's target Unity version and grid setting; with layers selected, how the first one is placed
//! (position, the 9-point pivot) and what the target version can't represent in the selection.

use egui::{Align2, CornerRadius, Id, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use figcraft_doc::{Document, NodeId};
use figcraft_uitk::{Issue, Position, Severity, UnityVersion};
use serde_json::json;

use crate::FigApp;
use crate::i18n::{tr, trf};
use crate::right::{menu_item, menu_popup, row};
use crate::theme::{Tokens, regular, semibold};
use crate::widgets::{self, PAD};

/// Issues listed under the section at most (the counts cover all of them).
const MAX_LISTED: usize = 12;
/// The pivot picker's cell size and gap.
const CELL: f32 = 14.0;
const GAP: f32 = 4.0;

/// With nothing selected: the file's Unity version and grid setting, and the page's counts.
pub fn file_section(app: &mut FigApp, ui: &mut Ui, doc: &Document) {
    let t = Tokens::get(ui.ctx());
    let opts = figcraft_uitk::file_options(doc);
    widgets::section_header(ui, tr("Unity UI Toolkit"), &[], false);
    row(ui, 32.0, |ui| {
        widgets::label(ui, tr("Target version"), t.text_secondary);
        let resp = widgets::dropdown(ui, &format!("Unity {}", opts.version), 110.0, None);
        menu_popup(&resp).show(|ui| {
            for v in UnityVersion::known() {
                if menu_item(ui, &format!("Unity {v}"), "") {
                    app.run("unity.settings.set", json!({"version": v.to_string()}));
                    ui.close();
                }
            }
        });
    });
    if opts.version.has(figcraft_uitk::Feature::Grid) {
        row(ui, 28.0, |ui| {
            if widgets::checkbox(ui, opts.grid_enabled, tr("Grid layout (experimental)")) {
                app.run("unity.settings.set", json!({"grid": !opts.grid_enabled}));
            }
        });
    }
    let fixed = figcraft_uitk::file_export_options(doc).fixed_text_boxes;
    row(ui, 28.0, |ui| {
        if widgets::checkbox(ui, fixed, tr("Keep FigCraft's text box sizes")) {
            app.run("unity.settings.set", json!({"fixedTextBoxes": !fixed}));
        }
    });
    let Ok(page) = app.session.page() else { return };
    issues(app, ui, doc, &[page]);
    export_button(app, ui, None);
}

/// Export to a folder: `ids` (the selection's top-level frames), or every top-level frame.
fn export_button(app: &mut FigApp, ui: &mut Ui, ids: Option<&[NodeId]>) {
    let mut clicked = false;
    row(ui, 36.0, |ui| clicked = widgets::button(ui, tr("Export to Unity…"), false, true).clicked());
    if !clicked {
        return;
    }
    let ids = ids.map(<[NodeId]>::to_vec);
    app.pick_folder(tr("Export to Unity…"), move |app, dir| {
        let mut p = json!({"dir": dir.to_string_lossy()});
        if let Some(ids) = &ids {
            p["ids"] = json!(ids);
        }
        if let Some(r) = app.run("file.exportUitk", p) {
            let n = r["documents"].as_array().map_or(0, Vec::len);
            app.toast(trf("Exported {n} frames for Unity", &[("n", &n)]));
        }
    });
}

/// With layers selected: position, pivot and the selection's issues.
pub fn layer_section(app: &mut FigApp, ui: &mut Ui, doc: &Document, sel: &[NodeId]) {
    let t = Tokens::get(ui.ctx());
    let Some(first) = sel.first().copied() else { return };
    let Some(info) = figcraft_uitk::layer_info(doc, first) else { return };
    let opts = figcraft_uitk::file_options(doc);
    widgets::section_header(ui, tr("Unity UI Toolkit"), &[], false);
    // The header's right end names the target version.
    let hr = ui.min_rect();
    ui.painter().text(
        pos2(hr.max.x - PAD, hr.max.y - 20.0),
        Align2::RIGHT_CENTER,
        format!("Unity {}", opts.version),
        regular(11.0),
        t.text_secondary,
    );
    row(ui, 32.0, |ui| {
        widgets::label(ui, tr("Position"), t.text_secondary);
        match info.position {
            Position::Root => widgets::label(ui, tr("Panel root (top-level frame)"), t.text),
            _ if !info.in_auto_layout => widgets::label(ui, tr("Absolute (no auto layout parent)"), t.text),
            pos => {
                let labels = [tr("Relative"), tr("Absolute")];
                let active = usize::from(pos == Position::Absolute);
                if let Some(i) = widgets::segmented_text(ui, &labels, active)
                    && i != active
                {
                    app.run("unity.layer.set", json!({"ids": sel, "position": if i == 1 { "absolute" } else { "relative" }}));
                }
            }
        }
    });
    if info.position == Position::Absolute {
        pivot_row(app, ui, sel, info.pivot, info.pivot_set);
    }
    if info.position == Position::Root && doc.node(first).is_some_and(|n| n.kind.is_frame_like()) {
        canvas_scaler_rows(app, ui, doc, first);
    }
    issues(app, ui, doc, sel);
    export_button(app, ui, Some(sel));
}

/// A top-level frame's Unity UI scaling (uGUI CanvasScaler, UI Toolkit PanelSettings): none, a
/// scale mode or the short-side preset, then that mode's settings.
fn canvas_scaler_rows(app: &mut FigApp, ui: &mut Ui, doc: &Document, id: NodeId) {
    use figcraft_doc::{ScreenMatchMode, UiScaleMode};
    let t = Tokens::get(ui.ctx());
    let c = doc.node(id).and_then(|n| n.canvas_scaler);
    let set = |app: &mut FigApp, props: serde_json::Value| {
        let mut p = json!({"ids": [id]});
        if let (Some(o), Some(extra)) = (p.as_object_mut(), props.as_object()) {
            o.extend(extra.clone());
        }
        app.run("unity.canvasScaler.set", p);
    };
    let label = match c {
        None => tr("None (exports short side)").to_string(),
        Some(c) => match (c.short_side_of(), c.scale_mode) {
            (Some(side), _) => format!("{} {}", tr("Short side"), (side * 1000.0).round() / 1000.0),
            (None, UiScaleMode::ConstantPixelSize) => tr("Constant pixel size").to_string(),
            (None, UiScaleMode::ScaleWithScreenSize) => tr("Scale with screen size").to_string(),
            (None, UiScaleMode::ConstantPhysicalSize) => tr("Constant physical size").to_string(),
        },
    };
    row(ui, 32.0, |ui| {
        widgets::label(ui, tr("UI scaling"), t.text_secondary);
        let resp = widgets::dropdown(ui, &label, 150.0, None);
        menu_popup(&resp).show(|ui| {
            if menu_item(ui, tr("None"), "") {
                set(app, json!({"clear": true}));
                ui.close();
            }
            if menu_item(ui, tr("Short side (rotation-safe)"), "") {
                set(app, json!({"preset": "shortSide"}));
                ui.close();
            }
            for (name, mode) in [
                (tr("Constant pixel size"), "constantPixelSize"),
                (tr("Scale with screen size"), "scaleWithScreenSize"),
                (tr("Constant physical size"), "constantPhysicalSize"),
            ] {
                if menu_item(ui, name, "") {
                    set(app, json!({"scaleMode": mode}));
                    ui.close();
                }
            }
        });
    });
    let Some(c) = c else { return };
    let num = |v: f64| format!("{}", (v * 1000.0).round() / 1000.0);
    let field = |ui: &mut Ui, key: &str, v: f64, w: f32, hint: &str| {
        widgets::text_field(ui, Id::new(("cs", key, id)), &num(v), w, hint).and_then(|v| v.trim().parse::<f64>().ok())
    };
    match c.scale_mode {
        UiScaleMode::ScaleWithScreenSize => {
            let label = match c.screen_match_mode {
                ScreenMatchMode::MatchWidthOrHeight => tr("Match width or height"),
                ScreenMatchMode::Expand => "Expand",
                ScreenMatchMode::Shrink => "Shrink",
            };
            row(ui, 32.0, |ui| {
                widgets::label(ui, tr("Screen match"), t.text_secondary);
                let resp = widgets::dropdown(ui, label, 150.0, None);
                menu_popup(&resp).show(|ui| {
                    for (name, mode) in [(tr("Match width or height"), "matchWidthOrHeight"), ("Expand", "expand"), ("Shrink", "shrink")] {
                        if menu_item(ui, name, "") {
                            set(app, json!({"screenMatchMode": mode}));
                            ui.close();
                        }
                    }
                });
            });
            row(ui, 32.0, |ui| {
                widgets::label(ui, tr("Reference"), t.text_secondary);
                if let Some(v) = field(ui, "w", c.reference_width, 64.0, "W") {
                    set(app, json!({"referenceResolution": [v, c.reference_height]}));
                }
                if let Some(v) = field(ui, "h", c.reference_height, 64.0, "H") {
                    set(app, json!({"referenceResolution": [c.reference_width, v]}));
                }
            });
            if c.screen_match_mode == ScreenMatchMode::MatchWidthOrHeight {
                row(ui, 32.0, |ui| {
                    widgets::label(ui, tr("Match (0 width – 1 height)"), t.text_secondary);
                    if let Some(v) = field(ui, "m", c.match_width_or_height, 56.0, "") {
                        set(app, json!({"match": v.clamp(0.0, 1.0)}));
                    }
                });
            }
        }
        UiScaleMode::ConstantPhysicalSize => {
            row(ui, 32.0, |ui| {
                widgets::label(ui, tr("Reference DPI"), t.text_secondary);
                if let Some(v) = field(ui, "rdpi", c.reference_dpi, 56.0, "") {
                    set(app, json!({"referenceDpi": v.clamp(1.0, 2000.0)}));
                }
            });
            row(ui, 32.0, |ui| {
                widgets::label(ui, tr("Fallback DPI"), t.text_secondary);
                if let Some(v) = field(ui, "fdpi", c.fallback_dpi, 56.0, "") {
                    set(app, json!({"fallbackDpi": v.clamp(1.0, 2000.0)}));
                }
            });
        }
        UiScaleMode::ConstantPixelSize => {}
    }
    row(ui, 32.0, |ui| {
        widgets::label(ui, tr("Scale factor"), t.text_secondary);
        if let Some(v) = field(ui, "k", c.scale_factor, 56.0, "") {
            set(app, json!({"scaleFactor": v.clamp(0.01, 100.0)}));
        }
    });
}

/// The 9-point pivot picker, the pivot as numbers, and Reset when the layer sets its own.
fn pivot_row(app: &mut FigApp, ui: &mut Ui, sel: &[NodeId], pivot: figcraft_doc::Pivot, set: bool) {
    let t = Tokens::get(ui.ctx());
    let side = CELL * 3.0 + GAP * 2.0;
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(vec2(w, side + 16.0), Sense::hover());
    ui.painter().text(pos2(r.min.x + PAD, r.min.y + 18.0), Align2::LEFT_CENTER, tr("Pivot"), regular(11.0), t.text_secondary);
    let grid = Rect::from_min_size(pos2(r.min.x + PAD + 64.0, r.min.y + 8.0), vec2(side, side));
    ui.painter().rect_stroke(grid.expand(3.0), CornerRadius::same(4), Stroke::new(1.0, t.border), StrokeKind::Inside);
    for gy in 0..3 {
        for gx in 0..3 {
            let (px, py) = (gx as f64 / 2.0, gy as f64 / 2.0);
            let cell = Rect::from_min_size(pos2(grid.min.x + gx as f32 * (CELL + GAP), grid.min.y + gy as f32 * (CELL + GAP)), vec2(CELL, CELL));
            let resp = ui.interact(cell, Id::new(("unity_pivot", gx, gy)), Sense::click());
            let on = (pivot.x - px).abs() < 1e-6 && (pivot.y - py).abs() < 1e-6;
            let fill = if on {
                t.accent
            } else if resp.hovered() {
                t.hover
            } else {
                t.field
            };
            ui.painter().rect_filled(cell, CornerRadius::same(3), fill);
            if resp.clicked() {
                app.run("unity.layer.set", json!({"ids": sel, "pivot": [px, py]}));
            }
        }
    }
    // A pivot between the nine points shows as a dot where it is.
    let on_grid = [pivot.x, pivot.y].iter().all(|v| [0.0, 0.5, 1.0].iter().any(|g| (v - g).abs() < 1e-6));
    if !on_grid {
        let at = pos2(grid.min.x + CELL / 2.0 + pivot.x as f32 * (side - CELL), grid.min.y + CELL / 2.0 + pivot.y as f32 * (side - CELL));
        ui.painter().circle_filled(at, 3.5, t.accent);
    }
    let text_x = grid.max.x + 16.0;
    ui.painter().text(pos2(text_x, grid.min.y + 8.0), Align2::LEFT_CENTER, format!("{:.2}, {:.2}", pivot.x, pivot.y), regular(11.0), t.text);
    let note = if set { tr("Set on the layer") } else { tr("From constraints") };
    ui.painter().text(pos2(text_x, grid.min.y + 26.0), Align2::LEFT_CENTER, note, regular(11.0), t.text_secondary);
    if set {
        let reset = Rect::from_min_size(pos2(text_x - 4.0, grid.min.y + 36.0), vec2(60.0, 18.0));
        let resp = ui.interact(reset, Id::new("unity_pivot_reset"), Sense::click());
        let col = if resp.hovered() { t.accent_hover } else { t.accent_text };
        ui.painter().text(pos2(text_x, reset.center().y), Align2::LEFT_CENTER, tr("Reset"), semibold(11.0), col);
        if resp.clicked() {
            app.run("unity.layer.set", json!({"ids": sel, "pivot": null}));
        }
    }
}

/// The check's counts for `roots` and the first issues; a click on one selects its layer. The
/// result is kept until the document, the roots or the settings change.
fn issues(app: &mut FigApp, ui: &mut Ui, doc: &Document, roots: &[NodeId]) {
    let t = Tokens::get(ui.ctx());
    let opts = figcraft_uitk::file_options(doc);
    let revision = app.session.ds().map(|d| d.revision).unwrap_or(0);
    let key = Id::new(("unity_check", revision, roots.to_vec(), opts.version.minor, opts.grid_enabled));
    let found: Option<Vec<Issue>> = ui.data(|d| d.get_temp(key));
    let found = found.unwrap_or_else(|| {
        let v = figcraft_uitk::check(doc, roots, opts);
        ui.data_mut(|d| d.insert_temp(key, v.clone()));
        v
    });
    let count = |s: Severity| found.iter().filter(|i| i.severity == s).count();
    let (e, w, i) = (count(Severity::Error), count(Severity::Warning), count(Severity::Info));
    row(ui, 28.0, |ui| {
        if found.is_empty() {
            widgets::label(ui, tr("Nothing to change for this Unity version"), t.success);
        } else {
            let text = trf("{errors} errors · {warnings} warnings · {infos} converted", &[("errors", &e), ("warnings", &w), ("infos", &i)]);
            widgets::label(ui, &text, if e > 0 { t.danger } else { t.text });
        }
    });
    for issue in found.iter().take(MAX_LISTED) {
        let colour = match issue.severity {
            Severity::Error => t.danger,
            Severity::Warning => t.warning,
            Severity::Info => t.text_secondary,
        };
        let w = ui.available_width();
        let text = format!("{} — {}", issue.name, issue.message);
        let galley = ui.painter().layout(text, regular(11.0), t.text, w - PAD * 2.0 - 14.0);
        let h = galley.size().y + 8.0;
        let (r, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(r.shrink2(vec2(8.0, 0.0)), CornerRadius::same(4), t.hover);
        }
        ui.painter().circle_filled(pos2(r.min.x + PAD + 3.0, r.min.y + 4.0 + 7.0), 3.0, colour);
        ui.painter().galley(pos2(r.min.x + PAD + 14.0, r.min.y + 4.0), galley, t.text);
        let resp = resp.on_hover_text(issue.rule);
        if resp.clicked() {
            app.run("select.set", json!({"ids": [issue.id]}));
        }
    }
    if found.len() > MAX_LISTED {
        row(ui, 24.0, |ui| {
            widgets::label(ui, &trf("and {n} more (unity.check lists them all)", &[("n", &(found.len() - MAX_LISTED))]), t.text_secondary)
        });
    }
}
