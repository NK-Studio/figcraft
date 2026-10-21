//! The Design tab's Spine section (`docs/spine.md`): the selected skeleton's animation, skin,
//! time, speed, looping and playback. Everything goes through `spine.set`.

use egui::{Id, Ui};
use figcraft_doc::{Document, NodeId, NodeKind};
use serde_json::{Value, json};

use crate::FigApp;
use crate::i18n::tr;
use crate::right::{menu_item, menu_popup, row};
use crate::theme::Tokens;
use crate::widgets::{self, Lead};

/// Shown when the first selected layer is a Spine skeleton; returns whether it drew.
pub fn section(app: &mut FigApp, ui: &mut Ui, doc: &Document, sel: &[NodeId]) -> bool {
    let Some(first) = sel.first().copied() else { return false };
    let Some(NodeKind::Spine(data)) = doc.node(first).map(|n| &n.kind) else { return false };
    let t = Tokens::get(ui.ctx());
    let ids: Vec<NodeId> = sel.iter().copied().filter(|i| doc.node(*i).is_some_and(|n| matches!(n.kind, NodeKind::Spine(_)))).collect();
    widgets::section_header(ui, tr("Spine"), &[], false);
    let Some(asset) = figcraft_render::spine::asset(doc, &data.asset) else {
        row(ui, 28.0, |ui| widgets::label(ui, tr("The skeleton doesn't load"), t.text_secondary));
        return true;
    };
    let info = asset.info();
    let set = |p: Value| {
        let mut q = json!({"ids": ids});
        if let (Some(o), Some(extra)) = (q.as_object_mut(), p.as_object()) {
            o.extend(extra.clone());
        }
        q
    };
    let setup = tr("Setup pose");
    row(ui, 32.0, |ui| {
        widgets::label(ui, tr("Animation"), t.text_secondary);
        let resp = widgets::dropdown(ui, data.animation.as_deref().unwrap_or(setup), 140.0, None);
        menu_popup(&resp).show(|ui| {
            if menu_item(ui, setup, "") {
                app.run("spine.set", set(json!({"animation": null})));
                ui.close();
            }
            for a in &info.animations {
                if menu_item(ui, &a.name, &format!("{:.2}s", a.duration)) {
                    app.run("spine.set", set(json!({"animation": a.name, "time": 0})));
                    ui.close();
                }
            }
        });
    });
    if info.skins.len() > 1 {
        let default = tr("Default skin");
        row(ui, 32.0, |ui| {
            widgets::label(ui, tr("Skin"), t.text_secondary);
            let resp = widgets::dropdown(ui, data.skin.as_deref().unwrap_or(default), 140.0, None);
            menu_popup(&resp).show(|ui| {
                if menu_item(ui, default, "") {
                    app.run("spine.set", set(json!({"skin": null})));
                    ui.close();
                }
                for k in &info.skins {
                    if menu_item(ui, k, "") {
                        app.run("spine.set", set(json!({"skin": k})));
                        ui.close();
                    }
                }
            });
        });
    }
    if data.animation.is_some() {
        let half = (ui.available_width() - widgets::PAD * 2.0 - 8.0) / 2.0;
        let mut time = None;
        let mut speed = None;
        row(ui, 32.0, |ui| {
            time = widgets::number_field(ui, Id::new("spine_time"), Lead::Text(tr("Time")), Some(data.time), "s", half, 0.01);
            ui.add_space(8.0);
            speed = widgets::number_field(ui, Id::new("spine_speed"), Lead::Text(tr("Speed")), Some(data.speed), "×", half, 0.1);
        });
        let q = set(json!({}));
        app.field_run(time, "spine.set", |v| with(&q, "time", json!(v.max(0.0))));
        app.field_run(speed, "spine.set", |v| with(&q, "speed", json!(v)));
        row(ui, 28.0, |ui| {
            if widgets::checkbox(ui, data.looping, tr("Loop")) {
                app.run("spine.set", set(json!({"looping": !data.looping})));
            }
            ui.add_space(12.0);
            if widgets::checkbox(ui, data.playing, tr("Play in presentation")) {
                app.run("spine.set", set(json!({"playing": !data.playing})));
            }
        });
    }
    true
}

fn with(q: &Value, key: &str, v: Value) -> Value {
    let mut q = q.clone();
    q[key] = v;
    q
}
