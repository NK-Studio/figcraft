//! `spine.*` and Spine folders through `file.import`: placing a skeleton from its folder or from
//! data, posing it, and the refusals.

use std::path::PathBuf;

use base64::Engine as _;
use figcraft_spine::fixture::{ATLAS, PAGE, SKELETON_JSON};
use serde_json::{Value, json};

use crate::Session;

fn page_png() -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(PAGE.1, PAGE.2, image::Rgba([200, 40, 40, 255]));
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
    out
}

/// The fixture written out as an export: `figure.json`, `figure.atlas`, `figure.png`.
fn export_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("figcraft-spine-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("figure.json"), SKELETON_JSON).unwrap();
    std::fs::write(dir.join("figure.atlas"), ATLAS).unwrap();
    std::fs::write(dir.join(PAGE.0), page_png()).unwrap();
    dir
}

fn id(r: &Value) -> u64 {
    r["id"].as_u64().unwrap()
}

#[test]
fn imports_a_skeleton_from_its_folder() {
    let dir = export_dir("import");
    let mut s = Session::new();
    let r = s.execute("file.import", &json!({"path": dir, "x": 10, "y": 20})).unwrap();
    let n = id(&r);
    assert_eq!(r["width"], 64.0);
    assert_eq!(r["height"], 32.0);
    assert_eq!(r["animations"][0]["name"], "wave");
    let info = s.execute("spine.info", &json!({})).unwrap();
    assert_eq!(info["id"], n);
    assert_eq!(info["name"], "figure");
    assert_eq!(info["skins"], json!(["default", "bare"]));
    assert_eq!(info["pages"], json!(["figure.png"]));
    // spine.place takes the folder too; both layers use the one stored skeleton.
    let r = s.execute("spine.place", &json!({"dir": dir})).unwrap();
    assert_ne!(id(&r), n);
    assert_eq!(s.doc().unwrap().spine.len(), 1);
    // Without a folder (from the menus) it asks for one and places nothing.
    assert_eq!(s.execute("spine.place", &json!({})).unwrap()["placed"], false);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn single_files_are_refused_and_unity_names_work() {
    let dir = export_dir("files");
    let mut s = Session::new();
    for f in ["figure.json", "figure.atlas"] {
        let e = s.execute("file.import", &json!({"path": dir.join(f)})).unwrap_err().to_string();
        assert!(e.contains("imports as its folder"), "{f}: {e}");
    }
    let e = s.execute("spine.place", &json!({"dir": dir.join("figure.json")})).unwrap_err().to_string();
    assert!(e.contains("isn't a folder"), "{e}");
    // A folder copied out of a Unity project: `.skel.bytes`-style and `.atlas.txt` names.
    std::fs::rename(dir.join("figure.atlas"), dir.join("figure.atlas.txt")).unwrap();
    assert!(s.execute("spine.place", &json!({"dir": dir})).is_ok());
    // Several skeletons: name one.
    std::fs::write(dir.join("other.json"), SKELETON_JSON).unwrap();
    let e = s.execute("spine.place", &json!({"dir": dir})).unwrap_err().to_string();
    assert!(e.contains("several skeletons (figure, other)"), "{e}");
    let r = s.execute("spine.place", &json!({"dir": dir, "skeleton": "other"})).unwrap();
    assert_eq!(s.execute("spine.info", &json!({"id": id(&r)})).unwrap()["name"], "other");
    assert!(s.execute("spine.place", &json!({"dir": dir, "skeleton": "nope"})).unwrap_err().to_string().contains("no skeleton named"));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn places_from_data_and_round_trips() {
    let b64 = |b: &[u8]| base64::engine::general_purpose::STANDARD.encode(b);
    let mut s = Session::new();
    let r = s
        .execute(
            "spine.place",
            &json!({"skeleton": b64(SKELETON_JSON.as_bytes()), "atlas": ATLAS, "pages": {PAGE.0: b64(&page_png())}, "name": "Hero", "animation": "wave"}),
        )
        .unwrap();
    let n = id(&r);
    s.execute("spine.set", &json!({"ids": [n], "time": 0.25, "playing": true, "speed": 2, "skin": "bare"})).unwrap();
    let info = s.execute("spine.info", &json!({"id": n})).unwrap();
    assert_eq!((info["animation"].clone(), info["skin"].clone(), info["time"].clone()), (json!("wave"), json!("bare"), json!(0.25)));
    assert_eq!((info["playing"].clone(), info["speed"].clone()), (json!(true), json!(2.0)));
    // Undo goes back a step; null clears to the setup pose.
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.execute("spine.info", &json!({"id": n})).unwrap()["time"], 0.0);
    s.execute("spine.set", &json!({"ids": [n], "animation": null})).unwrap();
    assert_eq!(s.execute("spine.info", &json!({"id": n})).unwrap()["animation"], Value::Null);
    // It renders, and saves and loads with its skeleton.
    assert!(figcraft_render::render_node(s.doc().unwrap(), figcraft_doc::NodeId(n), 1.0).is_some());
    let bytes = figcraft_format::save_bytes(s.doc().unwrap(), None).unwrap();
    let (back, issues) = figcraft_format::load_bytes(&bytes).unwrap();
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(back.spine, s.doc().unwrap().spine);
}

#[test]
fn refusals() {
    let dir = export_dir("refuse");
    let mut s = Session::new();
    let path = &dir;
    let err = |s: &mut Session, cmd: &str, p: Value| s.execute(cmd, &p).unwrap_err().to_string();
    assert!(err(&mut s, "spine.place", json!({"dir": path, "animation": "nope"})).contains("no animation"));
    std::fs::remove_file(dir.join(PAGE.0)).unwrap();
    assert!(err(&mut s, "spine.place", json!({"dir": path})).contains("figure.png"));
    std::fs::remove_file(dir.join("figure.atlas")).unwrap();
    assert!(err(&mut s, "spine.place", json!({"dir": path})).contains("no atlas"));
    // A JSON that isn't Spine still goes to the document importer.
    std::fs::write(dir.join("other.json"), "{\"skeleton\": 1}").unwrap();
    assert!(!err(&mut s, "file.import", json!({"path": dir.join("other.json")})).contains("Spine"));
    // Pages named outside the atlas folder are refused.
    std::fs::write(dir.join("figure.atlas"), ATLAS.replace("figure.png", "../figure.png")).unwrap();
    std::fs::write(dir.join(PAGE.0), page_png()).unwrap();
    assert!(err(&mut s, "spine.place", json!({"dir": path})).contains("isn't a file beside"));
    // Data without its pages, and spine.set on a rectangle.
    assert!(err(&mut s, "spine.place", json!({"skeleton": "e30=", "atlas": ""})).contains("Spine"));
    let r = id(&s.execute("shape.rectangle", &json!({"x": 0, "y": 0, "width": 10, "height": 10})).unwrap());
    assert!(err(&mut s, "spine.set", json!({"ids": [r], "time": 1})).contains("isn't a Spine layer"));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn junk_on_a_spine_layer_never_panics() {
    let dir = export_dir("junk");
    let mut s = Session::new();
    let n = id(&s.execute("file.import", &json!({"path": dir})).unwrap());
    let junk = [
        json!({"ids": [n], "animation": 5, "skin": [1], "time": "x"}),
        json!({"ids": [n], "time": 1e308, "speed": -1e308, "looping": "yes", "playing": null}),
        json!({"ids": [n], "animation": "", "skin": "\u{0}"}),
        json!({"id": n, "path": [1], "skeleton": 7, "atlas": {"x": 1}, "pages": {"a": 5}}),
        json!({"skeleton": "////", "atlas": "\u{0}\n", "pages": {"../x": "AAAA"}, "binary": true}),
    ];
    for cmd in ["spine.set", "spine.info", "spine.place"] {
        for j in &junk {
            if let Err(crate::EngineError::Internal { cmd, msg }) = s.execute(cmd, j) {
                panic!("{cmd} panicked on {j}: {msg}");
            }
        }
    }
    // Junk times still pose.
    s.execute("spine.set", &json!({"ids": [n], "animation": "wave", "time": 1e12, "looping": true})).unwrap();
    assert!(figcraft_render::render_node(s.doc().unwrap(), figcraft_doc::NodeId(n), 1.0).is_some());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn unity_export_carries_the_skeleton_files() {
    let dir = export_dir("unity");
    let out = dir.join("unity");
    let mut s = Session::new();
    let f = id(&s.execute("frame.create", &json!({"x": 0, "y": 0, "width": 200, "height": 100, "name": "Screen"})).unwrap());
    let n = id(&s.execute("spine.place", &json!({"dir": dir, "parent": f, "x": 10, "y": 10, "name": "Hero"})).unwrap());
    s.execute("spine.set", &json!({"ids": [n], "animation": "wave", "playing": true})).unwrap();
    let check = s.execute("unity.check", &json!({"ids": [f]})).unwrap();
    assert!(check["issues"].as_array().unwrap().iter().any(|i| i["rule"] == "spine"), "{check}");
    let r = s.execute("file.exportUitk", &json!({"ids": [f], "dir": out})).unwrap();
    let uxml = std::fs::read_to_string(out.join("Screen.uxml")).unwrap();
    assert!(uxml.contains("name=\"Hero\"") && uxml.contains("spine-skeleton"), "{uxml}");
    let uss = std::fs::read_to_string(out.join("Screen.uss")).unwrap();
    assert!(uss.contains(&format!("baked-{n}.png")), "{uss}");
    let manifest: Value = serde_json::from_slice(&std::fs::read(out.join("Screen.spine.json")).unwrap()).unwrap();
    let sk = &manifest["skeletons"][0];
    assert_eq!((sk["name"].clone(), sk["animation"].clone(), sk["playing"].clone()), (json!("Hero"), json!("wave"), json!(true)));
    assert_eq!(sk["skeleton"], "spine/figure/figure.json");
    assert_eq!(sk["atlas"], "spine/figure/figure.atlas.txt");
    assert_eq!(std::fs::read_to_string(out.join("spine/figure/figure.json")).unwrap(), SKELETON_JSON);
    assert_eq!(std::fs::read_to_string(out.join("spine/figure/figure.atlas.txt")).unwrap(), ATLAS);
    assert!(out.join("spine/figure").join(PAGE.0).is_file());
    assert!(out.join(format!("images/baked-{n}.png")).is_file());
    assert_eq!(r["documents"][0]["spine"][0]["id"], n);
    std::fs::remove_dir_all(dir).ok();
}
