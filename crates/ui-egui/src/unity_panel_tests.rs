//! "Export to UI Toolkit…" is in the Export section, with the other exports, in files that target
//! Unity. The Design tab's Unity section shows only in files that target Unity (or on a layer that
//! already carries Unity data), and no longer repeats the auto layout position: "Absolute (no
//! auto layout parent)" under the Auto layout section of a frame that has auto layout read as a
//! contradiction, and its Relative/Absolute switch was the Position section's "Ignore auto layout".

use egui::Shape;
use serde_json::json;

use crate::layers_drag_tests::Ui;

/// The text the right panel paints this frame, with each piece's top edge.
fn panel_texts_at(u: &mut Ui) -> Vec<(String, f32)> {
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
    out.into_iter().filter(|(_, r)| r.min.x >= panel.min.x).map(|(t, r)| (t, r.min.y)).collect()
}

/// The text the right panel paints this frame.
fn panel_texts(u: &mut Ui) -> Vec<String> {
    panel_texts_at(u).into_iter().map(|(t, _)| t).collect()
}

/// Where a text is painted in the right panel (the last one when it repeats).
fn y_of(texts: &[(String, f32)], text: &str) -> f32 {
    texts.iter().rev().find(|(t, _)| t == text).map(|(_, y)| *y).unwrap_or_else(|| panic!("`{text}` isn't in {texts:?}"))
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
    assert!(!has(&page, "Target version") && !has(&page, "Export to UI Toolkit…"), "{page:?}");
    for id in [f, c] {
        run(&mut u, "select.set", json!({"ids": [id]}));
        let layer = panel_texts(&mut u);
        assert!(!has(&layer, "Unity") && !has(&layer, "Export to UI Toolkit…"), "{id}: {layer:?}");
    }

    // Turned on: the file's settings, and the section on layers.
    run(&mut u, "unity.settings.set", json!({"enabled": true}));
    run(&mut u, "select.set", json!({"ids": []}));
    let page = panel_texts(&mut u);
    assert!(has(&page, "Target version") && has(&page, "Export to UI Toolkit…"), "{page:?}");
    run(&mut u, "select.set", json!({"ids": [f]}));
    let frame = panel_texts(&mut u);
    assert!(has(&frame, "Unity") && has(&frame, "Unity 6.0") && has(&frame, "Export to UI Toolkit…"), "{frame:?}");

    // Turned off again, it goes.
    run(&mut u, "unity.settings.set", json!({"enabled": false}));
    let frame = panel_texts(&mut u);
    assert!(!has(&frame, "Unity") && !has(&frame, "Export to UI Toolkit…"), "{frame:?}");
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

#[test]
fn export_to_ui_toolkit_is_in_the_export_section() {
    let mut u = Ui::new(figcraft_engine::Session::new());
    let (f, c) = auto_layout_frame(&mut u);
    let loose = run(&mut u, "shape.rectangle", json!({"x": 400, "y": 0, "width": 40, "height": 40}))["id"].as_u64().unwrap();
    run(&mut u, "unity.settings.set", json!({"enabled": true}));

    // On a top-level frame and on a layer inside one: under the Export header, below the Unity
    // section, and once (the Unity section has no button of its own).
    for id in [f, c] {
        run(&mut u, "select.set", json!({"ids": [id]}));
        let texts = panel_texts_at(&mut u);
        let button = y_of(&texts, "Export to UI Toolkit…");
        assert!(button > y_of(&texts, "Export") && y_of(&texts, "Export") > y_of(&texts, "Unity"), "{id}: {texts:?}");
        assert_eq!(texts.iter().filter(|(t, _)| t == "Export to UI Toolkit…").count(), 1, "{id}: {texts:?}");
    }
    // A layer that is in no top-level frame has nothing to export this way.
    run(&mut u, "select.set", json!({"ids": [loose]}));
    assert!(!has(&panel_texts(&mut u), "Export to UI Toolkit…"));
    // With nothing selected it exports the page's frames, from the page's Export section.
    run(&mut u, "select.set", json!({"ids": []}));
    let texts = panel_texts_at(&mut u);
    assert!(y_of(&texts, "Export to UI Toolkit…") > y_of(&texts, "Export page as PNG"), "{texts:?}");
    // A file that isn't for Unity has no such button anywhere.
    run(&mut u, "unity.settings.set", json!({"enabled": false}));
    assert!(!has(&panel_texts(&mut u), "Export to UI Toolkit…"));
    run(&mut u, "select.set", json!({"ids": [f]}));
    assert!(!has(&panel_texts(&mut u), "Export to UI Toolkit…"));
}

#[test]
fn the_export_section_sums_up_the_check_when_it_finds_something() {
    let mut u = Ui::new(figcraft_engine::Session::new());
    let (f, c) = auto_layout_frame(&mut u);
    run(&mut u, "unity.settings.set", json!({"enabled": true}));
    run(&mut u, "select.set", json!({"ids": [f]}));
    // A gap converts to margins before 6.7: no error or warning, so no summary.
    assert!(!panel_texts(&mut u).iter().any(|t| t.contains("errors · ") && !t.contains("converted")));
    // A gradient is baked before 6.7: one warning, summed up by the button.
    run(&mut u, "node.set", json!({"ids": [c], "props": {"fills": [{"type": "linearGradient", "stops": ["#FF0000", "#0000FF"]}]}}));
    let texts = panel_texts_at(&mut u);
    assert!(y_of(&texts, "0 errors · 1 warnings") < y_of(&texts, "Export to UI Toolkit…"), "{texts:?}");
    assert!(y_of(&texts, "0 errors · 1 warnings") > y_of(&texts, "Export"), "{texts:?}");
}
