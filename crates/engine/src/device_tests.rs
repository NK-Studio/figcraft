//! `device.*`: importing device profiles and reading safe areas (synthetic devices only).

use serde_json::{Value, json};

use crate::Session;

/// A synthetic Unity-shaped device file (made-up numbers).
fn unity_device(name: &str) -> Value {
    json!({
        "friendlyName": name,
        "screens": [{
            "width": 1200, "height": 2600, "navigationBarHeight": 0, "dpi": 300.0,
            "orientations": [
                {"orientation": 1, "safeArea": {"x": 0.0, "y": 90.0, "width": 1200.0, "height": 2360.0},
                 "cutouts": [{"x": 300.0, "y": 2510.0, "width": 600.0, "height": 90.0}]},
                {"orientation": 3, "safeArea": {"x": 150.0, "y": 63.0, "width": 2300.0, "height": 1137.0}, "cutouts": []}
            ],
            "presentation": {"overlayPath": "DeviceOverlays/frame.png", "borderSize": {"x": 10.0, "y": 10.0, "z": 10.0, "w": 10.0}, "cornerRadius": 120.0}
        }],
        "systemInfo": {"deviceModel": "Test1,1", "operatingSystem": "iOS 99.0"}
    })
}

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("figcraft-device-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn import_text_and_read_safe_area() {
    let mut s = Session::new();
    let r = s.execute("device.import", &json!({"text": unity_device("Test Phone").to_string()})).unwrap();
    assert_eq!(r["imported"], json!(["unity/test-phone"]));
    let g = s.execute("device.get", &json!({"id": "unity/test-phone"})).unwrap();
    assert_eq!(g["orientation"], "portrait");
    assert_eq!(g["scale"], 3.0);
    assert_eq!(g["sizePt"], json!({"width": 400.0, "height": 866.6667}));
    assert_eq!(g["insetsPt"], json!({"top": 50.0, "right": 0.0, "bottom": 30.0, "left": 0.0}));
    assert_eq!(g["safeAreaPx"], json!({"x": 0.0, "y": 150.0, "width": 1200.0, "height": 2360.0}));
    assert_eq!(g["cutoutsPt"], json!([{"x": 100.0, "y": 0.0, "width": 200.0, "height": 30.0}]));
    assert_eq!(g["cornerRadiusPt"], 40.0);
    let l = s.execute("device.get", &json!({"id": "unity/test-phone", "orientation": 3})).unwrap();
    assert_eq!(l["orientation"], "landscapeLeft");
    assert_eq!(l["sizePx"], json!({"width": 2600.0, "height": 1200.0}));
    assert_eq!(l["insetsPx"], json!({"top": 0.0, "right": 150.0, "bottom": 63.0, "left": 150.0}));
    let e = s.execute("device.get", &json!({"id": "unity/test-phone", "orientation": "landscapeRight"})).unwrap_err().to_string();
    assert!(e.contains("doesn't support landscapeRight") && e.contains("portrait, landscapeLeft"), "{e}");
    assert!(s.execute("device.get", &json!({"id": "unity/test-phone", "screen": 1})).is_err());
    assert!(s.execute("device.get", &json!({"id": "nope"})).is_err());
    // Not an undo step: the document history is untouched.
    assert!(s.ds().unwrap().history.undo.is_empty());
}

#[test]
fn import_unity_package_folder_skips_bad_files() {
    let root = temp_dir("pkg");
    let devices = root.join("Editor").join("Devices");
    std::fs::create_dir_all(&devices).unwrap();
    std::fs::write(devices.join("A Phone.device"), unity_device("A Phone").to_string()).unwrap();
    // A BOM, as some editors write.
    let mut b = b"\xEF\xBB\xBF".to_vec();
    b.extend(unity_device("B Phone").to_string().into_bytes());
    std::fs::write(devices.join("B Phone.device"), b).unwrap();
    std::fs::write(devices.join("Broken.device"), "{ not json").unwrap();
    std::fs::write(devices.join("A Phone.device.meta"), "guid: x").unwrap();
    let mut s = Session::new();
    let r = s.execute("device.import", &json!({"path": root.to_string_lossy()})).unwrap();
    assert_eq!(r["imported"], json!(["unity/a-phone", "unity/b-phone"]));
    assert_eq!(r["skipped"].as_array().unwrap().len(), 1);
    assert!(r["skipped"][0]["file"].as_str().unwrap().ends_with("Broken.device"));
    let list = s.execute("device.list", &json!({"platform": "ios"})).unwrap();
    assert_eq!(list.as_array().unwrap().len(), 2);
    assert_eq!(list[0]["screens"][0]["widthPt"], 400.0);
    assert_eq!(list[0]["screens"][0]["orientations"], json!(["portrait", "landscapeLeft"]));
    assert_eq!(s.execute("device.list", &json!({"platform": "android"})).unwrap(), json!([]));
    let g = s.execute("device.get", &json!({"id": "unity/a-phone"})).unwrap();
    assert_eq!(std::path::Path::new(g["overlay"]["path"].as_str().unwrap()), devices.join("DeviceOverlays/frame.png"));
    // Without replace, a second import is skipped per file.
    let again = s.execute("device.import", &json!({"path": devices.to_string_lossy(), "replace": false})).unwrap();
    assert_eq!(again["imported"], json!([]));
    assert_eq!(again["skipped"].as_array().unwrap().len(), 3);
    assert!(s.execute("device.import", &json!({"path": temp_dir("empty").to_string_lossy()})).is_err());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn export_reimports_as_figcraft_device() {
    let dir = temp_dir("export");
    let mut s = Session::new();
    s.execute("device.import", &json!({"text": unity_device("Round Trip").to_string(), "scale": 2})).unwrap();
    let text = s.execute("device.export", &json!({"id": "unity/round-trip"})).unwrap()["json"].as_str().unwrap().to_string();
    assert!(text.contains("figcraft-device"));
    let path = dir.join("custom.figdevice.json");
    assert!(s.execute("device.export", &json!({"id": "unity/round-trip", "path": dir.join("x.txt").to_string_lossy()})).is_err());
    s.execute("device.export", &json!({"id": "unity/round-trip", "path": path.to_string_lossy()})).unwrap();
    assert_eq!(s.execute("device.remove", &json!({"all": true})).unwrap()["removed"], 1);
    s.execute("device.import", &json!({"path": path.to_string_lossy()})).unwrap();
    let g = s.execute("device.get", &json!({"id": "unity/round-trip"})).unwrap();
    assert_eq!((g["scale"].clone(), g["scaleSource"].clone()), (json!(2.0), json!("override")));
    assert_eq!(g["source"]["kind"], "figCraft");
    assert_eq!(g["insetsPt"]["top"], 75.0);
    assert_eq!(s.execute("device.remove", &json!({"id": "unity/round-trip"})).unwrap()["removed"], 1);
    assert_eq!(s.execute("device.remove", &json!({"id": "unity/round-trip"})).unwrap()["removed"], 0);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn import_refusals() {
    let mut s = Session::new();
    assert!(s.execute("device.import", &json!({})).is_err());
    assert!(s.execute("device.import", &json!({"text": "[]"})).is_err());
    assert!(s.execute("device.import", &json!({"text": unity_device("x").to_string(), "scale": 0})).is_err());
    assert!(s.execute("device.import", &json!({"path": "/definitely/not/here.device"})).is_err());
    assert!(s.execute("device.list", &json!({"platform": "palm"})).is_err());
    assert_eq!(s.devices.len(), 0);
}

fn node_id(v: &Value) -> figcraft_doc::NodeId {
    v["id"].as_str().map(|s| s.parse().unwrap()).or_else(|| v["id"].as_u64().map(figcraft_doc::NodeId)).unwrap()
}

fn session_with_device(name: &str) -> (Session, figcraft_doc::NodeId) {
    let mut s = Session::new();
    s.execute("device.import", &json!({"text": unity_device(name).to_string()})).unwrap();
    let f = node_id(&s.execute("frame.create", &json!({"x": 0, "y": 0, "width": 100, "height": 100})).unwrap());
    (s, f)
}

#[test]
fn frame_takes_device_size_and_safe_area_in_one_undo_step() {
    let (mut s, f) = session_with_device("Test Phone");
    let r = s.execute("frame.device.set", &json!({"ids": [f], "device": "unity/test-phone"})).unwrap();
    assert_eq!((r["orientation"].clone(), r["width"].clone(), r["height"].clone()), (json!("portrait"), json!(400.0), json!(866.6667)));
    let n = s.doc().unwrap().node(f).unwrap().clone();
    assert_eq!((n.width, n.height), (400.0, 866.6667));
    let d = n.device.clone().unwrap();
    assert_eq!((d.safe_area.top, d.safe_area.bottom, d.corner_radius), (50.0, 30.0, 40.0));
    assert_eq!(d.cutouts.len(), 1);
    assert_eq!((d.cutouts[0].x, d.cutouts[0].width, d.cutouts[0].height), (100.0, 200.0, 30.0));
    // node.get reports it.
    let g = s.execute("node.get", &json!({"id": f})).unwrap();
    assert_eq!(g["device"]["id"], "unity/test-phone");
    // One undo step puts the size back and drops the device.
    s.execute("edit.undo", &json!({})).unwrap();
    let n = s.doc().unwrap().node(f).unwrap();
    assert_eq!((n.width, n.height, n.device.is_none()), (100.0, 100.0, true));
    // Without resizing, the frame keeps its size.
    s.execute("frame.device.set", &json!({"ids": [f], "device": "unity/test-phone", "orientation": "landscapeLeft", "resize": false})).unwrap();
    let n = s.doc().unwrap().node(f).unwrap();
    assert_eq!((n.width, n.device.as_ref().unwrap().orientation.as_str()), (100.0, "landscapeLeft"));
    assert_eq!(s.execute("frame.device.clear", &json!({"ids": [f]})).unwrap()["cleared"], 1);
    assert!(s.doc().unwrap().node(f).unwrap().device.is_none());
}

#[test]
fn frame_device_refusals() {
    let (mut s, f) = session_with_device("Test Phone");
    let rect = node_id(&s.execute("shape.rectangle", &json!({"x": 0, "y": 0, "width": 10, "height": 10})).unwrap());
    assert!(s.execute("frame.device.set", &json!({"ids": [rect], "device": "unity/test-phone"})).unwrap_err().to_string().contains("not a frame"));
    assert!(s.execute("frame.device.set", &json!({"ids": [f], "device": "nope"})).is_err());
    let e =
        s.execute("frame.device.set", &json!({"ids": [f], "device": "unity/test-phone", "orientation": "landscapeRight"})).unwrap_err().to_string();
    assert!(e.contains("portrait, landscapeLeft"), "{e}");
    assert!(s.execute("frame.device.set", &json!({"ids": [f], "device": "unity/test-phone", "screen": 3})).is_err());
    assert!(s.doc().unwrap().node(f).unwrap().device.is_none());
}

#[test]
fn swapping_orientation_turns_the_device() {
    let (mut s, f) = session_with_device("Test Phone");
    s.execute("frame.device.set", &json!({"ids": [f], "device": "unity/test-phone"})).unwrap();
    s.execute("frame.swapOrientation", &json!({"ids": [f]})).unwrap();
    let n = s.doc().unwrap().node(f).unwrap();
    let d = n.device.as_ref().unwrap();
    assert_eq!((n.width, n.height), (866.6667, 400.0));
    assert_eq!((d.orientation.as_str(), d.safe_area.left, d.safe_area.right, d.safe_area.bottom), ("landscapeLeft", 50.0, 50.0, 21.0));
    s.execute("frame.swapOrientation", &json!({"ids": [f]})).unwrap();
    assert_eq!(s.doc().unwrap().node(f).unwrap().device.as_ref().unwrap().orientation, "portrait");
    // Without the profile the device can't turn, so it's dropped.
    s.execute("device.remove", &json!({"all": true})).unwrap();
    s.execute("frame.swapOrientation", &json!({"ids": [f]})).unwrap();
    assert!(s.doc().unwrap().node(f).unwrap().device.is_none());
}

#[test]
fn device_frames_survive_saving() {
    let (mut s, f) = session_with_device("Test Phone");
    s.execute("frame.device.set", &json!({"ids": [f], "device": "unity/test-phone"})).unwrap();
    let doc = s.doc().unwrap().clone();
    let back: figcraft_doc::Document = serde_json::from_value(serde_json::to_value(&doc).unwrap()).unwrap();
    assert_eq!(back.node(f).unwrap().device, doc.node(f).unwrap().device);
}

#[test]
fn safe_areas_toggle_is_kept() {
    let mut s = Session::new();
    assert_eq!(s.execute("view.safeAreas", &json!({})).unwrap()["on"], false);
    assert_eq!(s.prefs.view.get("view.safeAreas"), Some(&false));
    assert_eq!(s.execute("view.safeAreas", &json!({"on": true})).unwrap()["on"], true);
}

#[test]
fn imported_folders_come_back_at_start_up() {
    let dir = temp_dir("restore");
    std::fs::write(dir.join("R Phone.device"), unity_device("R Phone").to_string()).unwrap();
    let mut s = Session::new();
    s.execute("device.import", &json!({"path": dir.to_string_lossy()})).unwrap();
    s.execute("device.import", &json!({"path": dir.to_string_lossy()})).unwrap();
    assert_eq!(s.prefs.device_sources.len(), 1, "remembered once");
    let mut next = Session::new();
    next.prefs = s.prefs.clone();
    assert_eq!(crate::cmd::device::restore_devices(&mut next), 1);
    assert!(next.devices.get("unity/r-phone").is_some());
    // Gone folders are skipped quietly; removing every device forgets them.
    let _ = std::fs::remove_dir_all(&dir);
    let mut later = Session::new();
    later.prefs = s.prefs.clone();
    assert_eq!(crate::cmd::device::restore_devices(&mut later), 0);
    s.execute("device.remove", &json!({"all": true})).unwrap();
    assert!(s.prefs.device_sources.is_empty());
}

#[test]
fn layers_in_the_safe_area_follow_it_when_the_device_changes() {
    let mut s = Session::new();
    s.execute("device.import", &json!({"text": unity_device("Notch Phone").to_string()})).unwrap();
    // A second device without insets, the same size.
    let mut flat = unity_device("Flat Phone");
    flat["screens"][0]["orientations"][0]["safeArea"] = json!({"x": 0.0, "y": 0.0, "width": 1200.0, "height": 2600.0});
    s.execute("device.import", &json!({"text": flat.to_string()})).unwrap();
    let f = node_id(&s.execute("frame.create", &json!({"x": 0, "y": 0, "width": 100, "height": 100})).unwrap());
    s.execute("frame.device.set", &json!({"ids": [f], "device": "unity/notch-phone"})).unwrap();
    // Notch phone: 400 × 866.67 pt, safe area 50 down from the top and 30 up from the bottom.
    let rect = |s: &mut Session, x: f64, y: f64, h: &str, v: &str| {
        let id = node_id(&s.execute("shape.rectangle", &json!({"x": x, "y": y, "width": 40, "height": 20, "parent": f})).unwrap());
        s.execute("node.set", &json!({"id": id, "props": {"constraints": {"horizontal": h, "vertical": v}}})).unwrap();
        id
    };
    let top = rect(&mut s, 10.0, 60.0, "left", "top");
    let bottom = rect(&mut s, 10.0, 866.6667 - 30.0 - 20.0 - 10.0, "left", "bottom");
    let bleed = rect(&mut s, 0.0, 0.0, "left", "top");
    s.execute("frame.device.set", &json!({"ids": [f], "device": "unity/flat-phone"})).unwrap();
    let y = |s: &Session, id| s.doc().unwrap().node(id).unwrap().transform.as_coeffs()[5];
    // 10 below the safe top (now 0) and 10 above the safe bottom (now the frame's).
    assert!((y(&s, top) - 10.0).abs() < 1e-3, "{}", y(&s, top));
    assert!((y(&s, bottom) - (866.6667 - 20.0 - 10.0)).abs() < 1e-3, "{}", y(&s, bottom));
    // A layer in the unsafe band follows the frame, not the safe area.
    assert_eq!(y(&s, bleed), 0.0);
    // One undo step puts everything back.
    s.execute("edit.undo", &json!({})).unwrap();
    assert!((y(&s, top) - 60.0).abs() < 1e-3);
}
