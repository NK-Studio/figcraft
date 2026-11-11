//! The Design tab's Unity section shows only in files that target Unity (or on a layer that
//! already carries Unity data), and no longer repeats the auto layout position: "Absolute (no
//! auto layout parent)" under the Auto layout section of a frame that has auto layout read as a
//! contradiction, and its Relative/Absolute switch was the Position section's "Ignore auto layout".

use egui::Shape;
use serde_json::json;

use crate::layers_drag_tests::Ui;

/// The text the right panel paints this frame.
fn panel_texts(u: &mut Ui) -> Vec<String> {
    fn walk(shape: &Shape, out: &mut Vec<(String, egui::Rect)>) {
        match shape {
            Shape::Text(t) => out.push((t.galley.text().to_string(), t.visual_bounding_rect())),
            Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            _ => {}
        }
    }
    u.frame(vec![]);
    let frame = u.frame_output(vec![]);
    let mut out = Vec::new();
    for c in &frame.shapes {
        walk(&c.shape, &mut out);
    }
    let panel = u.app.ui.right_panel_rect.expect("the right panel shows");
    out.into_iter().filter(|(_, r)| r.min.x >= panel.min.x).map(|(t, _)| t).collect()
}

fn has(texts: &[String], text: &str) -> bool {
    texts.iter().any(|t| t == text)
}

fn run(u: &mut Ui, command: &str, params: serde_json::Value) -> serde_json::Value {
    u.app.session.execute(command, &params).unwrap_or_else(|e| panic!("{command}: {e}"))
}

/// A top-level auto layout frame with one layer in its flow: `(frame, child)`.
fn auto_layout_frame(u: &mut Ui) -> (u64, u64) {
    let f = run(u, "frame.create", json!({"x": 0, "y": 0, "width": 200, "height": 80, "props": {"layout": "horizontal"}}))["id"].as_u64().unwrap();
    let c = run(u, "shape.rectangle", json!({"x": 0, "y": 0, "width": 40, "height": 40, "parent": f}))["id"].as_u64().unwrap();
    (f, c)
}

#[test]
fn the_section_shows_only_in_files_for_unity() {
    let mut u = Ui::new(figcraft_engine::Session::new());
    let (f, c) = auto_layout_frame(&mut u);

    // A file that isn't for Unity: one checkbox with nothing selected, nothing on layers.
    run(&mut u, "select.set", json!({"ids": []}));
    let page = panel_texts(&mut u);
    assert!(has(&page, "Unity") && has(&page, "This file is for Unity"), "{page:?}");
    assert!(!has(&page, "Target version") && !has(&page, "Export to Unity…"), "{page:?}");
    for id in [f, c] {
        run(&mut u, "select.set", json!({"ids": [id]}));
        let layer = panel_texts(&mut u);
        assert!(!has(&layer, "Unity") && !has(&layer, "Export to Unity…"), "{id}: {layer:?}");
    }

    // Turned on: the file's settings, and the section on layers.
    run(&mut u, "unity.settings.set", json!({"enabled": true}));
    run(&mut u, "select.set", json!({"ids": []}));
    let page = panel_texts(&mut u);
    assert!(has(&page, "Target version") && has(&page, "Export to Unity…"), "{page:?}");
    run(&mut u, "select.set", json!({"ids": [f]}));
    let frame = panel_texts(&mut u);
    assert!(has(&frame, "Unity") && has(&frame, "Unity 6.0") && has(&frame, "Export to Unity…"), "{frame:?}");

    // Turned off again, it goes.
    run(&mut u, "unity.settings.set", json!({"enabled": false}));
    let frame = panel_texts(&mut u);
    assert!(!has(&frame, "Unity") && !has(&frame, "Export to Unity…"), "{frame:?}");
}

#[test]
fn the_section_does_not_repeat_the_auto_layout_position() {
    let mut u = Ui::new(figcraft_engine::Session::new());
    let (f, c) = auto_layout_frame(&mut u);
    run(&mut u, "unity.settings.set", json!({"enabled": true}));
    for id in [f, c] {
        run(&mut u, "select.set", json!({"ids": [id]}));
        let texts = panel_texts(&mut u);
        assert!(has(&texts, "Unity"), "{id}: {texts:?}");
        for gone in ["Absolute (no auto layout parent)", "Panel root (top-level frame)", "Relative", "Absolute"] {
            assert!(!has(&texts, gone), "{id}: `{gone}` in {texts:?}");
        }
    }
    // A layer in the flow has no pivot to pick; one that ignores auto layout does. The switch
    // itself is the Position section's.
    assert!(!has(&panel_texts(&mut u), "Pivot"));
    run(&mut u, "node.set", json!({"ids": [c], "props": {"absolute": true}}));
    assert!(has(&panel_texts(&mut u), "Pivot"));
}

#[test]
fn a_layer_with_unity_data_keeps_its_section_in_any_file() {
    let mut u = Ui::new(figcraft_engine::Session::new());
    let f = run(&mut u, "frame.create", json!({"x": 0, "y": 0, "width": 200, "height": 80}))["id"].as_u64().unwrap();
    let c = run(&mut u, "shape.rectangle", json!({"x": 10, "y": 10, "width": 40, "height": 40, "parent": f}))["id"].as_u64().unwrap();
    run(&mut u, "unity.layer.set", json!({"ids": [c], "pivot": "center"}));
    assert!(u.app.session.doc().unwrap().unity.is_none(), "a pivot doesn't make the file one for Unity");
    run(&mut u, "select.set", json!({"ids": [c]}));
    let texts = panel_texts(&mut u);
    assert!(has(&texts, "Unity") && has(&texts, "Pivot") && has(&texts, "Set on the layer"), "{texts:?}");
}
