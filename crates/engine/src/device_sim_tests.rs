//! The device preview's engine side: `device.simulate` and `device.preview` (synthetic devices
//! only). A simulated frame is what `frame.device.set` would leave, the file stays as it is, and
//! the warnings name the layers under a notch or off the screen.

use std::sync::Arc;

use figcraft_doc::{Document, NodeId};
use serde_json::{Value, json};

use crate::Session;
use crate::cmd::device_sim::{WarningKind, simulate, warnings};
use crate::params::id_of;

/// A synthetic Unity-shaped device file (made-up numbers): `w × h` px, portrait only, with the
/// safe area `top` and `bottom` px in from the screen's edges.
fn device(name: &str, (w, h): (u32, u32), (top, bottom): (u32, u32)) -> Value {
    json!({
        "friendlyName": name,
        "screens": [{
            "width": w, "height": h, "navigationBarHeight": 0, "dpi": 300.0,
            "orientations": [
                {"orientation": 1, "safeArea": {"x": 0.0, "y": f64::from(bottom), "width": f64::from(w), "height": f64::from(h - top - bottom)}, "cutouts": []},
                {"orientation": 3, "safeArea": {"x": f64::from(top), "y": 0.0, "width": f64::from(h - top - bottom), "height": f64::from(w)}, "cutouts": []}
            ],
            "presentation": {"overlayPath": "", "borderSize": {"x": 0.0, "y": 0.0, "z": 0.0, "w": 0.0}, "cornerRadius": 0.0}
        }],
        "systemInfo": {"deviceModel": "Test1,1", "operatingSystem": "iOS 99.0"}
    })
}

fn ex(s: &mut Session, cmd: &str, p: Value) -> Value {
    match s.execute(cmd, &p) {
        Ok(v) => v,
        Err(e) => panic!("{cmd} {p}: {e}"),
    }
}

fn create(s: &mut Session, p: Value) -> NodeId {
    id_of(&ex(s, "node.create", p)["id"]).unwrap()
}

/// The devices of these tests, all at 3 px per point:
/// - `unity/notch-phone`: 400 × 866.67 pt, safe area 50 pt from the top and 30 from the bottom;
/// - `unity/big-phone`: 480 × 1040 pt, 60 and 36;
/// - `unity/small-phone`: 300 × 600 pt, 30 and 20.
fn session() -> Session {
    let mut s = Session::new();
    for d in
        [device("Notch Phone", (1200, 2600), (150, 90)), device("Big Phone", (1440, 3120), (180, 108)), device("Small Phone", (900, 1800), (90, 60))]
    {
        ex(&mut s, "device.import", json!({"text": d.to_string()}));
    }
    s
}

/// A screen designed on the notch phone: a background, a title under the notch, a header that
/// bleeds with a label in it, a card and a list item in the safe area, a home bar hint in the
/// bottom band and a hidden layer. Returns the frame and those layers by name.
fn screen(s: &mut Session) -> (NodeId, [NodeId; 7]) {
    let f = create(s, json!({"type": "frame", "name": "Screen", "x": 0, "y": 0, "width": 100, "height": 100}));
    ex(s, "frame.device.set", json!({"ids": [f], "device": "unity/notch-phone"}));
    let mut add = |p: Value, constraints: Value| {
        let id = create(s, p);
        ex(s, "node.set", json!({"id": id, "props": {"constraints": constraints}}));
        id
    };
    let stretch = json!({"horizontal": "leftRight", "vertical": "topBottom"});
    let top_left = json!({"horizontal": "left", "vertical": "top"});
    let bg = add(json!({"type": "rectangle", "name": "bg", "parent": f, "x": 0, "y": 0, "width": 400, "height": 866.6667}), stretch);
    let title = add(json!({"type": "rectangle", "name": "title", "parent": f, "x": 150, "y": 10, "width": 100, "height": 16}), top_left.clone());
    let header = add(
        json!({"type": "frame", "name": "header", "parent": f, "x": 0, "y": 0, "width": 400, "height": 100}),
        json!({"horizontal": "leftRight", "vertical": "top"}),
    );
    let label = add(json!({"type": "rectangle", "name": "label", "parent": header, "x": 20, "y": 20, "width": 60, "height": 16}), top_left.clone());
    let card = add(json!({"type": "rectangle", "name": "card", "parent": f, "x": 20, "y": 120, "width": 360, "height": 100}), top_left.clone());
    let list = add(json!({"type": "rectangle", "name": "list", "parent": f, "x": 20, "y": 700, "width": 100, "height": 100}), top_left.clone());
    let home = add(
        json!({"type": "rectangle", "name": "home", "parent": f, "x": 150, "y": 846.6667, "width": 100, "height": 10}),
        json!({"horizontal": "center", "vertical": "bottom"}),
    );
    let hidden = add(json!({"type": "rectangle", "name": "hidden", "parent": f, "x": 10, "y": 10, "width": 10, "height": 10}), top_left);
    ex(s, "node.set", json!({"id": hidden, "props": {"visible": false}}));
    (f, [bg, title, header, label, card, list, home])
}

/// Every layer's place and size under `root`, in document order.
fn boxes(doc: &Document, root: NodeId) -> Vec<(NodeId, [f64; 6], f64, f64)> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let n = doc.node(id).unwrap();
        out.push((id, n.transform.as_coeffs(), n.width, n.height));
        stack.extend(n.children.iter().rev());
    }
    out
}

fn assert_same(sim: &Document, real: &Document, root: NodeId) {
    let (a, b) = (boxes(sim, root), boxes(real, root));
    assert_eq!(a.len(), b.len());
    for ((ia, ta, wa, ha), (ib, tb, wb, hb)) in a.iter().zip(&b) {
        let near = ta.iter().zip(tb).all(|(x, y)| (x - y).abs() < 1e-9) && (wa - wb).abs() < 1e-9 && (ha - hb).abs() < 1e-9;
        assert!(near && ia == ib, "{ia}: simulated {ta:?} {wa}×{ha}, set {tb:?} {wb}×{hb}");
    }
}

#[test]
fn a_simulated_frame_is_what_setting_the_device_leaves() {
    for scaler in [false, true] {
        for (device, orientation) in [("unity/big-phone", "portrait"), ("unity/small-phone", "portrait"), ("unity/big-phone", "landscapeLeft")] {
            let mut s = session();
            let (f, _) = screen(&mut s);
            // A window filling the safe area (a SafeArea container) with a box in it, and an auto
            // layout row: they follow the safe area and flow again.
            let win = create(&mut s, json!({"type": "frame", "name": "window", "parent": f, "x": 0, "y": 50, "width": 400, "height": 786.6667}));
            ex(&mut s, "node.set", json!({"id": win, "props": {"constraints": {"horizontal": "leftRight", "vertical": "topBottom"}, "fills": []}}));
            let row = create(&mut s, json!({"type": "frame", "name": "row", "parent": win, "x": 10, "y": 10, "width": 380, "height": 40}));
            ex(&mut s, "node.set", json!({"id": row, "props": {"constraints": {"horizontal": "leftRight", "vertical": "top"}}}));
            for _ in 0..3 {
                create(&mut s, json!({"type": "rectangle", "parent": row, "width": 30, "height": 30}));
            }
            ex(&mut s, "layout.add", json!({"ids": [row]}));
            if scaler {
                ex(&mut s, "unity.canvasScaler.set", json!({"ids": [f], "referenceResolution": [1080, 1920], "screenMatchMode": "expand"}));
            }
            let before = s.ds().unwrap().doc.clone();
            let undo = s.ds().unwrap().history.undo.len();
            let r = ex(&mut s, "device.simulate", json!({"frame": f, "device": device, "orientation": orientation}));
            // The file is untouched: the same document, no undo step.
            assert!(Arc::ptr_eq(&before, &s.ds().unwrap().doc), "{device} {orientation}");
            assert_eq!(s.ds().unwrap().history.undo.len(), undo);
            ex(&mut s, "frame.device.set", json!({"ids": [f], "device": device, "orientation": orientation}));
            let real = s.doc().unwrap();
            let snap = real.node(f).unwrap().device.clone().unwrap();
            let sim = simulate(&before, f, &snap).unwrap();
            assert_same(&sim, real, f);
            assert_eq!(sim.node(f).unwrap().device, Some(snap.clone()));
            let n = real.node(f).unwrap();
            assert_eq!((r["width"].as_f64(), r["height"].as_f64()), (Some(n.width), Some(n.height)), "{device} {orientation} {scaler}");
            assert_eq!((r["device"].as_str(), r["orientation"].as_str()), (Some(device), Some(orientation)));
        }
    }
}

#[test]
fn warnings_name_layers_under_the_notch_and_off_the_screen() {
    let mut s = session();
    let (f, [_, title, _, label, card, list, home]) = screen(&mut s);
    let found = |s: &mut Session, device: &str| -> Vec<(NodeId, String, Vec<String>)> {
        let r = ex(s, "device.simulate", json!({"frame": f, "device": device}));
        r["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| {
                (
                    id_of(&w["id"]).unwrap(),
                    w["kind"].as_str().unwrap().to_string(),
                    w["edges"].as_array().unwrap().iter().map(|e| e.as_str().unwrap().to_string()).collect(),
                )
            })
            .collect()
    };
    let edge = |e: &str| vec![e.to_string()];
    let safe = |id: NodeId, e: &str| (id, "safeArea".to_string(), edge(e));
    let off = |id: NodeId, e: &str| (id, "offscreen".to_string(), edge(e));
    // On its own device: the title and the header's label sit under the notch, the home hint in
    // the bottom band. The background and the header bleed, the hidden layer doesn't show.
    assert_eq!(found(&mut s, "unity/notch-phone"), vec![safe(title, "top"), safe(label, "top"), safe(home, "bottom")]);
    // A narrower, shorter screen: the fixed card and the list item leave it too.
    let small = found(&mut s, "unity/small-phone");
    assert_eq!(small, vec![safe(title, "top"), safe(label, "top"), off(card, "right"), off(list, "bottom"), safe(home, "bottom")]);
    // A frame that scrolls down has room below the screen.
    ex(&mut s, "node.set", json!({"id": f, "props": {"overflow": "vertical"}}));
    assert_eq!(found(&mut s, "unity/small-phone"), vec![safe(title, "top"), safe(label, "top"), off(card, "right"), safe(home, "bottom")]);
    // Boxes are in the frame, in points.
    let r = ex(&mut s, "device.simulate", json!({"frame": f, "device": "unity/small-phone"}));
    assert_eq!(r["safeArea"], json!({"top": 30.0, "right": 0.0, "bottom": 20.0, "left": 0.0}));
    assert_eq!(r["warnings"][2]["box"], json!({"x": 20.0, "y": 100.0, "width": 360.0, "height": 100.0}));
    // The library calls agree with the command.
    let doc = s.doc().unwrap();
    let snap = {
        let mut t = session();
        let (tf, _) = screen(&mut t);
        ex(&mut t, "frame.device.set", json!({"ids": [tf], "device": "unity/small-phone"}));
        t.doc().unwrap().node(tf).unwrap().device.clone().unwrap()
    };
    let sim = simulate(doc, f, &snap).unwrap();
    let w = warnings(doc, &sim, f);
    assert_eq!(
        w.iter().map(|w| (w.id, w.kind)).collect::<Vec<_>>(),
        vec![(title, WarningKind::SafeArea), (label, WarningKind::SafeArea), (card, WarningKind::Offscreen), (home, WarningKind::SafeArea)]
    );
}

#[test]
fn simulate_draws_a_png_and_refuses_what_it_cant_use() {
    let mut s = session();
    let (f, [_, title, ..]) = screen(&mut s);
    let dir = std::env::temp_dir().join(format!("figcraft-device-sim-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("small.png");
    // A layer in the frame stands for its frame; the selection is the default.
    let r = ex(&mut s, "device.simulate", json!({"frame": title, "device": "unity/small-phone", "path": path.to_string_lossy(), "scale": 0.5}));
    assert_eq!((id_of(&r["frame"]), r["screen"].as_u64(), r["name"].as_str()), (Some(f), Some(0), Some("Small Phone")));
    let png = std::fs::read(&path).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    // 300 × 600 pt at half a pixel per point.
    assert_eq!((u32::from_be_bytes(png[16..20].try_into().unwrap()), u32::from_be_bytes(png[20..24].try_into().unwrap())), (150, 300));
    ex(&mut s, "select.set", json!({"ids": [title]}));
    assert_eq!(id_of(&ex(&mut s, "device.simulate", json!({"device": "unity/big-phone"}))["frame"]), Some(f));
    for bad in [
        json!({"frame": f}),
        json!({"frame": f, "device": "nope"}),
        json!({"frame": 987_654, "device": "unity/big-phone"}),
        json!({"frame": f, "device": "unity/big-phone", "orientation": "landscapeRight"}),
        json!({"frame": f, "device": "unity/big-phone", "screen": 1}),
        json!({"frame": f, "device": "unity/big-phone", "scale": 9}),
        json!({"frame": f, "device": "unity/big-phone", "path": dir.join("x.txt").to_string_lossy()}),
        json!({"frame": f, "device": "unity/big-phone", "path": 5}),
        json!({"frame": f, "device": "unity/big-phone", "devise": 1}),
    ] {
        assert!(s.execute("device.simulate", &bad).is_err(), "{bad}");
    }
    assert!(!dir.join("x.txt").exists());
    let _ = std::fs::remove_dir_all(&dir);
    // No frame at all.
    let mut empty = session();
    assert!(empty.execute("device.simulate", &json!({"device": "unity/big-phone"})).unwrap_err().to_string().contains("no frame"));
}

#[test]
fn preview_asks_the_ui_for_a_frame_and_a_device() {
    let mut s = session();
    let (f, [_, title, ..]) = screen(&mut s);
    let last = |s: &mut Session| {
        let r = s.ui_requests.pop().unwrap();
        s.ui_requests.clear();
        (r.kind, r.data)
    };
    let undo = s.ds().unwrap().history.undo.len();
    let r = ex(&mut s, "device.preview", json!({"frame": title}));
    assert_eq!(r, json!({"frame": f}));
    assert_eq!(last(&mut s), ("devicePreview".to_string(), json!({"frame": f})));
    ex(&mut s, "device.preview", json!({"device": "unity/big-phone", "orientation": "landscapeLeft"}));
    assert_eq!(
        last(&mut s),
        ("devicePreview".to_string(), json!({"frame": f, "device": "unity/big-phone", "orientation": "landscapeLeft", "screen": 0}))
    );
    for bad in [json!({"device": "nope"}), json!({"orientation": "portrait"}), json!({"screen": 0}), json!({"frame": 987_654}), json!({"device": 7})]
    {
        assert!(s.execute("device.preview", &bad).is_err(), "{bad}");
    }
    assert!(s.ui_requests.is_empty());
    ex(&mut s, "device.closePreview", json!({}));
    assert_eq!(last(&mut s).0, "devicePreview.close");
    assert_eq!(s.ds().unwrap().history.undo.len(), undo, "not undo steps");
}
