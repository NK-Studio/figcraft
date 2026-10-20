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
