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
