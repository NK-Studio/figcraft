use serde_json::{Value, json};

use crate::*;

/// A synthetic, Unity-shaped device (made-up numbers; no Unity file is copied).
fn unity_fixture() -> Value {
    json!({
        "friendlyName": "Test Phone X1",
        "version": 1,
        "screens": [{
            "width": 1200, "height": 2600, "navigationBarHeight": 0, "dpi": 300.0,
            "orientations": [
                {"orientation": 1, "safeArea": {"serializedVersion": "2", "x": 0.0, "y": 90.0, "width": 1200.0, "height": 2360.0},
                 "cutouts": [{"serializedVersion": "2", "x": 300.0, "y": 2510.0, "width": 600.0, "height": 90.0}]},
                {"orientation": 2, "safeArea": {"x": 0.0, "y": 150.0, "width": 1200.0, "height": 2360.0}, "cutouts": []},
                {"orientation": 3, "safeArea": {"x": 150.0, "y": 63.0, "width": 2300.0, "height": 1137.0}, "cutouts": []},
                {"orientation": 5, "safeArea": {"x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0}}
            ],
            "presentation": {"overlayPath": "Overlays/Test Phone X1.png", "borderSize": {"x": 10.0, "y": 20.0, "z": 30.0, "w": 40.0}, "cornerRadius": 120.0}
        }],
        "systemInfo": {"deviceModel": "Test1,1", "operatingSystem": "iOS 99.0", "graphicsDependentData": [{"graphicsDeviceType": 16}]}
    })
}

fn bytes(v: &Value) -> Vec<u8> {
    serde_json::to_vec(v).unwrap()
}

fn parse_fixture() -> DeviceProfile {
    parse(&bytes(&unity_fixture()), &ParseHint::default()).unwrap()
}

#[test]
fn unity_safe_area_flips_to_top_left_insets() {
    let p = parse_fixture();
    assert_eq!(p.id, "unity/test-phone-x1");
    assert_eq!(p.platform, Platform::Ios);
    assert_eq!(p.model.as_deref(), Some("Test1,1"));
    let s = &p.screens[0];
    assert_eq!((s.scale, s.scale_source), (3.0, ScaleSource::InferredIos));
    let i = s.insets_px(Orientation::Portrait).unwrap();
    assert_eq!((i.top, i.right, i.bottom, i.left), (150.0, 0.0, 90.0, 0.0));
    let pt = s.insets_pt(Orientation::Portrait).unwrap();
    assert_eq!((pt.top, pt.bottom), (50.0, 30.0));
    assert_eq!(s.size_pt(Orientation::Portrait), (400.0, 2600.0 / 3.0));
    let r = s.safe_area_px(Orientation::Portrait).unwrap();
    assert_eq!((r.x, r.y, r.width, r.height), (0.0, 150.0, 1200.0, 2360.0));
    // Upside down: the larger inset is now at the bottom.
    let u = s.insets_px(Orientation::PortraitUpsideDown).unwrap();
    assert_eq!((u.top, u.bottom), (90.0, 150.0));
}

#[test]
fn unity_landscape_swaps_size_and_measures_in_its_own_frame() {
    let s = parse_fixture().screens.remove(0);
    assert_eq!(s.size_px(Orientation::LandscapeLeft), (2600.0, 1200.0));
    let i = s.insets_px(Orientation::LandscapeLeft).unwrap();
    assert_eq!((i.top, i.right, i.bottom, i.left), (0.0, 150.0, 63.0, 150.0));
    // Not in the file: unsupported, and Unity's value 5 is skipped.
    assert!(s.info(Orientation::LandscapeRight).is_none());
    assert_eq!(s.supported(), vec![Orientation::Portrait, Orientation::PortraitUpsideDown, Orientation::LandscapeLeft]);
}

#[test]
fn unity_cutouts_flip_and_scale() {
    let s = parse_fixture().screens.remove(0);
    let c = s.info(Orientation::Portrait).unwrap().cutouts[0];
    assert_eq!((c.x, c.y, c.width, c.height), (300.0, 0.0, 600.0, 90.0));
    let pt = s.cutouts_pt(Orientation::Portrait)[0];
    assert_eq!((pt.x, pt.y, pt.width, pt.height), (100.0, 0.0, 200.0, 30.0));
    assert!(s.cutouts_pt(Orientation::LandscapeRight).is_empty());
}

#[test]
fn unity_overlay_and_border() {
    let hint = ParseHint { dir: Some("/devices".into()), file: Some("/devices/x.device".into()), ..Default::default() };
    let p = parse(&bytes(&unity_fixture()), &hint).unwrap();
    let o = p.screens[0].overlay.clone().unwrap();
    assert_eq!(std::path::Path::new(&o.path), std::path::Path::new("/devices/Overlays/Test Phone X1.png"));
    assert_eq!((o.border_px.left, o.border_px.top, o.border_px.right, o.border_px.bottom), (10.0, 20.0, 30.0, 40.0));
    assert_eq!(p.screens[0].corner_radius_px, 120.0);
    assert_eq!(p.source, Source::UnityDevice { file: Some("/devices/x.device".into()) });
}

#[test]
fn scale_inference() {
    let ios = |w, h| infer_scale_for(Platform::Ios, w, h, None);
    assert_eq!(ios(640.0, 1136.0), 2.0);
    assert_eq!(ios(750.0, 1334.0), 2.0);
    assert_eq!(ios(828.0, 1792.0), 2.0);
    assert_eq!(ios(1080.0, 1920.0), 1080.0 / 414.0);
    assert_eq!(ios(1080.0, 2338.0), 3.0);
    assert_eq!(ios(1284.0, 2778.0), 3.0);
    assert_eq!(ios(1640.0, 2360.0), 2.0);
    assert_eq!(ios(2048.0, 2732.0), 2.0);
    assert_eq!(unity::infer_scale(Platform::Android, 1080.0, 1920.0, Some(420.0)), (2.625, ScaleSource::InferredAndroid));
    assert_eq!(unity::infer_scale(Platform::Android, 1080.0, 1920.0, Some(0.0)), (1.0, ScaleSource::Default));
    assert_eq!(unity::infer_scale(Platform::Other, 1080.0, 1920.0, Some(420.0)), (1.0, ScaleSource::Default));
}

fn infer_scale_for(p: Platform, w: f64, h: f64, dpi: Option<f64>) -> f64 {
    unity::infer_scale(p, w, h, dpi).0
}

#[test]
fn platform_detection() {
    assert_eq!(unity::platform_of("Android OS 9 / API-28", "", ""), Platform::Android);
    assert_eq!(unity::platform_of("iPadOS 17", "", ""), Platform::Ios);
    assert_eq!(unity::platform_of("", "", "Some iPad"), Platform::Ios);
    assert_eq!(unity::platform_of("", "", "Desk screen"), Platform::Other);
}

#[test]
fn scale_override_and_bom() {
    let mut b = b"\xEF\xBB\xBF".to_vec();
    b.extend(bytes(&unity_fixture()));
    let p = parse(&b, &ParseHint { scale: Some(2.0), ..Default::default() }).unwrap();
    assert_eq!((p.screens[0].scale, p.screens[0].scale_source), (2.0, ScaleSource::Override));
    assert_eq!(p.screens[0].insets_pt(Orientation::Portrait).unwrap().top, 75.0);
    for bad in [0.0, -1.0, 11.0, f64::NAN, f64::INFINITY] {
        assert!(parse(&b, &ParseHint { scale: Some(bad), ..Default::default() }).is_err(), "{bad}");
    }
}

#[test]
fn android_navigation_bar_and_density() {
    let mut v = unity_fixture();
    v["systemInfo"]["operatingSystem"] = json!("Android OS 12");
    v["screens"][0]["dpi"] = json!(480.0);
    v["screens"][0]["navigationBarHeight"] = json!(126);
    let s = parse(&bytes(&v), &ParseHint::default()).unwrap().screens.remove(0);
    assert_eq!((s.scale, s.navigation_bar_px), (3.0, 126.0));
}

#[test]
fn refuses_bad_files() {
    let p = |b: &[u8]| parse(b, &ParseHint::default());
    assert!(matches!(p(b"not json"), Err(DeviceError::Format(_))));
    assert!(matches!(p(b"{\"hello\": 1}"), Err(DeviceError::Format(_))));
    assert!(matches!(p(&vec![b' '; MAX_FILE_BYTES as usize + 1]), Err(DeviceError::TooLarge)));
    assert!(p(br#"{"friendlyName": "x", "screens": 5}"#).is_err());
    assert!(p(br#"{"friendlyName": "x", "screens": []}"#).is_err());
    assert!(p(br#"{"friendlyName": "x", "screens": [{"width": 1e400, "height": 2}]}"#).is_err());
    let with = |path: &str, val: Value| {
        let mut v = unity_fixture();
        let mut at = &mut v;
        for k in path.split('.') {
            at = match k.parse::<usize>() {
                Ok(i) => &mut at[i],
                Err(_) => &mut at[k],
            };
        }
        *at = val;
        parse(&bytes(&v), &ParseHint::default())
    };
    assert!(with("screens.0.width", json!(0)).is_err());
    assert!(with("screens.0.width", json!(-5)).is_err());
    assert!(with("screens.0.height", json!(40000)).is_err());
    assert!(with("screens.0.navigationBarHeight", json!(-1)).is_err());
    assert!(with("screens.0.presentation.cornerRadius", json!(-1)).is_err());
    assert!(with("screens.0.orientations.0.safeArea.width", json!(5000)).is_err(), "safe area wider than the screen");
    assert!(with("screens.0.orientations.0.safeArea.x", json!(-50)).is_err());
    assert!(with("screens.0.orientations.0.cutouts.0.x", json!(1000)).is_err(), "cutout past the right edge");
    assert!(with("screens.0.orientations.1.orientation", json!(1)).is_err(), "portrait twice");
    assert!(with("screens.0.orientations", Value::Array(vec![json!({"orientation": 1, "safeArea": {}}); 9])).is_err());
    assert!(with("screens", Value::Array(vec![unity_fixture()["screens"][0].clone(); 9])).is_err());
    assert!(with("friendlyName", json!(7)).is_err());
}

#[test]
fn clamps_within_tolerance() {
    let mut v = unity_fixture();
    // Half a pixel past the top edge: accepted and clamped.
    v["screens"][0]["orientations"][0]["safeArea"]["height"] = json!(2510.5);
    let s = parse(&bytes(&v), &ParseHint::default()).unwrap().screens.remove(0);
    assert_eq!(s.insets_px(Orientation::Portrait).unwrap().top, 0.0);
}

#[test]
fn native_round_trip() {
    let p = parse_fixture();
    let text = native::to_string(&p).unwrap();
    assert!(text.contains("\"format\": \"figcraft-device\""));
    let q = parse(text.as_bytes(), &ParseHint { file: Some("a.figdevice.json".into()), ..Default::default() }).unwrap();
    assert_eq!(q.screens, p.screens);
    assert_eq!((q.id.as_str(), q.name.as_str()), (p.id.as_str(), p.name.as_str()));
    assert_eq!(q.source, Source::FigCraft { file: Some("a.figdevice.json".into()) });
    let mut newer = native::to_value(&p).unwrap();
    newer["version"] = json!(2);
    assert!(matches!(parse(&bytes(&newer), &ParseHint::default()), Err(DeviceError::Format(_))));
}

#[test]
fn native_hand_written_minimal() {
    let text = br#"{"format": "figcraft-device", "id": "My Devices/Tall Phone!", "name": "Tall phone", "platform": "android",
        "screens": [{"widthPx": 1080, "heightPx": 2400, "scale": 2.75,
          "orientations": [{"orientation": "portrait", "safeArea": {"top": 110, "bottom": 66}}],
          "overlay": {"path": "frames/tall.png"}}]}"#;
    let p = parse(text, &ParseHint { dir: Some("/home/me".into()), ..Default::default() }).unwrap();
    assert_eq!(p.id, "my-devices/tall-phone");
    assert_eq!(p.platform, Platform::Android);
    let s = &p.screens[0];
    assert_eq!(s.scale_source, ScaleSource::File);
    assert_eq!(s.insets_pt(Orientation::Portrait).unwrap().top, 40.0);
    assert_eq!(std::path::Path::new(&s.overlay.as_ref().unwrap().path), std::path::Path::new("/home/me/frames/tall.png"));
}

#[test]
fn orientation_names() {
    for o in Orientation::ALL {
        assert_eq!(Orientation::parse(o.name()), Some(o));
        assert_eq!(Orientation::from_unity(o.unity() as i64), Some(o));
    }
    assert_eq!(Orientation::parse("Landscape-Right"), Some(Orientation::LandscapeRight));
    assert_eq!(Orientation::parse("upside down"), Some(Orientation::PortraitUpsideDown));
    assert_eq!(Orientation::parse("sideways"), None);
    assert_eq!(Orientation::from_unity(0), None);
}

#[test]
fn slugs_and_ids() {
    assert_eq!(slug("Apple iPhone 12 Pro Max"), "apple-iphone-12-pro-max");
    assert_eq!(slug("  ---  "), "device");
    assert_eq!(slug("iPad Pro 12.9\" (2018)"), "ipad-pro-12-9-2018");
    assert_eq!(slug("한국어 기기"), "device");
    assert!(slug(&"a".repeat(500)).len() <= 64);
    assert_eq!(clean_id("../../etc/passwd"), "etc/passwd");
    assert_eq!(clean_id(""), "device");
    assert_eq!(clean_id("a/b/c/d/e/f"), "a/b/c/d");
}

#[test]
fn registry() {
    let mut r = DeviceRegistry::default();
    let p = parse_fixture();
    assert!(!r.insert(p.clone(), true).unwrap());
    assert!(r.insert(p.clone(), true).unwrap(), "replaced");
    assert!(r.insert(p.clone(), false).is_err());
    assert_eq!(r.len(), 1);
    assert!(r.get("unity/test-phone-x1").is_some());
    assert!(r.remove("unity/test-phone-x1"));
    assert!(!r.remove("unity/test-phone-x1"));
    for i in 0..MAX_PROFILES {
        let mut q = p.clone();
        q.id = format!("t/{i}");
        r.insert(q, false).unwrap();
    }
    let mut q = p;
    q.id = "t/over".into();
    assert_eq!(r.insert(q, false), Err(DeviceError::Full));
    assert_eq!(r.clear(), MAX_PROFILES);
}

/// Truncated and corrupted files never panic.
#[test]
fn mangled_files_never_panic() {
    let good = bytes(&unity_fixture());
    for cut in (0..good.len()).step_by(7) {
        let _ = parse(&good[..cut], &ParseHint::default());
    }
    let text = String::from_utf8(good).unwrap();
    for (from, to) in [("1200", "-1"), ("2600", "0.5"), ("90.0", "1e308"), ("\"x\"", "\"q\""), ("[", "{"), ("300.0", "\"300\""), ("1,", "99,")] {
        let _ = parse(text.replace(from, to).as_bytes(), &ParseHint::default());
    }
    let text = native::to_string(&parse_fixture()).unwrap();
    for (from, to) in [("3.0", "0"), ("1200.0", "1e308"), ("portrait", "nope"), ("\"top\"", "\"x\""), ("150.0", "-1e308")] {
        let _ = parse(text.replace(from, to).as_bytes(), &ParseHint::default());
    }
}
