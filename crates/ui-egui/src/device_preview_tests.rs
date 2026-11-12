//! The device preview pane driven headless through egui: it opens from `device.preview`, shows the
//! frame as it would be on the device without touching the file, follows edits and the editor's
//! selection, and its bar turns the device, switches screens and closes it.

use egui::pos2;
use figcraft_doc::NodeId;
use figcraft_engine::Session;
use serde_json::{Value, json};

use crate::present_tests::{Ui, id};

/// A synthetic Unity-shaped device file (made-up numbers): `w × h` px at 3 px per point, the safe
/// area `top` and `bottom` px in from the screen's edges, portrait and landscape left.
fn device(name: &str, (w, h): (u32, u32), (top, bottom): (u32, u32)) -> Value {
    let screen = json!({
        "width": w, "height": h, "navigationBarHeight": 0, "dpi": 300.0,
        "orientations": [
            {"orientation": 1, "safeArea": {"x": 0.0, "y": f64::from(bottom), "width": f64::from(w), "height": f64::from(h - top - bottom)}, "cutouts": []},
            {"orientation": 3, "safeArea": {"x": f64::from(top), "y": 0.0, "width": f64::from(h - top - bottom), "height": f64::from(w)}, "cutouts": []}
        ],
        "presentation": {"overlayPath": "", "borderSize": {"x": 0.0, "y": 0.0, "z": 0.0, "w": 0.0}, "cornerRadius": 60.0}
    });
    json!({"friendlyName": name, "screens": [screen], "systemInfo": {"deviceModel": "Test1,1", "operatingSystem": "iOS 99.0"}})
}

/// `unity/notch-phone` (400 × 866.67 pt, safe area 50 / 30 pt), `unity/small-phone` (300 × 600 pt,
/// 30 / 20) and `unity/fold-phone` (two screens: 300 × 600 pt and 500 × 600 pt), with a frame
/// "Home" (a badge pinned to its right edge, a fixed card) and a frame "Other".
fn setup() -> (Ui, NodeId, NodeId, NodeId, NodeId) {
    let mut s = Session::new();
    let ex = |s: &mut Session, c: &str, p: Value| s.execute(c, &p).unwrap();
    let mut fold = device("Fold Phone", (900, 1800), (90, 60));
    let inner = device("x", (1500, 1800), (0, 0))["screens"][0].clone();
    fold["screens"].as_array_mut().unwrap().push(inner);
    for d in [device("Notch Phone", (1200, 2600), (150, 90)), device("Small Phone", (900, 1800), (90, 60)), fold] {
        ex(&mut s, "device.import", json!({"text": d.to_string()}));
    }
    let home = id(&ex(&mut s, "node.create", json!({"type": "frame", "name": "Home", "x": 0, "y": 0, "width": 400, "height": 800})));
    let badge =
        id(&ex(&mut s, "node.create", json!({"type": "rectangle", "name": "badge", "parent": home, "x": 340, "y": 100, "width": 40, "height": 20})));
    ex(&mut s, "node.set", json!({"id": badge, "props": {"constraints": {"horizontal": "right", "vertical": "top"}}}));
    let card =
        id(&ex(&mut s, "node.create", json!({"type": "rectangle", "name": "card", "parent": home, "x": 20, "y": 200, "width": 250, "height": 80})));
    let other = id(&ex(&mut s, "node.create", json!({"type": "frame", "name": "Other", "x": 600, "y": 0, "width": 200, "height": 200})));
    (Ui::new(s), home, badge, card, other)
}

fn pane(u: &Ui) -> Value {
    u.app.inspect()["devicePreview"].clone()
}

fn size(v: &Value) -> (f64, f64) {
    (v["size"][0].as_f64().unwrap(), v["size"][1].as_f64().unwrap())
}

fn warned(v: &Value) -> Vec<(NodeId, String)> {
    v["warnings"].as_array().unwrap().iter().map(|w| (id(w), w["kind"].as_str().unwrap().to_string())).collect()
}

fn click(u: &mut Ui, p: egui::Pos2) {
    u.button(p, true);
    u.button(p, false);
    u.frame(vec![]);
}

/// `device.preview`, then the frames a new window takes before it reacts to the pointer.
fn open(u: &mut Ui, params: Value) {
    u.run("device.preview", params);
    u.frames(2, Vec::new);
}

fn rect(u: &Ui) -> egui::Rect {
    u.ctx.memory(|m| m.area_rect(crate::device_preview::pane_id())).expect("the pane is open")
}

#[test]
fn the_pane_shows_the_frame_on_a_device_and_leaves_the_file_alone() {
    let (mut u, home, badge, card, other) = setup();
    assert_eq!(pane(&u), Value::Null);
    let undo = u.app.session.ds().unwrap().history.undo.len();
    open(&mut u, json!({"frame": badge, "device": "unity/small-phone"}));
    let p = pane(&u);
    assert_eq!(
        (id(&json!({"id": p["frame"]})), p["device"].as_str(), p["orientation"].as_str()),
        (home, Some("unity/small-phone"), Some("portrait"))
    );
    assert_eq!(size(&p), (300.0, 600.0));
    assert_eq!(p["safeArea"], json!({"top": 30.0, "right": 0.0, "bottom": 20.0, "left": 0.0}));
    assert_eq!(warned(&p), vec![], "the badge follows the right edge and the card still fits");
    // The file is as designed: no resize, no device on the frame, no undo step.
    let doc = u.app.session.doc().unwrap();
    assert_eq!((doc.node(home).unwrap().width, doc.node(home).unwrap().device.is_none()), (400.0, true));
    assert_eq!(u.app.session.ds().unwrap().history.undo.len(), undo);
    // Edits show at once: a wider card leaves the 300 pt screen.
    u.run("node.set", json!({"id": card, "props": {"width": 320}}));
    assert_eq!(warned(&pane(&u)), vec![(card, "offscreen".to_string())]);
    u.run("edit.undo", json!({}));
    assert_eq!(warned(&pane(&u)), vec![]);
    // Selecting another top-level frame shows that one, and a layer of the first brings it back
    // (the selection as it was when the pane opened changes nothing).
    u.run("select.set", json!({"ids": [card]}));
    assert_eq!(id(&json!({"id": pane(&u)["frame"]})), home);
    u.run("select.set", json!({"ids": [other]}));
    assert_eq!(id(&json!({"id": pane(&u)["frame"]})), other);
    u.run("select.set", json!({"ids": [badge]}));
    assert_eq!(id(&json!({"id": pane(&u)["frame"]})), home);
    // Asked for again with another device and orientation: the pane takes them.
    u.run("device.preview", json!({"frame": home, "device": "unity/notch-phone", "orientation": "landscapeLeft"}));
    let p = pane(&u);
    assert_eq!((size(&p).0.round(), size(&p).1), (867.0, 400.0));
    assert_eq!(p["safeArea"]["left"], json!(50.0));
    u.run("device.closePreview", json!({}));
    assert_eq!(pane(&u), Value::Null);
    // Opened again without a device: the one shown last.
    u.run("device.preview", json!({"frame": home}));
    assert_eq!((pane(&u)["device"].as_str(), pane(&u)["orientation"].as_str()), (Some("unity/notch-phone"), Some("landscapeLeft")));
}

#[test]
fn the_bar_turns_the_device_switches_screens_and_closes() {
    let (mut u, home, _, card, _) = setup();
    open(&mut u, json!({"frame": home, "device": "unity/fold-phone"}));
    assert_eq!((size(&pane(&u)), pane(&u)["screen"].as_u64()), ((300.0, 600.0), Some(0)));
    // Right to left: close, turn, the screen (a device with two).
    let r = rect(&u);
    let (close, turn, screen) = (pos2(r.max.x - 18.0, r.min.y + 18.0), pos2(r.max.x - 44.0, r.min.y + 18.0), pos2(r.max.x - 72.0, r.min.y + 18.0));
    click(&mut u, screen);
    assert_eq!((size(&pane(&u)), pane(&u)["screen"].as_u64()), ((500.0, 600.0), Some(1)), "the inner screen");
    click(&mut u, turn);
    assert_eq!((size(&pane(&u)), pane(&u)["orientation"].as_str()), ((600.0, 500.0), Some("landscapeLeft")));
    click(&mut u, turn);
    click(&mut u, screen);
    assert_eq!((size(&pane(&u)), pane(&u)["screen"].as_u64(), pane(&u)["orientation"].as_str()), ((300.0, 600.0), Some(0), Some("portrait")));
    // The status line's count selects the layers to check.
    u.run("node.set", json!({"id": card, "props": {"width": 320}}));
    assert_eq!(warned(&pane(&u)), vec![(card, "offscreen".to_string())]);
    let r = rect(&u);
    click(&mut u, pos2(r.max.x - 24.0, r.max.y - 12.0));
    assert_eq!(u.app.session.ds().unwrap().selection, vec![card]);
    click(&mut u, close);
    assert_eq!(pane(&u), Value::Null);
}

#[test]
fn the_pane_starts_on_a_built_in_device_and_closes_with_its_frame() {
    let mut s = Session::new();
    let f = id(&s.execute("node.create", &json!({"type": "frame", "x": 0, "y": 0, "width": 100, "height": 100})).unwrap());
    let mut u = Ui::new(s);
    // Nothing imported, nothing chosen yet: the first of FigCraft's own devices.
    u.run("device.preview", json!({"frame": f}));
    let p = pane(&u);
    assert_eq!((p["device"].as_str(), size(&p)), (Some("figcraft/notch-phone"), (390.0, 844.0)));
    assert_eq!(p["safeArea"], json!({"top": 47.0, "right": 0.0, "bottom": 34.0, "left": 0.0}));
    // The device shown is removed while the pane is open: back to the first.
    u.run("device.import", json!({"text": device("Small Phone", (900, 1800), (90, 60)).to_string()}));
    u.run("device.preview", json!({"frame": f, "device": "unity/small-phone"}));
    assert_eq!(size(&pane(&u)), (300.0, 600.0));
    u.run("device.remove", json!({"all": true}));
    assert_eq!((pane(&u)["device"].as_str(), size(&pane(&u))), (Some("figcraft/notch-phone"), (390.0, 844.0)));
    // The frame deleted: the pane closes.
    u.run("edit.delete", json!({"ids": [f]}));
    assert_eq!(pane(&u), Value::Null);
}
