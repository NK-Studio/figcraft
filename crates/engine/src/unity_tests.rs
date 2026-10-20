//! `unity.check`: the Unity UI Toolkit compatibility check as agents and the UI call it.

use serde_json::json;

use crate::Session;

fn frame_with_gap(s: &mut Session) -> u64 {
    let r = s.execute("frame.create", &json!({"x": 0, "y": 0, "width": 300, "height": 100})).unwrap();
    let id = r["id"].as_u64().unwrap();
    s.execute("layout.add", &json!({"ids": [id]})).unwrap();
    s.execute("node.set", &json!({"id": id, "props": {"itemSpacing": 12}})).unwrap();
    id
}

#[test]
fn reports_by_target_version() {
    let mut s = Session::new();
    let f = frame_with_gap(&mut s);
    let gap = |r: &serde_json::Value| r["issues"].as_array().unwrap().iter().any(|i| i["rule"] == "gap" && i["id"] == json!(f));
    // The default target is 6.0, and with nothing selected the page is checked.
    s.execute("select.none", &json!({})).unwrap();
    let r = s.execute("unity.check", &json!({})).unwrap();
    assert_eq!(r["version"], "6.0");
    assert!(gap(&r), "{r}");
    assert_eq!(r["counts"]["info"], 1);
    for version in [json!("6.7"), json!("6000.7.0f1"), json!(6.7)] {
        let r = s.execute("unity.check", &json!({"version": version})).unwrap();
        assert!(!gap(&r), "{version}: {r}");
    }
    let r = s.execute("unity.check", &json!({"ids": [f], "version": "6.3"})).unwrap();
    assert_eq!(r["version"], "6.3");
    assert!(gap(&r));
}

#[test]
fn grid_option_and_refusals() {
    let mut s = Session::new();
    let f = frame_with_gap(&mut s);
    s.execute("node.set", &json!({"id": f, "props": {"layoutMode": "grid"}})).unwrap();
    let sev = |r: &serde_json::Value| r["issues"][0]["severity"].clone();
    assert_eq!(sev(&s.execute("unity.check", &json!({"ids": [f], "version": "6.7"})).unwrap()), "error");
    assert_eq!(sev(&s.execute("unity.check", &json!({"ids": [f], "version": "6.7", "grid": true})).unwrap()), "info");
    for bad in [json!("5.6"), json!("2022.3"), json!("seven"), json!([6])] {
        assert!(s.execute("unity.check", &json!({"version": bad})).is_err(), "{bad}");
    }
    // Not an undo step.
    let before = s.ds().unwrap().history.undo.len();
    s.execute("unity.check", &json!({})).unwrap();
    assert_eq!(s.ds().unwrap().history.undo.len(), before);
}

#[test]
fn file_settings_drive_the_check_and_undo() {
    let mut s = Session::new();
    let f = frame_with_gap(&mut s);
    assert_eq!(s.execute("unity.settings", &json!({})).unwrap()["version"], "6.0");
    let r = s.execute("unity.settings.set", &json!({"version": "6000.7", "grid": true})).unwrap();
    assert_eq!((r["version"].clone(), r["grid"].clone()), (json!("6.7"), json!(true)));
    assert_eq!(r["versions"].as_array().unwrap().len(), 8);
    // The check takes the file's version unless told otherwise.
    let c = s.execute("unity.check", &json!({"ids": [f]})).unwrap();
    assert_eq!((c["version"].clone(), c["counts"]["info"].clone()), (json!("6.7"), json!(0)));
    assert_eq!(s.execute("unity.check", &json!({"ids": [f], "version": "6.0"})).unwrap()["counts"]["info"], 1);
    // Saved with the file, one undo step.
    let doc = s.doc().unwrap().clone();
    let back: figcraft_doc::Document = serde_json::from_value(serde_json::to_value(&doc).unwrap()).unwrap();
    assert_eq!(back.unity, doc.unity);
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.execute("unity.settings", &json!({})).unwrap()["version"], "6.0");
    assert!(s.execute("unity.settings.set", &json!({"version": "5.6"})).is_err());
    assert!(s.execute("unity.settings.set", &json!({"grid": "maybe"})).is_err());
}

#[test]
fn layer_position_and_pivot() {
    let mut s = Session::new();
    let row = frame_with_gap(&mut s);
    let child = s.execute("shape.rectangle", &json!({"x": 0, "y": 0, "width": 20, "height": 20, "parent": row})).unwrap()["id"].as_u64().unwrap();
    let free = s.execute("frame.create", &json!({"x": 400, "y": 0, "width": 200, "height": 200})).unwrap()["id"].as_u64().unwrap();
    let inside = s.execute("shape.rectangle", &json!({"x": 10, "y": 10, "width": 20, "height": 20, "parent": free})).unwrap()["id"].as_u64().unwrap();
    let info = |s: &mut Session, id: u64| s.execute("unity.layer", &json!({"id": id})).unwrap();
    assert_eq!(info(&mut s, row)["position"], "root");
    assert_eq!(info(&mut s, child)["position"], "relative");
    let i = info(&mut s, inside);
    assert_eq!((i["position"].clone(), i["pivotSet"].clone(), i["pivot"].clone()), (json!("absolute"), json!(false), json!({"x": 0.0, "y": 0.0})));
    // Constraints imply the pivot until one is set.
    s.execute("node.set", &json!({"id": inside, "props": {"constraints": {"horizontal": "right", "vertical": "center"}}})).unwrap();
    assert_eq!(info(&mut s, inside)["pivot"], json!({"x": 1.0, "y": 0.5}));
    s.execute("unity.layer.set", &json!({"ids": [inside], "pivot": "center"})).unwrap();
    let i = info(&mut s, inside);
    assert_eq!((i["pivot"].clone(), i["pivotSet"].clone()), (json!({"x": 0.5, "y": 0.5}), json!(true)));
    assert_eq!(s.execute("node.get", &json!({"id": inside})).unwrap()["pivot"], json!({"x": 0.5, "y": 0.5}));
    s.execute("unity.layer.set", &json!({"ids": [inside], "pivot": [0.25, 1]})).unwrap();
    assert_eq!(info(&mut s, inside)["pivot"], json!({"x": 0.25, "y": 1.0}));
    s.execute("unity.layer.set", &json!({"ids": [inside], "pivot": null})).unwrap();
    assert_eq!(info(&mut s, inside)["pivotSet"], false);
    // Position switches in auto layout only.
    s.execute("unity.layer.set", &json!({"ids": [child], "position": "absolute"})).unwrap();
    assert_eq!(info(&mut s, child)["position"], "absolute");
    s.execute("unity.layer.set", &json!({"ids": [child], "position": "relative"})).unwrap();
    assert_eq!(info(&mut s, child)["position"], "relative");
    assert!(s.execute("unity.layer.set", &json!({"ids": [inside], "position": "relative"})).unwrap_err().to_string().contains("always absolute"));
    assert!(s.execute("unity.layer.set", &json!({"ids": [row], "position": "absolute"})).is_err());
    for bad in [json!([2, 0]), json!("sideways"), json!({"x": "NaN", "y": 0}), json!([0.5])] {
        assert!(s.execute("unity.layer.set", &json!({"ids": [inside], "pivot": bad})).is_err(), "{bad}");
    }
    assert!(s.execute("unity.layer.set", &json!({"ids": [inside]})).is_err());
    // A pivot on a layer in the flow is reported as ignored.
    s.execute("unity.layer.set", &json!({"ids": [child], "pivot": "center"})).unwrap();
    let c = s.execute("unity.check", &json!({"ids": [child]})).unwrap();
    assert!(c["issues"].as_array().unwrap().iter().any(|i| i["rule"] == "pivot"), "{c}");
}

#[test]
fn export_returns_text_or_writes_a_folder() {
    let mut s = Session::new();
    let f = frame_with_gap(&mut s);
    s.execute("shape.ellipse", &json!({"x": 10, "y": 10, "width": 20, "height": 20, "parent": f})).unwrap();
    s.execute("select.none", &json!({})).unwrap();
    // No folder: the text comes back (every top-level frame on the page).
    let r = s.execute("file.exportUitk", &json!({})).unwrap();
    assert_eq!(r["version"], "6.0");
    let d = &r["documents"][0];
    assert_eq!(d["frame"], json!(f));
    assert!(d["uxml"].as_str().unwrap().contains("<ui:UXML"));
    assert!(d["uss"].as_str().unwrap().contains(&format!(".fc-{f} {{")));
    assert_eq!(r["bakes"].as_array().unwrap().len(), 1, "the ellipse");
    // A folder: files, the baked PNG and the scripts.
    let dir = std::env::temp_dir().join(format!("figcraft-uitk-export-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let r = s.execute("file.exportUitk", &json!({"dir": dir.to_string_lossy(), "version": "6.3"})).unwrap();
    let files: Vec<String> = r["files"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    let stem = r["documents"][0]["stem"].as_str().unwrap().to_string();
    for want in [format!("{stem}.uxml"), format!("{stem}.uss"), "FigcraftPanelScale.cs".into(), "FigcraftSafeArea.cs".into()] {
        assert!(dir.join(&want).is_file(), "{want} in {files:?}");
    }
    let png = std::fs::read(dir.join(r["bakes"][0]["file"].as_str().unwrap())).unwrap();
    assert!(png.starts_with(b"\x89PNG"));
    assert!(r["documents"][0].get("uxml").is_none());
    let _ = std::fs::remove_dir_all(&dir);
    assert!(s.execute("file.exportUitk", &json!({"dir": 5})).is_err());
    assert!(s.execute("file.exportUitk", &json!({"scale": 100})).is_err());
}

#[test]
fn export_picks_the_selections_top_level_frames() {
    let mut s = Session::new();
    let a = frame_with_gap(&mut s);
    let b = s.execute("frame.create", &json!({"x": 500, "y": 0, "width": 100, "height": 100})).unwrap()["id"].as_u64().unwrap();
    let inner = s.execute("shape.rectangle", &json!({"x": 0, "y": 0, "width": 10, "height": 10, "parent": b})).unwrap()["id"].as_u64().unwrap();
    s.execute("select.set", &json!({"ids": [inner]})).unwrap();
    let r = s.execute("file.exportUitk", &json!({})).unwrap();
    let frames: Vec<u64> = r["documents"].as_array().unwrap().iter().map(|d| d["frame"].as_u64().unwrap()).collect();
    assert_eq!(frames, vec![b]);
    let r = s.execute("file.exportUitk", &json!({"ids": [a, b, a]})).unwrap();
    assert_eq!(r["documents"].as_array().unwrap().len(), 2);
    let mut empty = Session::new();
    assert!(empty.execute("file.exportUitk", &json!({})).is_err());
}

#[test]
fn text_boxes_follow_the_file_setting_or_the_parameter() {
    let mut s = Session::new();
    let f = frame_with_gap(&mut s);
    let t = s.execute("text.create", &json!({"text": "Hi", "x": 10, "y": 10, "parent": f})).unwrap()["id"].as_u64().unwrap();
    let uss = |s: &mut Session, p: serde_json::Value| s.execute("file.exportUitk", &p).unwrap()["documents"][0]["uss"].as_str().unwrap().to_string();
    let label = |uss: &str| uss[uss.find(&format!(".fc-{t} {{")).unwrap()..].split('}').next().unwrap().to_string();
    assert!(!label(&uss(&mut s, json!({"ids": [f]}))).contains("width:"));
    assert!(label(&uss(&mut s, json!({"ids": [f], "textBoxes": "fixed"}))).contains("width:"));
    let r = s.execute("unity.settings.set", &json!({"fixedTextBoxes": true})).unwrap();
    assert_eq!(r["fixedTextBoxes"], true);
    assert!(label(&uss(&mut s, json!({"ids": [f]}))).contains("width:"));
    assert!(!label(&uss(&mut s, json!({"ids": [f], "textBoxes": "auto"}))).contains("width:"));
    assert!(s.execute("file.exportUitk", &json!({"textBoxes": "maybe"})).is_err());
    assert!(s.execute("unity.settings.set", &json!({"fixedTextBoxes": "x"})).is_err());
}
