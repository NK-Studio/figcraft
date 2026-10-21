//! Spine skeletons draw their atlas pages posed at the node's animation and time.

use std::collections::BTreeMap;
use std::sync::Arc;

use figcraft_doc::{Document, ImageData, NodeKind, SpineAsset, SpineData};
use figcraft_geom::{Point, Rect};
use figcraft_spine::fixture::{ATLAS, PAGE, SKELETON_JSON};

use crate::spine::texel_to_device;

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];

/// The fixture's page: the body region red, the arm region blue.
fn page() -> Vec<u8> {
    let img = image::RgbaImage::from_fn(PAGE.1, PAGE.2, |x, _| {
        image::Rgba(if x < 32 {
            RED
        } else if x < 48 {
            BLUE
        } else {
            [0; 4]
        })
    });
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
    out
}

fn doc(time: f64, animation: Option<&str>) -> (Document, figcraft_doc::NodeId) {
    let mut d = Document::new("File");
    let bytes = page();
    let hash = format!("spine-test-page-{}", bytes.len());
    d.images.insert(hash.clone(), ImageData { bytes: Arc::new(bytes), width: PAGE.1, height: PAGE.2 });
    // A key of its own per test document: the loaded-skeleton cache is shared.
    let key = format!("spine-test-{time}-{animation:?}");
    d.spine.insert(
        key.clone(),
        SpineAsset {
            name: "figure".into(),
            skeleton: Arc::new(SKELETON_JSON.as_bytes().to_vec()),
            binary: false,
            atlas: ATLAS.into(),
            pages: BTreeMap::from([(PAGE.0.to_string(), hash)]),
        },
    );
    let p = d.first_page().unwrap();
    let data = SpineData {
        asset: key,
        animation: animation.map(str::to_string),
        skin: None,
        time,
        looping: false,
        playing: false,
        speed: 1.0,
        frame: [-16.0, -32.0, 64.0, 32.0],
    };
    let id = d.create(p, NodeKind::Spine(data), "figure", 0.0, 0.0, 64.0, 32.0).unwrap();
    (d, id)
}

fn px(doc: &Document, id: figcraft_doc::NodeId, x: u32, y: u32) -> [u8; 4] {
    let pix = crate::area_at(doc, &[id], Rect::new(0.0, 0.0, 64.0, 32.0), 1.0, 0.0).unwrap();
    let c = pix.pixel(x, y).unwrap();
    [c.red(), c.green(), c.blue(), c.alpha()]
}

#[test]
fn the_setup_pose_draws_body_and_arm() {
    let (d, id) = doc(0.0, None);
    assert_eq!(px(&d, id, 10, 16), RED);
    assert_eq!(px(&d, id, 50, 16), BLUE);
    // Beside the arm, nothing.
    assert_eq!(px(&d, id, 50, 2)[3], 0);
}

#[test]
fn the_animation_moves_the_arm() {
    let (d, id) = doc(1.0, Some("wave"));
    // Raised: no longer out to the right, and the body is unchanged.
    assert_eq!(px(&d, id, 50, 16)[3], 0);
    assert_eq!(px(&d, id, 10, 16), RED);
}

#[test]
fn playing_skeletons_tick_the_clock() {
    let (mut d, id) = doc(0.0, Some("wave"));
    let page = d.first_page().unwrap();
    assert_eq!(crate::images::animation_clock(&d, page, 0.0), None);
    if let NodeKind::Spine(s) = &mut d.node_mut(id).unwrap().kind {
        s.playing = true;
    }
    let (a, next) = crate::images::animation_clock(&d, page, 0.0).unwrap();
    assert!(next > 0.0 && next <= 1.0 / 60.0 + 1e-9);
    let (b, _) = crate::images::animation_clock(&d, page, 0.5).unwrap();
    assert_ne!(a, b);
}

#[test]
fn a_missing_asset_draws_nothing() {
    let (mut d, id) = doc(0.0, None);
    d.spine.clear();
    assert_eq!(px(&d, id, 10, 16)[3], 0);
}

#[test]
fn texel_maps_hit_the_corners() {
    let t = [Point::new(0.0, 0.0), Point::new(10.0, 0.0), Point::new(0.0, 10.0)];
    let d = [Point::new(5.0, 5.0), Point::new(5.0, 25.0), Point::new(-5.0, 5.0)];
    let m = texel_to_device(t, d).unwrap();
    for (a, b) in t.iter().zip(&d) {
        let p = m * *a;
        assert!((p - *b).hypot() < 1e-9);
    }
    assert!(texel_to_device([Point::ORIGIN; 3], d).is_none());
}
